//! Window insets state and callback.
//!
//! The Android `RlobKitMainActivity` calls `nativeOnWindowInsets` via JNI,
//! which feeds into this module.  Framework integrations (e.g. repose-platform)
//! subscribe via [`set_on_insets`] to forward the values into their own layout
//! system.

use std::sync::OnceLock;

/// System bar and IME insets in physical pixels.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowInsets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    pub ime_bottom: f32,
}

static INSETS_CB: OnceLock<Box<dyn Fn(WindowInsets) + Send + Sync>> = OnceLock::new();
static LAST_INSETS: std::sync::Mutex<Option<WindowInsets>> = std::sync::Mutex::new(None);

/// Register a callback invoked on every window-insets change.
///
/// Called from the JNI thread (Java main thread).  The callback **must** be
/// `Send + Sync` and should forward to the UI thread / layout system.
pub fn set_on_insets(cb: Box<dyn Fn(WindowInsets) + Send + Sync>) {
    // Forward the most recent value so the subscriber catches up.
    if let Ok(guard) = LAST_INSETS.lock() {
        if let Some(insets) = *guard {
            cb(insets);
        }
    }
    let _ = INSETS_CB.set(cb);
}

/// Called by the JNI `nativeOnWindowInsets` bridge.
///
/// Stores the latest insets and notifies the registered callback (if any).
pub fn set_window_insets(insets: WindowInsets) {
    *LAST_INSETS.lock().unwrap_or_else(|e| e.into_inner()) = Some(insets);
    if let Some(cb) = INSETS_CB.get() {
        cb(insets);
    }
}

/// Return the last reported window insets, if any.
pub fn last_window_insets() -> Option<WindowInsets> {
    *LAST_INSETS.lock().unwrap_or_else(|e| e.into_inner())
}
