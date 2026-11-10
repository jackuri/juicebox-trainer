//! Guarded MCP server (stdio) for the AI agent (PLAN.md §3, §5.2).
//!
//! The AI touches games only through these tools. They call the same
//! `ot-core` / `ot-packs` / `ot-saves` functions as the app's UI, so the
//! anti-cheat guard (no off switch), byte-checked patches, the crash-safe
//! journal and save backups all apply. Every call is logged and every change
//! is undone by `revert_all`, `detach` or the server exiting.
//!
//! Run as `ot-mcp` (this crate's binary) or `opentrainer-desktop --mcp`.

pub mod extra;
pub mod protocol;
pub mod tools;
pub mod web;

pub use tools::{Config, Tools};

/// Serves MCP on stdin/stdout until stdin closes, then undoes every change.
pub fn serve_stdio(config: Config) {
    let mut tools = Tools::new(config);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    protocol::serve(&mut tools, stdin.lock(), stdout.lock());
    drop(tools);
}
