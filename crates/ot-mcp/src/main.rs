//! `ot-mcp`: the guarded MCP server on stdio. Logs go to stderr.

fn main() {
    tracing_subscriber_init();
    ot_mcp::serve_stdio(ot_mcp::Config::standard());
}

fn tracing_subscriber_init() {
    // stdout carries the protocol; keep logging off it.
    eprintln!(
        "ot-mcp {} (guarded tools for the OpenTrainer AI helper)",
        env!("CARGO_PKG_VERSION")
    );
}
