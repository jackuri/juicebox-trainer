//! Error type for everything in `ot-core`.

use crate::guard::Verdict;
use crate::process::Arch;
use crate::value::ValueType;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The anti-cheat guard refused. There is no way to override this.
    #[error("blocked by the anti-cheat guard: {0}")]
    Blocked(Verdict),

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
            Error::Win32 { .. } => "win32",
        }
    }
}
