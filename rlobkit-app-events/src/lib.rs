//! Shared Android intent capture, window insets, and the shared
//! `RlobKitMainActivity` (Kotlin).
//!
//! ## Intents
//!
//! 1. The Kotlin `RlobKitMainActivity` (or a custom subclass) calls
//!    `RlobKitIntentBridge.captureViewIntent()` which reads the `content://`
//!    URI via `ContentResolver` and writes the raw bytes to
//!    `<filesDir>/pending_intent`.
//!
//! 2. Rust `android_main()` calls [`take_pending_intent`] to grab the initial
//!    intent before the UI starts.
//!
//! 3. For runtime `onNewIntent` intents, poll [`take_pending_intent`] each
//!    frame or subscribe via [`drain_intents`].
//!
//! ## Window insets
//!
//! `RlobKitMainActivity` registers an `OnApplyWindowInsetsListener` that
//! calls `nativeOnWindowInsets` via JNI.  The Rust JNI bridge feeds into
//! the [`insets`] module.  Framework integrations (e.g. repose-platform)
//! call [`insets::set_on_insets`] during init to forward the values into
//! their own layout system.
//!
//! ## Example
//!
//! ```ignore
//! use rlobkit_app_events::AppIntent;
//!
//! fn android_main(android_app: AndroidApp) {
//!     let data_dir = android_app.internal_data_path();
//!     if let Some(dir) = &data_dir {
//!         if let Some(intent) = rlobkit_app_events::take_pending_intent(dir) {
//!             // process intent.data
//!         }
//!     }
//!     // ...
//! }
//! ```

pub mod insets;

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub mod jni;

use std::path::Path;

/// An incoming `ACTION_VIEW` intent.
#[derive(Debug, Clone)]
pub struct AppIntent {
    /// Raw bytes read from the content:// URI.
    pub data: Vec<u8>,
    /// Human-readable label (last segment of the URI, or "Shared file").
    pub name: String,
}

const PENDING_FILE: &str = "pending_intent";

/// Read and remove the `pending_intent` file saved by the Kotlin bridge.
///
/// Call this from `android_main` **before** starting the UI loop to capture
/// the intent that launched the app.  Returns `None` if no intent file
/// exists or if I/O fails.
pub fn take_pending_intent(data_dir: &Path) -> Option<AppIntent> {
    let path = data_dir.join(PENDING_FILE);
    let data = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    if data.is_empty() {
        return None;
    }
    #[cfg(target_os = "android")]
    log::info!(
        "rlobkit_app_events: took pending intent ({} bytes)",
        data.len()
    );
    Some(AppIntent {
        name: "Shared file".into(),
        data,
    })
}

use std::sync::Mutex;

static RUNTIME_QUEUE: Mutex<Option<Vec<AppIntent>>> = Mutex::new(None);

/// Push an intent for processing on the next frame.
///
/// This is safe to call before Rust static initialisers have run (the mutex
/// is lazily allocated).  Typically called from a JNI `nativeOnNewIntent`
/// callback or from a per-frame file poll.
pub fn push_intent(data: Vec<u8>) {
    let mut queue = RUNTIME_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.get_or_insert_with(Vec::new).push(AppIntent {
        name: "Shared file".into(),
        data,
    });
}

/// Drain all runtime intents queued since the last call.
///
/// Call this each frame in the render loop.
pub fn drain_intents() -> Vec<AppIntent> {
    let mut queue = RUNTIME_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.take().unwrap_or_default()
}
