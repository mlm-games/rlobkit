//! Theme palette, pushed from Android through the JNI bridge.
//!
//! `RlobKitMainActivity` can call `nativeOnTheme` with a `byte[]` of packed
//! colors, which feeds into this module.  Framework integrations (e.g.
//! repose-platform) subscribe via [`set_on_theme`] to forward the values into
//! their own styling system, mirroring [`crate::insets`].

use std::sync::OnceLock;

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
#[derive(Debug, Clone, Copy)]
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

static THEME_CB: OnceLock<Box<dyn Fn(ThemeColors) + Send + Sync>> = OnceLock::new();
static LAST_THEME: std::sync::Mutex<Option<ThemeColors>> = std::sync::Mutex::new(None);

/// Register a callback invoked on every theme change.
///
/// Called from the JNI thread (Java main thread).  The callback **must** be
/// `Send + Sync` and should forward to the UI thread / styling system.
pub fn set_on_theme(cb: Box<dyn Fn(ThemeColors) + Send + Sync>) {
    // Forward the most recent value so the subscriber catches up.
    if let Some(t) = LAST_THEME.lock().ok().and_then(|guard| *guard) {
        cb(t);
    }
    let _ = THEME_CB.set(cb);
}

/// Called by the JNI `nativeOnTheme` bridge.
///
/// Stores the latest palette and notifies the registered callback (if any).
pub fn set_theme(colors: ThemeColors) {
    *LAST_THEME.lock().unwrap_or_else(|e| e.into_inner()) = Some(colors);
    if let Some(cb) = THEME_CB.get() {
        cb(colors);
    }
}

/// Return the last reported palette, if any.
pub fn last_theme() -> Option<ThemeColors> {
    *LAST_THEME.lock().unwrap_or_else(|e| e.into_inner())
}
