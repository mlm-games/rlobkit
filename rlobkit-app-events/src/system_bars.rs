use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const STATUS: u8 = 1 << 0;
const NAVIGATION: u8 = 1 << 1;

/// Which system bars are visible. Either bar can be hidden on its own, so a
/// game can drop the status bar and keep the navigation bar reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemBars {
    pub status: bool,
    pub navigation: bool,
}

impl SystemBars {
    pub const fn all(visible: bool) -> Self {
        Self {
            status: visible,
            navigation: visible,
        }
    }

    /// Neither bar visible: the immersive-sticky mode.
    pub const fn is_immersive(self) -> bool {
        !self.status && !self.navigation
    }

    const fn bits(self) -> u8 {
        let mut bits = 0;
        if self.status {
            bits |= STATUS;
        }
        if self.navigation {
            bits |= NAVIGATION;
        }
        bits
    }
}

static VISIBLE: AtomicU8 = AtomicU8::new(STATUS | NAVIGATION);

/// Colour of the icons drawn in the bars that stay visible, which the
/// framework cannot infer from a game-rendered window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarIcons {
    /// White icons, for a dark background.
    Light,
    /// Black icons, for a light background.
    Dark,
}

static LIGHT_ICONS: AtomicBool = AtomicBool::new(true);

/// The system bars the app currently wants visible.
pub fn system_bars() -> SystemBars {
    let bits = VISIBLE.load(Ordering::Relaxed);
    SystemBars {
        status: bits & STATUS != 0,
        navigation: bits & NAVIGATION != 0,
    }
}

/// Show or hide each system bar. On Android this applies right away from any
/// thread and can be called again at any time to toggle while the app runs.
pub fn set_system_bars_visible(bars: SystemBars) {
    VISIBLE.store(bars.bits(), Ordering::Relaxed);
    #[cfg(all(feature = "jni-bridge", target_os = "android"))]
    post_to_activity();
}

/// Hide both system bars and keep them hidden until a swipe reveals them
/// transiently, or show both again with `false`. The Android Activity re-reads
/// the state above whenever it applies, so a call that lands before the
/// Activity exists is not lost.
pub fn set_immersive_sticky(hide: bool) {
    set_system_bars_visible(SystemBars::all(!hide));
}

/// The icon colour currently requested.
pub fn bar_icons() -> BarIcons {
    if LIGHT_ICONS.load(Ordering::Relaxed) {
        BarIcons::Light
    } else {
        BarIcons::Dark
    }
}

/// Match the bar icons to the app's background. Only matters while a bar is
/// visible, but applies immediately like [`set_system_bars_visible`] so a bar
/// revealed later already has the right contrast. Defaults to
/// [`BarIcons::Light`].
pub fn set_bar_icons(icons: BarIcons) {
    LIGHT_ICONS.store(icons == BarIcons::Light, Ordering::Relaxed);
    #[cfg(all(feature = "jni-bridge", target_os = "android"))]
    post_to_activity();
}

/// The Activity owns the window, so only it can change the bar state. It reads
/// [`system_bars`] itself; this just tells the running one to re-apply. A
/// class that is not `RlobKitMainActivity` yields a logged warning instead of
/// the abort that passing the wrong context type would cause.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn post_to_activity() {
    use jni::{jni_sig, jni_str};
    let result = jni_min_helper::jni_with_env(|env| {
        env.call_static_method(
            jni_str!("rust/rlobkit/RlobKitMainActivity"),
            jni_str!("refreshSystemBars"),
            jni_sig!("()V"),
            &[],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        log::warn!("rlobkit_app_events::system_bars: refresh failed: {e}");
    }
}
