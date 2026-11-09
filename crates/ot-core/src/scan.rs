//! Value scanner: first scan (exact / between / unknown) and next scans
//! (exact / between / changed / unchanged / increased / decreased).
//!
//! Memory is split into blocks of at most [`BLOCK_SIZE`] bytes that are scanned
//! in parallel. A block with many surviving slots is kept *dense* (a copy of
//! its bytes plus a bitmap of live slots); one with few is kept *sparse*
//! (offsets plus values). Reads go through [`AttachedProcess`], so the
//! anti-cheat guard applies to every scan.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::process::Arch;
use crate::session::AttachedProcess;
use crate::value::{Value, ValueType};

/// Runs `$f::<T>(args…)` with `T` matching the value type (pointers by arch).
macro_rules! dispatch {
    ($ty:expr, $arch:expr, $f:ident, $($arg:expr),* $(,)?) => {
        match $ty {
            ValueType::I8 => $f::<i8>($($arg),*),
            ValueType::U8 => $f::<u8>($($arg),*),
            ValueType::I16 => $f::<i16>($($arg),*),
            ValueType::U16 => $f::<u16>($($arg),*),
            ValueType::I32 => $f::<i32>($($arg),*),
            ValueType::U32 => $f::<u32>($($arg),*),
            ValueType::I64 => $f::<i64>($($arg),*),
            ValueType::U64 => $f::<u64>($($arg),*),
            ValueType::F32 => $f::<f32>($($arg),*),
            ValueType::F64 => $f::<f64>($($arg),*),
            ValueType::Ptr if $arch == Arch::X86 => $f::<u32>($($arg),*),
            ValueType::Ptr => $f::<u64>($($arg),*),
        }
    };
}

/// Largest block scanned as one unit.
pub const BLOCK_SIZE: u64 = 16 * 1024 * 1024;

/// An unknown-value first scan copies all scanned memory; refuse beyond this.
pub const UNKNOWN_SCAN_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

/// Options fixed for the lifetime of a scan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    /// Only scan writable memory (where game state lives).
    pub writable_only: bool,
    /// Only look at addresses aligned to the value size (much faster).
    pub aligned: bool,
    /// Absolute tolerance for float comparisons; `None` picks a default.
    pub float_tolerance: Option<f64>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            writable_only: true,
            aligned: true,
            float_tolerance: None,
        }
    }
}

/// How to start a scan.
#[derive(Debug, Clone, PartialEq)]
pub enum FirstScan {
    Exact(Value),
    /// Inclusive range; bounds may be given in either order.
    Between(Value, Value),
    /// Every slot is a candidate (snapshot of memory).
    Unknown,
}

/// How to narrow an existing scan.
#[derive(Debug, Clone, PartialEq)]
pub enum NextScan {
    Exact(Value),
    Between(Value, Value),
    Changed,
    Unchanged,
    Increased,
    Decreased,
}

/// Progress and cancellation shared with the UI while a scan runs.
#[derive(Debug, Default)]
pub struct ScanControl {
    cancel: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
}

impl ScanControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the cancel flag and progress. Call before starting a scan.
    pub fn reset(&self) {
        self.cancel.store(false, Ordering::SeqCst);
        self.done.store(0, Ordering::SeqCst);
        self.total.store(0, Ordering::SeqCst);
    }

    /// Asks the running scan to stop. Its previous results are kept.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// `(bytes done, bytes total)` of the running scan.
    pub fn progress(&self) -> (u64, u64) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    pub(crate) fn begin(&self, total: u64) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    pub(crate) fn add(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }
}

/// One scan result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub address: u64,
    /// Value as of the last scan.
    pub value: Value,
}

/// A scan in progress: its candidates and their values from the last round.
#[derive(Debug)]
pub struct Scan {
    ty: ValueType,
    arch: Arch,
    options: ScanOptions,
    blocks: Vec<Block>,
    count: usize,
    rounds: u32,
}

#[derive(Debug, Clone)]
struct Block {
    base: u64,
    count: usize,
    data: BlockData,
}

#[derive(Debug, Clone)]
enum BlockData {
    /// `bytes` copied from the target; slot `i` lives at `i * step`.
    Dense {
        bytes: Vec<u8>,
        alive: Vec<u64>,
        slots: usize,
    },
    /// Surviving offsets (relative to `base`) and their values, `size` bytes each.
    Sparse { offsets: Vec<u32>, values: Vec<u8> },
}

impl Scan {
    /// Runs a first scan over the target's memory.
    pub fn first(
        process: &AttachedProcess,
        ty: ValueType,
        mode: FirstScan,
        options: ScanOptions,
        control: &ScanControl,
    ) -> Result<Scan> {
        let arch = process.arch();
        match &mode {
            FirstScan::Exact(v) => check_type(v, ty)?,
            FirstScan::Between(a, b) => {
                check_type(a, ty)?;
                check_type(b, ty)?;
            }
            FirstScan::Unknown => {}
        }
        let size = ty.size(arch);
        let step = if options.aligned { size } else { 1 };

        let mut jobs = Vec::new();
        for r in process.regions()? {
            if !r.readable || (options.writable_only && !r.writable) {
                continue;
            }
            let mut off = 0u64;
            while off < r.size {
                let cover = BLOCK_SIZE.min(r.size - off);
                // Unaligned scans read a little past the block so values that
                // straddle the boundary are still seen.
                let overlap = if step == 1 {
                    (size as u64 - 1).min(r.size - off - cover)
                } else {
                    0
                };
                jobs.push(FirstJob {
                    base: r.base + off,
                    cover: cover as usize,
                    read_len: (cover + overlap) as usize,
                });
                off += cover;
            }
        }
        let total: u64 = jobs.iter().map(|j| j.read_len as u64).sum();
        if matches!(mode, FirstScan::Unknown) && total > UNKNOWN_SCAN_LIMIT {
            return Err(Error::ScanTooLarge {
                bytes: total,
                limit: UNKNOWN_SCAN_LIMIT,
            });
        }
        control.begin(total);

        let tol = tolerance(ty, options);
        let blocks = dispatch!(
            ty,
            arch,
            first_typed,
            process,
            &jobs,
            &mode,
            arch,
            step,
            tol,
            control
        )?;
        let count = blocks.iter().map(|b| b.count).sum();
        Ok(Scan {
            ty,
            arch,
            options,
            blocks,
            count,
            rounds: 1,
        })
    }

    /// Narrows the scan. On error or cancel the previous results are kept.
    pub fn next(
        &mut self,
        process: &AttachedProcess,
        mode: NextScan,
        control: &ScanControl,
    ) -> Result<()> {
        if process.arch() != self.arch {
            return Err(Error::InvalidValue {
                input: "scan belongs to another process".into(),
                ty: self.ty,
            });
        }
        match &mode {
            NextScan::Exact(v) => check_type(v, self.ty)?,
            NextScan::Between(a, b) => {
                check_type(a, self.ty)?;
                check_type(b, self.ty)?;
            }
            _ => {}
        }
        let size = self.ty.size(self.arch);
        let total: u64 = self.blocks.iter().map(|b| b.read_span(size).1 as u64).sum();
        control.begin(total);

        let tol = tolerance(self.ty, self.options);
        let step = self.step();
        let arch = self.arch;
        let blocks = dispatch!(
            self.ty,
            arch,
            next_typed,
            process,
            &self.blocks,
            &mode,
            arch,
            step,
            tol,
            control
        )?;
        self.count = blocks.iter().map(|b| b.count).sum();
        self.blocks = blocks;
        self.rounds += 1;
        Ok(())
    }

    /// Narrows a copy of this scan that reads its slots as `ty`, a type of the
    /// same size (an unknown-value scan as `i32` can go on as `f32`). This
    /// scan is left as it was.
    pub fn next_as(
        &self,
        process: &AttachedProcess,
        ty: ValueType,
        mode: NextScan,
        control: &ScanControl,
    ) -> Result<Scan> {
        if ty.size(self.arch) != self.ty.size(self.arch) {
            return Err(Error::InvalidValue {
                input: format!("a {} scan read as {ty}", self.ty),
                ty,
            });
        }
        let mut copy = Scan {
            ty,
            arch: self.arch,
            options: self.options,
            blocks: Vec::new(),
            count: self.count,
            rounds: self.rounds,
        };
        // Borrow the blocks for the narrowing instead of cloning them.
        let size = ty.size(self.arch);
        match &mode {
            NextScan::Exact(v) => check_type(v, ty)?,
            NextScan::Between(a, b) => {
                check_type(a, ty)?;
                check_type(b, ty)?;
            }
            _ => {}
        }
        let total: u64 = self.blocks.iter().map(|b| b.read_span(size).1 as u64).sum();
        control.begin(total);
        let tol = tolerance(ty, self.options);
        let step = self.step();
        let arch = self.arch;
        copy.blocks = dispatch!(
            ty,
            arch,
            next_typed,
            process,
            &self.blocks,
            &mode,
            arch,
            step,
            tol,
            control
        )?;
        copy.count = copy.blocks.iter().map(|b| b.count).sum();
        copy.rounds += 1;
        Ok(copy)
    }

    pub fn value_type(&self) -> ValueType {
        self.ty
    }

    pub fn options(&self) -> ScanOptions {
        self.options
    }

    /// Number of candidates left.
    pub fn count(&self) -> usize {
        self.count
    }

    /// How many scans (first + next) have run.
    pub fn rounds(&self) -> u32 {
        self.rounds
    }

    /// Approximate memory held by the results.
    pub fn memory_bytes(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| match &b.data {
                BlockData::Dense { bytes, alive, .. } => bytes.len() + alive.len() * 8,
                BlockData::Sparse { offsets, values } => offsets.len() * 4 + values.len(),
            })
            .sum()
    }

    fn step(&self) -> usize {
        if self.options.aligned {
            self.ty.size(self.arch)
        } else {
            1
        }
    }

    /// Up to `limit` hits starting at `offset`, in address order.
    pub fn hits(&self, offset: usize, limit: usize) -> Vec<Hit> {
        let size = self.ty.size(self.arch);
        let step = self.step();
        let mut skip = offset;
        let mut out = Vec::with_capacity(limit.min(self.count));
        for b in &self.blocks {
            if out.len() >= limit {
                break;
            }
            if skip >= b.count {
                skip -= b.count;
                continue;
            }
            let mut slots_iter: Box<dyn Iterator<Item = (usize, &[u8])>> = match &b.data {
                BlockData::Dense {
                    bytes,
                    alive,
                    slots,
                } => Box::new(
                    (0..*slots)
                        .filter(|&i| bit(alive, i))
                        .map(move |i| (i * step, &bytes[i * step..i * step + size])),
                ),
                BlockData::Sparse { offsets, values } => Box::new(
                    offsets
                        .iter()
                        .enumerate()
                        .map(move |(k, &off)| (off as usize, &values[k * size..(k + 1) * size])),
                ),
            };
            if skip > 0 {
                slots_iter = Box::new(slots_iter.skip(skip));
                skip = 0;
            }
            for (off, bytes) in slots_iter {
                if out.len() >= limit {
                    break;
                }
                if let Some(value) = Value::from_bytes(self.ty, bytes, self.arch) {
                    out.push(Hit {
                        address: b.base + off as u64,
                        value,
                    });
                }
            }
        }
        out
    }
}

impl Block {
    /// `(offset, len)` of the bytes a next scan must re-read.
    fn read_span(&self, size: usize) -> (usize, usize) {
        match &self.data {
            BlockData::Dense { bytes, .. } => (0, bytes.len()),
            BlockData::Sparse { offsets, .. } => {
                let first = offsets.first().copied().unwrap_or(0) as usize;
                let last = offsets.last().copied().unwrap_or(0) as usize;
                (first, last + size - first)
            }
        }
    }
}

fn check_type(v: &Value, ty: ValueType) -> Result<()> {
    if v.ty() == ty {
        Ok(())
    } else {
        Err(Error::InvalidValue {
            input: v.to_string(),
            ty,
        })
    }
}

fn tolerance(ty: ValueType, options: ScanOptions) -> f64 {
    match ty {
        ValueType::F32 => options.float_tolerance.unwrap_or(1e-3).abs(),
        ValueType::F64 => options.float_tolerance.unwrap_or(1e-6).abs(),
        _ => 0.0,
    }
}

// ----- typed core ---------------------------------------------------------

/// A fixed-size little-endian scalar we can compare.
trait Scalar: Copy + PartialOrd + Send + Sync + 'static {
    const SIZE: usize;
    const FLOAT: bool;
    fn from_le(b: &[u8]) -> Self;
    fn to_f64(self) -> f64;
    fn is_nan(self) -> bool {
        false
    }
}

macro_rules! int_scalar {
    ($($t:ty),*) => {$(
        impl Scalar for $t {
            const SIZE: usize = size_of::<$t>();
            const FLOAT: bool = false;
            fn from_le(b: &[u8]) -> Self {
                let mut a = [0u8; size_of::<$t>()];
                a.copy_from_slice(&b[..size_of::<$t>()]);
                <$t>::from_le_bytes(a)
            }
            fn to_f64(self) -> f64 {
                self as f64
            }
        }
    )*};
}
int_scalar!(i8, u8, i16, u16, i32, u32, i64, u64);

macro_rules! float_scalar {
    ($($t:ty),*) => {$(
        impl Scalar for $t {
            const SIZE: usize = size_of::<$t>();
            const FLOAT: bool = true;
            fn from_le(b: &[u8]) -> Self {
                let mut a = [0u8; size_of::<$t>()];
                a.copy_from_slice(&b[..size_of::<$t>()]);
                <$t>::from_le_bytes(a)
            }
            fn to_f64(self) -> f64 {
                f64::from(self)
            }
            fn is_nan(self) -> bool {
                <$t>::is_nan(self)
            }
        }
    )*};
}
float_scalar!(f32, f64);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cond<T> {
    Any,
    Exact(T),
    Between(T, T),
    Changed,
    Unchanged,
    Increased,
    Decreased,
}

fn scalar<T: Scalar>(v: &Value, arch: Arch) -> T {
    T::from_le(&v.to_bytes(arch))
}

fn between<T: Scalar>(a: T, b: T) -> Cond<T> {
    if b < a {
        Cond::Between(b, a)
    } else {
        Cond::Between(a, b)
    }
}

fn first_cond<T: Scalar>(mode: &FirstScan, arch: Arch) -> Cond<T> {
    match mode {
        FirstScan::Exact(v) => Cond::Exact(scalar(v, arch)),
        FirstScan::Between(a, b) => between(scalar(a, arch), scalar(b, arch)),
        FirstScan::Unknown => Cond::Any,
    }
}

fn next_cond<T: Scalar>(mode: &NextScan, arch: Arch) -> Cond<T> {
    match mode {
        NextScan::Exact(v) => Cond::Exact(scalar(v, arch)),
        NextScan::Between(a, b) => between(scalar(a, arch), scalar(b, arch)),
        NextScan::Changed => Cond::Changed,
        NextScan::Unchanged => Cond::Unchanged,
        NextScan::Increased => Cond::Increased,
        NextScan::Decreased => Cond::Decreased,
    }
}

#[inline]
fn test<T: Scalar>(cond: Cond<T>, cur: T, prev: Option<T>, tol: f64) -> bool {
    if cur.is_nan() {
        return false;
    }
    let p = || prev.filter(|p| !p.is_nan());
    if T::FLOAT {
        let c = cur.to_f64();
        match cond {
            Cond::Any => true,
            Cond::Exact(t) => (c - t.to_f64()).abs() <= tol,
            Cond::Between(lo, hi) => c >= lo.to_f64() - tol && c <= hi.to_f64() + tol,
            Cond::Changed => p().is_some_and(|p| (c - p.to_f64()).abs() > tol),
            Cond::Unchanged => p().is_some_and(|p| (c - p.to_f64()).abs() <= tol),
            Cond::Increased => p().is_some_and(|p| c - p.to_f64() > tol),
            Cond::Decreased => p().is_some_and(|p| p.to_f64() - c > tol),
        }
    } else {
        match cond {
            Cond::Any => true,
            Cond::Exact(t) => cur == t,
            Cond::Between(lo, hi) => lo <= cur && cur <= hi,
            Cond::Changed => p().is_some_and(|p| cur != p),
            Cond::Unchanged => p().is_some_and(|p| cur == p),
            Cond::Increased => p().is_some_and(|p| cur > p),
            Cond::Decreased => p().is_some_and(|p| cur < p),
        }
    }
}

fn bit(bits: &[u64], i: usize) -> bool {
    bits[i / 64] & (1 << (i % 64)) != 0
}

fn set_bit(bits: &mut [u64], i: usize) {
    bits[i / 64] |= 1 << (i % 64);
}

/// Number of slots in a block covering `cover` bytes with `len` bytes read.
fn slot_count(cover: usize, len: usize, step: usize, size: usize) -> usize {
    if cover == 0 || len < size {
        return 0;
    }
    let max_off = (len - size).min(cover - 1);
    max_off / step + 1
}

/// First-scan test of one block of bytes. Returns the live-slot bitmap and count.
fn first_block<T: Scalar>(
    bytes: &[u8],
    cover: usize,
    step: usize,
    cond: Cond<T>,
    tol: f64,
) -> (Vec<u64>, usize, usize) {
    let slots = slot_count(cover, bytes.len(), step, T::SIZE);
    let mut alive = vec![0u64; slots.div_ceil(64)];
    let mut count = 0;
    for i in 0..slots {
        let off = i * step;
        if test(cond, T::from_le(&bytes[off..]), None, tol) {
            set_bit(&mut alive, i);
            count += 1;
        }
    }
    (alive, count, slots)
}

/// Next-scan test of a dense block against freshly read bytes.
fn next_dense<T: Scalar>(
    prev: &[u8],
    fresh: &[u8],
    alive: &[u64],
    slots: usize,
    step: usize,
    cond: Cond<T>,
    tol: f64,
) -> (Vec<u64>, usize) {
    let mut out = vec![0u64; alive.len()];
    let mut count = 0;
    for i in (0..slots).filter(|&i| bit(alive, i)) {
        let off = i * step;
        let cur = T::from_le(&fresh[off..]);
        let old = T::from_le(&prev[off..]);
        if test(cond, cur, Some(old), tol) {
            set_bit(&mut out, i);
            count += 1;
        }
    }
    (out, count)
}

/// Next-scan test of a sparse block. `fresh` starts at `span_start`.
fn next_sparse<T: Scalar>(
    offsets: &[u32],
    values: &[u8],
    fresh: &[u8],
    span_start: usize,
    cond: Cond<T>,
    tol: f64,
) -> (Vec<u32>, Vec<u8>) {
    let size = T::SIZE;
    let mut new_offsets = Vec::new();
    let mut new_values = Vec::new();
    for (k, &off) in offsets.iter().enumerate() {
        let rel = off as usize - span_start;
        let cur_bytes = &fresh[rel..rel + size];
        let cur = T::from_le(cur_bytes);
        let old = T::from_le(&values[k * size..]);
        if test(cond, cur, Some(old), tol) {
            new_offsets.push(off);
            new_values.extend_from_slice(cur_bytes);
        }
    }
    (new_offsets, new_values)
}

/// Picks the cheaper representation for a block's survivors.
fn compact(
    base: u64,
    bytes: Vec<u8>,
    alive: Vec<u64>,
    slots: usize,
    count: usize,
    step: usize,
    size: usize,
) -> Option<Block> {
    if count == 0 {
        return None;
    }
    let dense_cost = bytes.len() + alive.len() * 8;
    let sparse_cost = count * (4 + size);
    let data = if sparse_cost < dense_cost {
        let mut offsets = Vec::with_capacity(count);
        let mut values = Vec::with_capacity(count * size);
        for i in (0..slots).filter(|&i| bit(&alive, i)) {
            let off = i * step;
            offsets.push(off as u32);
            values.extend_from_slice(&bytes[off..off + size]);
        }
        BlockData::Sparse { offsets, values }
    } else {
        BlockData::Dense {
            bytes,
            alive,
            slots,
        }
    };
    Some(Block { base, count, data })
}

#[derive(Debug, Clone, Copy)]
struct FirstJob {
    base: u64,
    cover: usize,
    read_len: usize,
}

/// Reads target memory; unreadable memory (freed, protection changed) is
/// skipped, but a guard block or exited process stops the scan.
pub(crate) fn read_or_skip(
    p: &AttachedProcess,
    address: u64,
    len: usize,
) -> Result<Option<Vec<u8>>> {
    match p.read_bytes(address, len) {
        Ok(b) => Ok(Some(b)),
        Err(e @ (Error::Blocked(_) | Error::ProcessExited { .. })) => Err(e),
        Err(_) => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
fn first_typed<T: Scalar>(
    p: &AttachedProcess,
    jobs: &[FirstJob],
    mode: &FirstScan,
    arch: Arch,
    step: usize,
    tol: f64,
    control: &ScanControl,
) -> Result<Vec<Block>> {
    let cond = first_cond::<T>(mode, arch);
    parallel(jobs, control, |job| {
        let bytes = read_or_skip(p, job.base, job.read_len)?;
        control.add(job.read_len as u64);
        let Some(bytes) = bytes else { return Ok(None) };
        let (alive, count, slots) = first_block::<T>(&bytes, job.cover, step, cond, tol);
        Ok(compact(job.base, bytes, alive, slots, count, step, T::SIZE))
    })
}

#[allow(clippy::too_many_arguments)]
fn next_typed<T: Scalar>(
    p: &AttachedProcess,
    blocks: &[Block],
    mode: &NextScan,
    arch: Arch,
    step: usize,
    tol: f64,
    control: &ScanControl,
) -> Result<Vec<Block>> {
    let cond = next_cond::<T>(mode, arch);
    parallel(blocks, control, |block| {
        let (start, len) = block.read_span(T::SIZE);
        let fresh = read_or_skip(p, block.base + start as u64, len)?;
        control.add(len as u64);
        let Some(fresh) = fresh else { return Ok(None) };
        Ok(match &block.data {
            BlockData::Dense {
                bytes,
                alive,
                slots,
            } => {
                let (alive, count) = next_dense::<T>(bytes, &fresh, alive, *slots, step, cond, tol);
                compact(block.base, fresh, alive, *slots, count, step, T::SIZE)
            }
            BlockData::Sparse { offsets, values } => {
                let (offsets, values) = next_sparse::<T>(offsets, values, &fresh, start, cond, tol);
                if offsets.is_empty() {
                    None
                } else {
                    Some(Block {
                        base: block.base,
                        count: offsets.len(),
                        data: BlockData::Sparse { offsets, values },
                    })
                }
            }
        })
    })
}

/// Runs `f` over `items` on all cores, keeping input order in the output.
pub(crate) fn parallel<I, O, F>(items: &[I], control: &ScanControl, f: F) -> Result<Vec<O>>
where
    I: Sync,
    O: Send,
    F: Fn(&I) -> Result<Option<O>> + Sync,
{
    let next = std::sync::atomic::AtomicUsize::new(0);
    let fatal: Mutex<Option<Error>> = Mutex::new(None);
    let stop = AtomicBool::new(false);
    let workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(items.len().max(1));

    let mut results: Vec<(usize, O)> = thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut local = Vec::new();
                    loop {
                        if stop.load(Ordering::Relaxed) || control.is_cancelled() {
                            break;
                        }
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else { break };
                        match f(item) {
                            Ok(Some(o)) => local.push((i, o)),
                            Ok(None) => {}
                            Err(e) => {
                                stop.store(true, Ordering::Relaxed);
                                fatal
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .get_or_insert(e);
                                break;
                            }
                        }
                    }
                    local
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });

    if let Some(e) = fatal.into_inner().unwrap_or_else(|e| e.into_inner()) {
        return Err(e);
    }
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    results.sort_by_key(|(i, _)| *i);
    Ok(results.into_iter().map(|(_, o)| o).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_i32s(vals: &[i32]) -> Vec<u8> {
        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn alive_slots(bits: &[u64], slots: usize) -> Vec<usize> {
        (0..slots).filter(|&i| bit(bits, i)).collect()
    }

    #[test]
    fn slot_counts() {
        assert_eq!(slot_count(16, 16, 4, 4), 4);
        assert_eq!(slot_count(16, 19, 1, 4), 16);
        assert_eq!(slot_count(16, 16, 1, 4), 13);
        assert_eq!(slot_count(0, 16, 4, 4), 0);
        assert_eq!(slot_count(16, 3, 4, 4), 0);
    }

    #[test]
    fn exact_aligned_and_unaligned() {
        let bytes = le_i32s(&[7, 250, 3, 250]);
        let (alive, count, slots) = first_block::<i32>(&bytes, 16, 4, Cond::Exact(250), 0.0);
        assert_eq!(count, 2);
        assert_eq!(alive_slots(&alive, slots), vec![1, 3]);

        // 250 at byte offset 1 is only visible unaligned.
        let mut b = vec![0u8; 8];
        b[1..5].copy_from_slice(&250i32.to_le_bytes());
        let (_, aligned, _) = first_block::<i32>(&b, 8, 4, Cond::Exact(250), 0.0);
        let (a, unaligned, s) = first_block::<i32>(&b, 8, 1, Cond::Exact(250), 0.0);
        assert_eq!(aligned, 0);
        assert_eq!(unaligned, 1);
        assert_eq!(alive_slots(&a, s), vec![1]);
    }

    #[test]
    fn between_accepts_reversed_bounds() {
        let bytes = le_i32s(&[1, 5, 10, 20]);
        let cond = between::<i32>(12, 4);
        let (alive, count, slots) = first_block::<i32>(&bytes, 16, 4, cond, 0.0);
        assert_eq!(count, 2);
        assert_eq!(alive_slots(&alive, slots), vec![1, 2]);
    }

    #[test]
    fn float_tolerance_and_nan() {
        let vals = [1.0f32, 1.0004, 1.01, f32::NAN];
        let bytes: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        let (alive, count, slots) = first_block::<f32>(&bytes, 16, 4, Cond::Exact(1.0), 1e-3);
        assert_eq!(count, 2);
        assert_eq!(alive_slots(&alive, slots), vec![0, 1]);
        let (_, any, _) = first_block::<f32>(&bytes, 16, 4, Cond::Any, 1e-3);
        assert_eq!(any, 3, "NaN slots are never candidates");
    }

    #[test]
    fn next_dense_conditions() {
        let prev = le_i32s(&[10, 10, 10, 10]);
        let fresh = le_i32s(&[10, 11, 9, 10]);
        let all = vec![0b1111u64];
        let run = |c| alive_slots(&next_dense::<i32>(&prev, &fresh, &all, 4, 4, c, 0.0).0, 4);
        assert_eq!(run(Cond::Changed), vec![1, 2]);
        assert_eq!(run(Cond::Unchanged), vec![0, 3]);
        assert_eq!(run(Cond::Increased), vec![1]);
        assert_eq!(run(Cond::Decreased), vec![2]);
        assert_eq!(run(Cond::Exact(9)), vec![2]);
        // Only live slots are considered.
        let some = vec![0b0001u64];
        let (bits, n) = next_dense::<i32>(&prev, &fresh, &some, 4, 4, Cond::Unchanged, 0.0);
        assert_eq!((alive_slots(&bits, 4), n), (vec![0], 1));
    }

    #[test]
    fn next_sparse_keeps_new_values() {
        let offsets = [4u32, 12];
        let values = le_i32s(&[5, 5]);
        // Fresh span starts at offset 4: [x@4, pad, pad, y@12]
        let fresh = le_i32s(&[6, 0, 5]);
        let (o, v) = next_sparse::<i32>(&offsets, &values, &fresh, 4, Cond::Increased, 0.0);
        assert_eq!(o, vec![4]);
        assert_eq!(v, 6i32.to_le_bytes().to_vec());
    }

    #[test]
    fn compact_prefers_sparse_for_few_hits() {
        let bytes = vec![0u8; 4096];
        let mut alive = vec![0u64; 1024 / 64];
        set_bit(&mut alive, 3);
        let b = compact(0x1000, bytes.clone(), alive, 1024, 1, 4, 4).unwrap();
        assert!(matches!(b.data, BlockData::Sparse { ref offsets, .. } if offsets == &[12]));

        let all = vec![u64::MAX; 1024 / 64];
        let b = compact(0x1000, bytes, all, 1024, 1024, 4, 4).unwrap();
        assert!(matches!(b.data, BlockData::Dense { .. }));
        assert!(compact(0, vec![0; 16], vec![0], 4, 0, 4, 4).is_none());
    }

    #[test]
    fn hits_paginate_across_blocks() {
        let dense_bytes = le_i32s(&[1, 2, 3, 4]);
        let scan = Scan {
            ty: ValueType::I32,
            arch: Arch::X64,
            options: ScanOptions::default(),
            blocks: vec![
                Block {
                    base: 0x1000,
                    count: 2,
                    data: BlockData::Dense {
                        bytes: dense_bytes,
                        alive: vec![0b1010],
                        slots: 4,
                    },
                },
                Block {
                    base: 0x9000,
                    count: 2,
                    data: BlockData::Sparse {
                        offsets: vec![0, 8],
                        values: le_i32s(&[7, 8]),
                    },
                },
            ],
            count: 4,
            rounds: 1,
        };
        let addrs = |o, l| {
            scan.hits(o, l)
                .iter()
                .map(|h| h.address)
                .collect::<Vec<_>>()
        };
        assert_eq!(addrs(0, 10), vec![0x1004, 0x100C, 0x9000, 0x9008]);
        assert_eq!(addrs(1, 2), vec![0x100C, 0x9000]);
        assert_eq!(addrs(3, 5), vec![0x9008]);
        assert_eq!(addrs(4, 5), Vec::<u64>::new());
        assert_eq!(scan.hits(2, 1)[0].value, Value::I32(7));
    }

    #[test]
    fn parallel_keeps_order_and_reports_cancel() {
        let ctl = ScanControl::new();
        let items: Vec<u32> = (0..1000).collect();
        let out = parallel(&items, &ctl, |&i| Ok((i % 3 == 0).then_some(i))).unwrap();
        assert_eq!(out, (0..1000).filter(|i| i % 3 == 0).collect::<Vec<_>>());

        ctl.cancel();
        assert!(matches!(
            parallel(&items, &ctl, |&i| Ok(Some(i))),
            Err(Error::Cancelled)
        ));
    }
}
