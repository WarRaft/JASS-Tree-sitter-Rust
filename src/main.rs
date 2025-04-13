mod lsp;

use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};

#[derive(Serialize, Deserialize, Debug)]
struct InitializeParams {
    process_id: Option<i64>,
    root_path: Option<String>,
    capabilities: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct InitializeResult {
    capabilities: Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct Response<T> {
    jsonrpc: String,
    id: Value,
    result: Option<T>,
    error: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Request {
    jsonrpc: String,
    id: Value,
    method: String,
    params: Option<Value>,
}

fn read_lsp_message<R: BufRead>(reader: &mut R) -> Option<String> {
    let mut content_length = 0;
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None; // stdin закрыт
        }
        if line == "\r\n" {
            break;
        } else if let Some(cl) = line.strip_prefix("Content-Length:") {
            content_length = cl.trim().parse::<usize>().ok()?;
        }
    }

    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).ok()?;
    Some(String::from_utf8(body).ok()?)
}

fn write_lsp_message<W: Write>(writer: &mut W, message: &Value) {
    let msg = message.to_string();
    write!(writer, "Content-Length: {}\r\n\r\n", msg.len()).unwrap();
    writer.write_all(msg.as_bytes()).unwrap();
    writer.flush().unwrap();
}

fn main() {
    env_logger::init();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    info!("LSP server started");

    while let Some(msg) = read_lsp_message(&mut reader) {
        info!("<<< {}", msg);

        let request: Request = match serde_json::from_str(&msg) {
            Ok(req) => req,
            Err(err) => {
                error!("Failed to parse request: {}", err);
                continue;
            }
        };

        match request.method.as_str() {
            "initialize" => {
                let result = InitializeResult {
                    capabilities: json!({}),
                };

                let response = Response {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(result),
                    error: None,
                };

                let value = serde_json::to_value(response).unwrap();
                info!(">>> {}", value);
                write_lsp_message(&mut writer, &value);
            }
            "shutdown" => {
                let response = Response {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(json!(null)),
                    error: None,
                };
                let value = serde_json::to_value(response).unwrap();
                info!(">>> {}", value);
                write_lsp_message(&mut writer, &value);
                break;
            }
            "exit" => {
                info!("Received 'exit' method. Exiting.");
                break;
            }
            _ => {
                let response: Response<Value> = Response {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(json!({
                        "code": -32601,
                        "message": "Method not found"
                    })),
                };
                let value = serde_json::to_value(response).unwrap();
                info!(">>> {}", value);
                write_lsp_message(&mut writer, &value);
            }
        }
    }

    info!("LSP server shutting down (stdin closed)");
    std::process::exit(0);
}
