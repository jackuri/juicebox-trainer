//! OpenTrainer core: process list, attach/detach, typed memory read/write,
//! module map and the anti-cheat guard. See `docs/PLAN.md` §3.
//!
//! The only way to get access to another process's memory is
//! [`AttachedProcess::attach`], which runs the [`guard`] first and refuses if
//! anti-cheat is present. The guard has no off switch.

pub mod address;
pub mod aob;
pub mod capture;
pub mod code;
pub mod dotnet;
pub mod engine;
pub mod error;
pub mod finder;
pub mod freeze;
pub mod guard;
pub mod hash;
pub mod il2cpp;
pub mod journal;
pub mod memory;
pub mod modules;
pub mod pointer;
pub mod process;
pub mod scan;
pub mod session;
pub mod signature;
pub mod unreal;
pub mod value;
mod win;
pub mod writes;

pub use aob::Pattern;
pub use error::{Error, Result};
pub use finder::{FindNext, FindStart, FoundValue, ValueFinder};
pub use freeze::Freezer;
pub use guard::{Finding, FindingKind, Verdict};
pub use memory::Region;
pub use modules::Module;
pub use process::{Arch, ProcessInfo, list_processes, memory_in_use};
pub use scan::{FirstScan, Hit, NextScan, Scan, ScanControl, ScanOptions};
pub use session::AttachedProcess;
pub use value::{Value, ValueType};

/// Version of this crate (the workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_semver() {
        let parts: Vec<_> = super::VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()));
    }
}
