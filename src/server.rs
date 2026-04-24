/// Syma Notebook Server — HTTP + WebSocket frontend for the Syma evaluator.
///
/// Serves a notebook UI (CodeMirror 6 with WL syntax highlighting) and
/// accepts evaluation requests over WebSocket.
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

use crate::kernel::SymaKernel;

/// Shared application state.
struct AppState {
    kernel: Arc<RwLock<SymaKernel>>,
}

/// Start the notebook server on the given host and port.
///
/// Blocks the current thread (runs the tokio runtime).
pub fn start_server(host: &str, port: u16) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async move {
        let kernel = SymaKernel::new();
        let state = Arc::new(AppState {
            kernel: Arc::new(RwLock::new(kernel)),
        });

        let app = Router::new()
            .route("/ws", get(ws_handler))
            .nest_service("/modules", ServeDir::new("notebook/modules"))
            .nest_service("/", ServeDir::new("nb"))
            .with_state(state);

        let addr = format!("{}:{}", host, port);
        println!("Syma Notebook Server listening on http://{}", addr);
        println!("Open http://{} in your browser.", addr);

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .expect("Failed to bind address");

        axum::serve(listener, app).await.expect("Server error");
    });
}

/// WebSocket upgrade handler.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle a single WebSocket connection.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Send ready message on connect
    let ready = serde_json::json!({
        "type": "ready",
        "version": env!("CARGO_PKG_VERSION")
    });
    let _ = sender.send(Message::Text(ready.to_string())).await;

    // Process incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let response = handle_message(&text, &state).await;
                if let Some(resp) = response
                    && sender.send(Message::Text(resp)).await.is_err()
                {
                    break; // connection closed
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

/// Handle a single JSON message from the frontend.
async fn handle_message(text: &str, state: &Arc<AppState>) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(text).ok()?;
    let msg_type = parsed.get("type")?.as_str()?;

    match msg_type {
        "eval" => {
            let code = parsed.get("code")?.as_str().unwrap_or("");
            let id = parsed.get("id").and_then(|v| v.as_str()).unwrap_or("");

            let result = {
                let kernel = state.kernel.read().await;
                kernel.eval(code)
            };

            let response = if result.success {
                serde_json::json!({
                    "type": "result",
                    "id": id,
                    "success": true,
                    "output": result.output,
                    "value": result.value,
                    "timing_ms": result.timing_ms
                })
            } else {
                serde_json::json!({
                    "type": "error",
                    "id": id,
                    "success": false,
                    "error": result.error.unwrap_or_default(),
                    "timing_ms": result.timing_ms
                })
            };

            Some(response.to_string())
        }
        _ => {
            // Unknown message type
            let response = serde_json::json!({
                "type": "error",
                "error": format!("Unknown message type: {}", msg_type)
            });
            Some(response.to_string())
        }
    }
}
