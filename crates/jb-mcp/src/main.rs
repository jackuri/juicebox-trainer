//! `jb-mcp`: the guarded MCP server on stdio. Logs go to stderr.

fn main() {
    tracing_subscriber_init();
    jb_mcp::serve_stdio(jb_mcp::Config::standard());
}

fn tracing_subscriber_init() {
    // stdout carries the protocol; keep logging off it.
    eprintln!(
        "jb-mcp {} (guarded tools for the JuiceBox Trainer AI helper)",
        env!("CARGO_PKG_VERSION")
    );
}
