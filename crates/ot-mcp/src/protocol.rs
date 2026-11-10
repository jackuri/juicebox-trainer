//! The Model Context Protocol over stdio: newline-delimited JSON-RPC 2.0.
//! Only what a tools-only server needs: `initialize`, `ping`, `tools/list`
//! and `tools/call`.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::tools::Tools;

/// Protocol version answered when the client asks for one we don't know.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Handles one message. Returns the reply, or `None` for notifications.
pub fn handle(tools: &mut Tools, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let Some(id) = id else {
        // Notifications (initialized, cancelled) need no reply.
        return None;
    };
    if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error(id, -32600, "not a JSON-RPC 2.0 request"));
    }
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            let version = if KNOWN_VERSIONS.contains(&asked) {
                asked
            } else {
                PROTOCOL_VERSION
            };
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "ot-mcp", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": crate::tools::INSTRUCTIONS,
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": tools.definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let content = match tools.call(name, &args) {
                // A picture (game_screenshot): the image, then what it is.
                Ok(mut v) if v.get(crate::tools::IMAGE_KEY).is_some() => {
                    let png = v
                        .as_object_mut()
                        .and_then(|o| o.remove(crate::tools::IMAGE_KEY))
                        .unwrap_or_default();
                    json!([
                        { "type": "image", "data": png, "mimeType": "image/png" },
                        { "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() },
                    ])
                }
                Ok(v) => {
                    json!([{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }])
                }
                Err(e) => {
                    return Some(ok(
                        id,
                        json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                    ));
                }
            };
            ok(id, json!({ "content": content, "isError": false }))
        }
        _ => error(id, -32601, &format!("unknown method {method}")),
    })
}

/// Serves requests from `input` until it closes.
pub fn serve(tools: &mut Tools, input: impl BufRead, mut output: impl Write) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => handle(tools, &msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("bad JSON: {e}"))),
        };
        if let Some(reply) = reply {
            let _ = writeln!(output, "{reply}");
            let _ = output.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(tools: &mut Tools, msg: Value) -> Value {
        handle(tools, &msg).unwrap()
    }

    #[test]
    fn initializes_and_lists_tools() {
        let mut t = Tools::for_tests();
        let r = call(
            &mut t,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                   "params": {"protocolVersion": "2025-03-26", "capabilities": {}}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "ot-mcp");
        assert!(
            handle(
                &mut t,
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .is_none()
        );
        let r = call(
            &mut t,
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        );
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"attach") && names.contains(&"pack_write"));
        for tool in r["result"]["tools"].as_array().unwrap() {
            assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
        }
    }

    #[test]
    fn tool_errors_and_unknown_methods() {
        let mut t = Tools::for_tests();
        let r = call(
            &mut t,
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                   "params": {"name": "read", "arguments": {"address": "0x10", "type": "i32"}}}),
        );
        assert_eq!(r["result"]["isError"], true);
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("attach")
        );
        let r = call(&mut t, json!({"jsonrpc": "2.0", "id": 4, "method": "nope"}));
        assert_eq!(r["error"]["code"], -32601);
        let r = call(&mut t, json!({"id": 5, "method": "ping"}));
        assert_eq!(r["error"]["code"], -32600);
    }

    #[test]
    fn serves_lines() {
        let mut t = Tools::for_tests();
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\nnot json\n";
        let mut out = Vec::new();
        serve(&mut t, &input[..], &mut out);
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"result\":{}"));
        assert!(lines[1].contains("-32700"));
    }
}
