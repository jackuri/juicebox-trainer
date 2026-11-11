//! Pointer scanner: finds pointer paths from a module (a *static* address,
//! fixed relative to the exe or a DLL) to a value, Cheat Engine style:
//! `[[game.exe+0x1A2B0]+0x10]+0x18`.
//!
//! A value found with the value scanner usually lives on the heap and moves
//! every time the game starts. A pointer path from a module stays valid, so
//! it can go into a cheat pack. The workflow:
//!
//! 1. Find the value's address with the scanner.
//! 2. [`scan`] for paths to it.
//! 3. Restart the game, find the value again and [`filter`] the paths with
//!    the new address. The paths that survive a few restarts are the stable
//!    ones.
//!
//! [`scan`] reads every readable region once to build a map of all
//! pointer-sized values that point into the target's memory, then walks it
//! backwards from the target, shortest paths first. All reads go through
//! [`AttachedProcess`], so the anti-cheat guard applies.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::memory::Region;
use crate::modules::Module;
use crate::process::Arch;
use crate::scan::{BLOCK_SIZE, ScanControl, parallel, read_or_skip};
use crate::session::AttachedProcess;

/// Most entries the pointer map may hold (16 bytes each).
pub const POINTER_MAP_LIMIT: u64 = 48_000_000;

/// Deepest path searched.
pub const MAX_DEPTH: u8 = 7;

/// Largest offset per step.
pub const MAX_OFFSET: u32 = 0x10_0000;

/// Visits before the search gives up (keeps a bad case from running forever).
const VISIT_BUDGET: u64 = 20_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerScanOptions {
    /// Most pointer reads in a path (1–7).
    pub max_depth: u8,
    /// Largest offset added after each read.
    pub max_offset: u32,
    /// Stop after this many paths.
    pub max_results: usize,
}

impl Default for PointerScanOptions {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_offset: 0x1000,
            max_results: 5000,
        }
    }
}

/// A path `[[module+module_offset]+o1]+o2…`: read a pointer at the static
/// address, add `o1`, read a pointer there, add `o2`, and so on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerPath {
    pub module: String,
    pub module_offset: u64,
    pub offsets: Vec<u64>,
}

impl PointerPath {
    /// The static start, e.g. `game.exe+0x1A2B0`.
    pub fn base(&self) -> String {
        format!("{}+{:#X}", self.module, self.module_offset)
    }

    /// Where the path leads now.
    pub fn resolve(&self, process: &AttachedProcess) -> Result<u64> {
        let base = process.resolve(&self.base())?;
        let offsets: Vec<i64> = self.offsets.iter().map(|&o| o as i64).collect();
        process.resolve_chain(base, &offsets)
    }
}

impl fmt::Display for PointerPath {
    /// Cheat Engine notation, accepted by [`crate::address::parse`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = self.base();
        for o in &self.offsets {
            s = format!("[{s}]+{o:#X}");
        }
        f.write_str(&s)
    }
}

/// Every pointer-sized value in readable memory that points into readable
/// memory: `(value, where it is stored)`, sorted by value.
#[derive(Debug)]
struct PointerMap {
    entries: Vec<(u64, u64)>,
}

struct Job {
    base: u64,
    len: usize,
}

fn build_map(
    process: &AttachedProcess,
    regions: &[Region],
    control: &ScanControl,
) -> Result<PointerMap> {
    let arch = process.arch();
    let size = arch.pointer_size();
    let readable: Vec<&Region> = regions.iter().filter(|r| r.readable).collect();
    let mut jobs = Vec::new();
    for r in &readable {
        let mut off = 0;
        while off < r.size {
            let len = BLOCK_SIZE.min(r.size - off);
            jobs.push(Job {
                base: r.base + off,
                len: len as usize,
            });
            off += len;
        }
    }
    control.begin(jobs.iter().map(|j| j.len as u64).sum());
    let spans: Vec<(u64, u64)> = readable.iter().map(|r| (r.base, r.base + r.size)).collect();
    let points_into = |v: u64| {
        // Object addresses are at least 4-byte aligned in practice.
        if !v.is_multiple_of(4) {
            return false;
        }
        let i = spans.partition_point(|s| s.0 <= v);
        i > 0 && v < spans[i - 1].1
    };
    let count = AtomicU64::new(0);
    let parts = parallel(&jobs, control, |job| {
        let bytes = read_or_skip(process, job.base, job.len)?;
        control.add(job.len as u64);
        let Some(bytes) = bytes else { return Ok(None) };
        let mut local = Vec::new();
        for (i, chunk) in bytes.chunks_exact(size).enumerate() {
            let v = match arch {
                Arch::X86 => u64::from(u32::from_le_bytes(chunk.try_into().expect("4 bytes"))),
                _ => u64::from_le_bytes(chunk.try_into().expect("8 bytes")),
            };
            if points_into(v) {
                local.push((v, job.base + (i * size) as u64));
            }
        }
        let total = count.fetch_add(local.len() as u64, Ordering::Relaxed) + local.len() as u64;
        if total > POINTER_MAP_LIMIT {
            return Err(Error::PointerMapTooLarge {
                entries: total,
                limit: POINTER_MAP_LIMIT,
            });
        }
        Ok(Some(local))
    })?;
    let mut entries: Vec<(u64, u64)> = parts.into_iter().flatten().collect();
    entries.sort_unstable();
    Ok(PointerMap { entries })
}

struct Search<'a> {
    map: &'a [(u64, u64)],
    modules: &'a [Module],
    options: PointerScanOptions,
    control: &'a ScanControl,
    offsets: Vec<u64>,
    results: Vec<PointerPath>,
    budget: u64,
}

impl Search<'_> {
    fn module_of(&self, address: u64) -> Option<&Module> {
        let i = self.modules.partition_point(|m| m.base <= address);
        (i > 0)
            .then(|| &self.modules[i - 1])
            .filter(|m| m.contains(address))
    }

    fn full(&self) -> bool {
        self.results.len() >= self.options.max_results
            || self.budget == 0
            || self.control.is_cancelled()
    }

    /// Records paths of exactly `limit` steps ending at `target`.
    fn walk(&mut self, target: u64, depth: u8, limit: u8) {
        let lo = target.saturating_sub(u64::from(self.options.max_offset));
        let start = self.map.partition_point(|e| e.0 < lo);
        for &(value, at) in &self.map[start..] {
            if value > target || self.full() {
                return;
            }
            self.budget -= 1;
            self.offsets.push(target - value);
            match self.module_of(at) {
                Some(m) => {
                    if depth == limit {
                        self.results.push(PointerPath {
                            module: m.name.clone(),
                            module_offset: at - m.base,
                            offsets: self.offsets.iter().rev().copied().collect(),
                        });
                    }
                }
                None => {
                    if depth < limit {
                        self.walk(at, depth + 1, limit);
                    }
                }
            }
            self.offsets.pop();
        }
    }
}

fn check_options(o: &PointerScanOptions) -> Result<()> {
    if !(1..=MAX_DEPTH).contains(&o.max_depth) {
        return Err(Error::InvalidValue {
            input: o.max_depth.to_string(),
            ty: crate::ValueType::U8,
        });
    }
    if o.max_offset == 0 || o.max_offset > MAX_OFFSET || o.max_results == 0 {
        return Err(Error::InvalidValue {
            input: format!("{:#X}", o.max_offset),
            ty: crate::ValueType::U32,
        });
    }
    Ok(())
}

/// Finds pointer paths from any loaded module to `target`, shortest first.
/// Paths starting in the main exe come before DLLs of the same length.
pub fn scan(
    process: &AttachedProcess,
    target: u64,
    options: PointerScanOptions,
    control: &ScanControl,
) -> Result<Vec<PointerPath>> {
    check_options(&options)?;
    let regions = process.regions()?;
    let mut modules = process.refresh_modules()?;
    modules.sort_by_key(|m| m.base);
    let map = build_map(process, &regions, control)?;
    let mut search = Search {
        map: &map.entries,
        modules: &modules,
        options,
        control,
        offsets: Vec::new(),
        results: Vec::new(),
        budget: VISIT_BUDGET,
    };
    for limit in 1..=options.max_depth {
        search.walk(target, 1, limit);
        if search.full() {
            break;
        }
    }
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let main = process.info().name.clone();
    let mut results = search.results;
    results.sort_by_key(|p| {
        (
            p.offsets.len(),
            !p.module.eq_ignore_ascii_case(&main),
            p.offsets.iter().sum::<u64>(),
        )
    });
    Ok(results)
}

/// Keeps the paths that lead to `target` now (after a restart, or after the
/// value moved). A guard block or exited game is an error; other read
/// failures just drop the path.
pub fn filter(
    process: &AttachedProcess,
    paths: &[PointerPath],
    target: u64,
) -> Result<Vec<PointerPath>> {
    let mut out = Vec::new();
    for p in paths {
        match p.resolve(process) {
            Ok(a) if a == target => out.push(p.clone()),
            Ok(_) => {}
            Err(e @ (Error::Blocked(_) | Error::ProcessExited { .. })) => return Err(e),
            Err(_) => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(name: &str, base: u64, size: u32) -> Module {
        Module {
            name: name.into(),
            path: String::new(),
            base,
            size,
        }
    }

    fn search(map: &[(u64, u64)], modules: &[Module], depth: u8) -> Vec<PointerPath> {
        let control = ScanControl::new();
        let mut map = map.to_vec();
        map.sort_unstable();
        let mut s = Search {
            map: &map,
            modules,
            options: PointerScanOptions {
                max_depth: depth,
                ..Default::default()
            },
            control: &control,
            offsets: Vec::new(),
            results: Vec::new(),
            budget: 1000,
        };
        for limit in 1..=depth {
            s.walk(0x9000_0010, 1, limit);
        }
        s.results
    }

    #[test]
    fn walks_back_to_static_addresses_shortest_first() {
        let modules = [module("game.exe", 0x40_0000, 0x10_0000)];
        // game.exe+0x2000 -> 0x8000_0000; [0x8000_0000+0x18] -> 0x9000_0000;
        // target 0x9000_0010. Also a direct static pointer at +0x3000.
        let map = [
            (0x8000_0000, 0x40_2000),
            (0x9000_0000, 0x8000_0018),
            (0x9000_0008, 0x40_3000),
            // A heap loop that never reaches a module.
            (0x7000_0000, 0x7000_0000),
        ];
        let r = search(&map, &modules, 3);
        assert_eq!(r.len(), 2, "{r:?}");
        assert_eq!(r[0].to_string(), "[game.exe+0x3000]+0x8");
        assert_eq!(r[1].to_string(), "[[game.exe+0x2000]+0x18]+0x10");
        assert_eq!(search(&map, &modules, 1).len(), 1);
    }

    #[test]
    fn offsets_respect_the_limit() {
        let modules = [module("game.exe", 0x40_0000, 0x10_0000)];
        // 0x9000_0010 - 0x8FFF_0000 = 0x10010 > 0x1000
        let r = search(&[(0x8FFF_0000, 0x40_2000)], &modules, 2);
        assert!(r.is_empty());
    }

    #[test]
    fn display_is_parseable() {
        let p = PointerPath {
            module: "game.exe".into(),
            module_offset: 0x1A2B0,
            offsets: vec![0x10, 0x0],
        };
        assert_eq!(p.to_string(), "[[game.exe+0x1A2B0]+0x10]+0x0");
        assert!(crate::address::parse(&p.to_string()).is_ok());
    }

    #[test]
    fn options_are_checked() {
        let bad = PointerScanOptions {
            max_depth: 0,
            ..Default::default()
        };
        assert!(check_options(&bad).is_err());
        let bad = PointerScanOptions {
            max_offset: MAX_OFFSET + 1,
            ..Default::default()
        };
        assert!(check_options(&bad).is_err());
        assert!(check_options(&PointerScanOptions::default()).is_ok());
    }
}
