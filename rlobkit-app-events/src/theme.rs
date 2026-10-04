//! Theme palette, pushed from Android through the JNI bridge.
//!
//! `RlobKitMainActivity` can call `nativeOnTheme` with a `byte[]` of packed
//! colors, which feeds into this module.  Framework integrations (e.g.
//! repose-platform) subscribe via [`set_on_theme`] to forward the values into
//! their own styling system, mirroring [`crate::insets`].
//!
//! Whether the system is light or dark is **not** here: winit already reports
//! that as `Window::system_theme()`, and every app using this crate runs on winit.
//! What winit has no notion of is the wallpaper-derived palette, which is a
//! Material 3 concept Android only publishes from Android 12 — hence
//! [`dynamic_colors_available`], which distinguishes "this Android cannot do
//! dynamic color" from "nothing has arrived yet".

use crate::subscriber::Subscriber;
use std::sync::Mutex;

/// Number of colors carried in a theme packet.
///
/// Matches the field order of `repose_core::locals::ColorScheme`:
/// primary, on_primary, primary_container, on_primary_container,
/// secondary, on_secondary, secondary_container, on_secondary_container,
/// tertiary, on_tertiary, tertiary_container, on_tertiary_container,
/// error, on_error, error_container, on_error_container,
/// background, on_background, surface, on_surface, surface_variant,
/// on_surface_variant, surface_container_lowest, surface_container_low,
/// surface_container, surface_container_high, surface_container_highest,
/// surface_bright, surface_dim, surface_tint,
/// inverse_surface, inverse_on_surface, inverse_primary,
/// outline, outline_variant, scrim, shadow, focus.
pub const THEME_COLOR_COUNT: usize = 38;

/// A resolved UI palette, one RGBA quad per Material 3 role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub rgba: [[u8; 4]; THEME_COLOR_COUNT],
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            rgba: [[0u8; 4]; THEME_COLOR_COUNT],
        }
    }
}

impl ThemeColors {
    /// Pack as 38 little-endian RGBA bytes, ready to hand to the native side.
    pub fn as_bytes(&self) -> [u8; THEME_COLOR_COUNT * 4] {
        let mut out = [0u8; THEME_COLOR_COUNT * 4];
        for (i, px) in self.rgba.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(px);
        }
        out
    }

    /// Parse the inverse of [`Self::as_bytes`]. Returns `None` on bad length.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != THEME_COLOR_COUNT * 4 {
            return None;
        }
        let mut rgba = [[0u8; 4]; THEME_COLOR_COUNT];
        for (i, chunk) in bytes.chunks_exact(4).enumerate() {
            rgba[i].copy_from_slice(chunk);
        }
        Some(Self { rgba })
    }
}

static SUBSCRIBER: Subscriber<ThemeColors> = Subscriber::new();
static LAST_THEME: Mutex<Option<ThemeColors>> = Mutex::new(None);
static DYNAMIC_AVAILABLE: Mutex<bool> = Mutex::new(false);

/// Register a callback invoked on every theme change.
///
/// Replaces any previous subscriber and first replays the current palette if one
/// has arrived, so a late subscriber catches up. Called from the JNI thread
/// (Java main thread).  The callback **must** be `Send + Sync` and should forward
/// to the UI thread / styling system.
pub fn set_on_theme(cb: impl Fn(ThemeColors) + Send + Sync + 'static) {
    let current = last_theme();
    SUBSCRIBER.set(cb);
    if let Some(current) = current {
        SUBSCRIBER.notify(current);
    }
}

/// Stores the latest palette and notifies the registered callback (if any).
pub fn set_theme(colors: ThemeColors) {
    *LAST_THEME.lock().unwrap_or_else(|e| e.into_inner()) = Some(colors);
    *DYNAMIC_AVAILABLE.lock().unwrap_or_else(|e| e.into_inner()) = true;
    SUBSCRIBER.notify(colors);
}

/// Return the last reported palette, if any.
///
/// `None` means Android is older than 12, the shared Activity is not in use, or
/// nothing has been pushed yet. [`dynamic_colors_available`] tells those apart.
pub fn last_theme() -> Option<ThemeColors> {
    *LAST_THEME.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether this Android version publishes a wallpaper-derived palette at all,
/// which needs Android 12 or later.
///
/// `false` before the first push, so treat it as "not known yet" rather than as a
/// definite answer. It says nothing about whether a palette has *arrived*: for
/// that, compare against [`last_theme`].
pub fn dynamic_colors_available() -> bool {
    *DYNAMIC_AVAILABLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Records whether this Android version publishes a palette, independently of
/// whether one has arrived. Called by the JNI `nativeOnTheme` bridge.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub(crate) fn set_dynamic_colors_available(available: bool) {
    *DYNAMIC_AVAILABLE.lock().unwrap_or_else(|e| e.into_inner()) = available;
}
