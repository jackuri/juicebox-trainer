//! Error type for everything in `ot-core`.

use crate::guard::Verdict;
use crate::process::Arch;
use crate::value::ValueType;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("cancelled")]
    Cancelled,

    #[error(
        "an unknown-value scan would copy {bytes} bytes (limit {limit}); start with an exact or \
         range scan instead"
    )]
    ScanTooLarge { bytes: u64, limit: u64 },

    /// The anti-cheat guard refused. There is no way to override this.
    #[error("blocked by the anti-cheat guard: {0}")]
    Blocked(Box<Verdict>),

    #[error(
        "access denied to process {pid}. If the game runs as administrator, restart OpenTrainer \
         as administrator. Protected system processes cannot be opened."
    )]
    AccessDenied { pid: u32 },

    #[error("process {pid} not found")]
    ProcessNotFound { pid: u32 },

    #[error("process {pid} has exited")]
    ProcessExited { pid: u32 },

    #[error("could not read {len} byte(s) at {address:#X}")]
    Read { address: u64, len: usize },

    #[error("could not write {len} byte(s) at {address:#X}")]
    Write { address: u64, len: usize },

    #[error("address {address:#X} (+{len} bytes) is outside the {arch} address space")]
    AddressOutOfRange {
        address: u64,
        len: usize,
        arch: Arch,
    },

    #[error("invalid address '{0}': use 0x1234ABCD or module.exe+0x1234")]
    InvalidAddress(String),

    #[error("module '{0}' is not loaded in the target")]
    ModuleNotFound(String),

    #[error("invalid {ty} value '{input}'")]
    InvalidValue { input: String, ty: ValueType },

    #[error("invalid byte pattern {0}")]
    InvalidPattern(String),

    /// Code bytes failed the safety check in [`crate::code::check_patch`].
    #[error("patch rejected: {0}")]
    PatchRejected(String),

    #[error("null pointer read at {at:#X} while following a pointer chain")]
    NullPointer { at: u64 },

    /// The bytes at a code site are not what was expected (usually a game
    /// update). Nothing was written.
    #[error("needs repair: expected {expected} at {address:#X}, found {found}")]
    BytesMismatch {
        address: u64,
        expected: String,
        found: String,
    },

    /// Another change already owns some of these bytes.
    #[error(
        "the code at {address:#X} overlaps a change already made at {other:#X}; turn that          cheat off first"
    )]
    CodeOverlap { address: u64, other: u64 },

    #[error(
        "the pointer map would hold {entries} entries (limit {limit}); close other programs in          the game or lower the search depth"
    )]
    PointerMapTooLarge { entries: u64, limit: u64 },

    #[error("no unique signature found at {address:#X} within {len} bytes; try a nearby address")]
    SignatureNotUnique { address: u64, len: usize },

    #[error("restore journal: {0}")]
    Journal(String),

    /// Watching for writes (debugger) failed.
    #[error("{0}")]
    Debugger(String),

    #[error("{op} failed: {source}")]
    Win32 {
        op: &'static str,
        #[source]
        source: windows::core::Error,
    },
}

impl Error {
    pub(crate) fn win32(op: &'static str, source: windows::core::Error) -> Self {
        Self::Win32 { op, source }
    }

    /// Short machine-readable kind, used by the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Cancelled => "cancelled",
            Error::ScanTooLarge { .. } => "scanTooLarge",
            Error::Blocked(_) => "blocked",
            Error::AccessDenied { .. } => "accessDenied",
            Error::ProcessNotFound { .. } => "processNotFound",
            Error::ProcessExited { .. } => "processExited",
            Error::Read { .. } => "read",
            Error::Write { .. } => "write",
            Error::AddressOutOfRange { .. } => "addressOutOfRange",
            Error::InvalidAddress(_) => "invalidAddress",
            Error::ModuleNotFound(_) => "moduleNotFound",
            Error::InvalidValue { .. } => "invalidValue",
            Error::InvalidPattern(_) => "invalidPattern",
            Error::PatchRejected(_) => "patchRejected",
            Error::NullPointer { .. } => "nullPointer",
            Error::BytesMismatch { .. } => "needsRepair",
            Error::CodeOverlap { .. } => "codeOverlap",
            Error::PointerMapTooLarge { .. } => "pointerMapTooLarge",
            Error::SignatureNotUnique { .. } => "signatureNotUnique",
            Error::Journal(_) => "journal",
            Error::Debugger(_) => "debugger",
            Error::Win32 { .. } => "win32",
        }
    }
}
