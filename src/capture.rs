use evdev::{AbsoluteAxisType, Device, EventStream, InputEventKind, Key, RelativeAxisType};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::processor::RawEvent;

// ---------------------------------------------------------------------------
// Device discovery
// ---------------------------------------------------------------------------

/// Spawns one tokio task per relevant input device found under /dev/input/.
/// Each task reads events and forwards them via `tx`.
///
/// Returns the number of devices that were successfully opened and spawned.
pub async fn spawn_capture_tasks(tx: mpsc::Sender<RawEvent>) -> usize {
    let mut count = 0;
    for (_path, device) in evdev::enumerate() {
        let kind = classify(&device);
        if kind == DeviceKind::None {
            continue;
        }
        let name = device
            .name()
            .unwrap_or("unknown")
            .to_string();
        let tx = tx.clone();
        match device.into_event_stream() {
            Ok(stream) => {
                debug!("Capturing {kind:?} device: {name}");
                tokio::spawn(read_device(stream, kind, tx));
                count += 1;
            }
            Err(e) => {
                warn!("Could not open event stream for {name}: {e}");
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Per-device event loop
// ---------------------------------------------------------------------------

async fn read_device(mut stream: EventStream, kind: DeviceKind, tx: mpsc::Sender<RawEvent>) {
    // Accumulate relative moves within one EV_SYN frame.
    let mut pending_dx: i32 = 0;
    let mut pending_dy: i32 = 0;
    let mut has_rel = false;

    loop {
        let ev = match stream.next_event().await {
            Ok(e) => e,
            Err(e) => {
                warn!("Event stream error: {e}");
                break;
            }
        };

        match ev.kind() {
            InputEventKind::RelAxis(code) => {
                match code {
                    RelativeAxisType::REL_X => {
                        pending_dx += ev.value();
                        has_rel = true;
                    }
                    RelativeAxisType::REL_Y => {
                        pending_dy += ev.value();
                        has_rel = true;
                    }
                    RelativeAxisType::REL_WHEEL => {
                        let _ = tx
                            .send(RawEvent::Scroll {
                                dx: 0,
                                dy: ev.value(),
                            })
                            .await;
                    }
                    RelativeAxisType::REL_HWHEEL => {
                        let _ = tx
                            .send(RawEvent::Scroll {
                                dx: ev.value(),
                                dy: 0,
                            })
                            .await;
                    }
                    _ => {}
                }
            }
            InputEventKind::AbsAxis(code) => {
                let event = match code {
                    AbsoluteAxisType::ABS_X => Some(RawEvent::MouseAbsX(ev.value())),
                    AbsoluteAxisType::ABS_Y => Some(RawEvent::MouseAbsY(ev.value())),
                    _ => None,
                };
                if let Some(e) = event {
                    let _ = tx.send(e).await;
                }
            }
            InputEventKind::Key(_) if kind == DeviceKind::Keyboard && ev.value() == 1 => {
                // Key-down only; key identity is discarded here.
                let _ = tx.send(RawEvent::Keystroke).await;
            }
            InputEventKind::Synchronization(_) => {
                if has_rel {
                    let _ = tx
                        .send(RawEvent::MouseRelMove {
                            dx: pending_dx,
                            dy: pending_dy,
                        })
                        .await;
                    pending_dx = 0;
                    pending_dy = 0;
                    has_rel = false;
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Device classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceKind {
    Mouse,
    Keyboard,
    Both,
    None,
}

fn classify(device: &Device) -> DeviceKind {
    let is_mouse = device
        .supported_relative_axes()
        .map_or(false, |axes| axes.contains(RelativeAxisType::REL_X))
        || device
            .supported_absolute_axes()
            .map_or(false, |axes| axes.contains(AbsoluteAxisType::ABS_X));

    // Only treat as keyboard if it has standard letter keys (avoids game pads
    // and media remotes that expose a handful of KEY_* codes).
    let is_keyboard = device
        .supported_keys()
        .map_or(false, |keys| keys.contains(Key::KEY_A));

    match (is_mouse, is_keyboard) {
        (true, true) => DeviceKind::Both,
        (true, false) => DeviceKind::Mouse,
        (false, true) => DeviceKind::Keyboard,
        (false, false) => DeviceKind::None,
    }
}
