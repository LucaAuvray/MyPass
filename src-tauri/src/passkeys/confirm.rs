//! In-app confirmation of a passkey request: an event opens the dialog
//! (`PasskeyRequestDialog`), the `respond_passkey` command answers it. Called
//! from the bridge's own threads, so it blocks on a std channel (no tokio).

use super::Prompt;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tauri::Emitter;

static PENDING: LazyLock<Mutex<HashMap<String, Sender<Option<String>>>>> = LazyLock::new(Default::default);

/// Shows the dialog and waits for the answer: the chosen account id, or
/// `None` (cancelled, closed, timed out, or no app window to ask in).
pub fn ask(prompt: &Prompt) -> Option<String> {
    let app = crate::native_messaging::app_handle()?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut payload = serde_json::to_value(prompt).ok()?;
    payload["requestId"] = request_id.clone().into();
    let choice = wait(&request_id, prompt.timeout, || {
        let _ = app.emit("passkey-request", payload);
    });
    // Closes the dialog if it is still open (timeout).
    let _ = app.emit("passkey-request-closed", serde_json::json!({ "requestId": request_id }));
    choice
}

/// On lock: answers every waiting request "cancelled", so the extension falls
/// back at once instead of waiting out its timeout behind a locked vault.
pub fn cancel_all() {
    for (_, tx) in PENDING.lock().unwrap().drain() {
        let _ = tx.send(None);
    }
}

pub fn respond(request_id: &str, choice: Option<String>) {
    if let Some(tx) = PENDING.lock().unwrap().remove(request_id) {
        let _ = tx.send(choice);
    }
}

/// Registers the request before `announce` shows it, so an instant answer
/// is not lost, then waits up to `timeout`.
fn wait(request_id: &str, timeout: Duration, announce: impl FnOnce()) -> Option<String> {
    let (tx, rx) = channel();
    PENDING.lock().unwrap().insert(request_id.to_string(), tx);
    announce();
    let choice = rx.recv_timeout(timeout).ok().flatten();
    PENDING.lock().unwrap().remove(request_id);
    choice
}

#[cfg(test)]
mod tests {
    use super::*;

    // PENDING is global and cancel_all() empties it: these tests run one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn respond_delivers_choice() {
        let _serial = serial();
        let h = std::thread::spawn(|| wait("r1", Duration::from_secs(5), || {}));
        while !PENDING.lock().unwrap().contains_key("r1") {
            std::thread::sleep(Duration::from_millis(5));
        }
        respond("r1", Some("e1".into()));
        assert_eq!(h.join().unwrap(), Some("e1".into()));
    }

    #[test]
    fn timeout_returns_none_and_forgets_request() {
        let _serial = serial();
        assert_eq!(wait("r2", Duration::from_millis(50), || {}), None);
        assert!(!PENDING.lock().unwrap().contains_key("r2"));
        respond("r2", Some("late".into())); // must not panic
    }

    #[test]
    fn cancel_all_releases_waiting_requests() {
        let _serial = serial();
        let h =std::thread::spawn(|| wait("r3", Duration::from_secs(5), || {}));
        while !PENDING.lock().unwrap().contains_key("r3") {
            std::thread::sleep(Duration::from_millis(5));
        }
        let start = std::time::Instant::now();
        cancel_all();
        assert_eq!(h.join().unwrap(), None);
        assert!(start.elapsed() < Duration::from_secs(1), "released at once, not at the timeout");
    }
}
