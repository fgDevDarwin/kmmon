use std::sync::Arc;

use anyhow::Result;
use foxglove::{ChannelBuilder, RawChannel, Schema, WebSocketServer};

// ---------------------------------------------------------------------------
// Channel schemas (JSON Schema / jsonschema encoding)
// ---------------------------------------------------------------------------

const MOUSE_POSITION_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "x": {"type": "integer"},
    "y": {"type": "integer"}
  },
  "required": ["x", "y"]
}"#;

const MOUSE_SCROLL_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "dx": {"type": "integer"},
    "dy": {"type": "integer"}
  },
  "required": ["dx", "dy"]
}"#;

const KEYBOARD_ACTIVITY_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "keystrokes_per_minute": {"type": "integer", "minimum": 0},
    "approx_wpm": {"type": "number", "minimum": 0},
    "active": {"type": "boolean"}
  },
  "required": ["keystrokes_per_minute", "approx_wpm", "active"]
}"#;

const MOUSE_ACTIVITY_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "pixels_per_second": {"type": "number", "minimum": 0},
    "active": {"type": "boolean"}
  },
  "required": ["pixels_per_second", "active"]
}"#;

// ---------------------------------------------------------------------------
// Channel bundle
// ---------------------------------------------------------------------------

pub struct Channels {
    pub mouse_position: Arc<RawChannel>,
    pub mouse_scroll: Arc<RawChannel>,
    pub mouse_activity: Arc<RawChannel>,
    pub keyboard_activity: Arc<RawChannel>,
}

/// Registers all kmmon channels with the global foxglove context.
/// Both the WebSocket server and any active MCAP writer will receive messages
/// published on these channels.
pub fn create_channels() -> Result<Channels> {
    Ok(Channels {
        mouse_position: build_channel(
            "/mouse/position",
            "MousePosition",
            MOUSE_POSITION_SCHEMA,
        )?,
        mouse_scroll: build_channel("/mouse/scroll", "MouseScroll", MOUSE_SCROLL_SCHEMA)?,
        mouse_activity: build_channel(
            "/mouse/activity",
            "MouseActivity",
            MOUSE_ACTIVITY_SCHEMA,
        )?,
        keyboard_activity: build_channel(
            "/keyboard/activity",
            "KeyboardActivity",
            KEYBOARD_ACTIVITY_SCHEMA,
        )?,
    })
}

fn build_channel(topic: &str, schema_name: &str, schema_json: &'static str) -> Result<Arc<RawChannel>> {
    let schema = Schema::new(schema_name, "jsonschema", schema_json.as_bytes());
    let ch = ChannelBuilder::new(topic)
        .schema(schema)
        .message_encoding("json")
        .build_raw()?;
    Ok(ch)
}

// ---------------------------------------------------------------------------
// WebSocket server wrapper
// ---------------------------------------------------------------------------

pub struct WsServer {
    handle: foxglove::WebSocketServerHandle,
}

impl WsServer {
    /// Starts the Foxglove WebSocket server and registers it as a global sink.
    pub async fn start(host: &str, port: u16) -> Result<Self> {
        let handle = WebSocketServer::new()
            .name("kmmon")
            .bind(host, port)
            .start()
            .await?;
        Ok(Self { handle })
    }

    /// Gracefully stops the server.
    pub fn stop(self) {
        let _ = self.handle.stop();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_channels_succeeds() {
        // Note: this test will fail with DuplicateChannel if run after another
        // test in the same process has already registered these topics. Each
        // test binary gets a fresh process so this is fine for `cargo test`.
        let ch = create_channels().unwrap();
        assert_eq!(ch.mouse_position.topic(), "/mouse/position");
        assert_eq!(ch.mouse_scroll.topic(), "/mouse/scroll");
        assert_eq!(ch.mouse_activity.topic(), "/mouse/activity");
        assert_eq!(ch.keyboard_activity.topic(), "/keyboard/activity");
    }

    #[tokio::test]
    async fn ws_server_starts_and_stops() {
        // Use a non-default port to avoid conflicts with a running kmmon instance.
        let server = WsServer::start("127.0.0.1", 18765).await.unwrap();
        server.stop();
    }
}
