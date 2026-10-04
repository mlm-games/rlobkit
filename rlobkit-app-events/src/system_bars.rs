use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const STATUS: u8 = 1 << 0;
const NAVIGATION: u8 = 1 << 1;
const EDGE_TO_EDGE_DISABLED: u8 = 0;
const EDGE_TO_EDGE_ENABLED: u8 = 1;
const EDGE_TO_EDGE_IMMERSIVE: u8 = 2;

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

/// How the window relates to the system bars.
///
/// This replaces the previous coupling where drawing behind the bars happened
/// only when *both* were hidden, so showing one bar silently reflowed the whole
/// layout. The three cases are now separate decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeToEdgeMode {
    /// The window is inset by the system bars; they take no part in the app's
    /// own layout. Only sensible for a window that is not full screen, such as a
    /// dialog or a floating window.
    Disabled,
    /// Draw behind the bars while they stay visible, and rely on the insets from
    /// [`crate::insets`] to keep content clear of them. The usual choice: the
    /// background runs to the screen edges, and only content moves.
    ///
    /// Android 15 (API 35) enforces this for apps targeting API 35 or later, so
    /// on a modern target it is the behaviour regardless of this setting.
    Enabled,
    /// Draw behind the bars and hide both, so nothing overlaps them at all.
    /// Hiding them is what distinguishes this from [`Enabled`](Self::Enabled).
    Immersive,
}

impl EdgeToEdgeMode {
    pub(crate) const fn to_wire(self) -> u8 {
        match self {
            Self::Disabled => EDGE_TO_EDGE_DISABLED,
            Self::Enabled => EDGE_TO_EDGE_ENABLED,
            Self::Immersive => EDGE_TO_EDGE_IMMERSIVE,
        }
    }

    const fn from_wire(value: u8) -> Self {
        match value {
            EDGE_TO_EDGE_ENABLED => Self::Enabled,
            EDGE_TO_EDGE_IMMERSIVE => Self::Immersive,
            _ => Self::Disabled,
        }
    }

    /// Whether both bars should be hidden.
    pub const fn hides_bars(self) -> bool {
        matches!(self, Self::Immersive)
    }
}

static VISIBLE: AtomicU8 = AtomicU8::new(STATUS | NAVIGATION);
static EDGE_TO_EDGE: AtomicU8 = AtomicU8::new(EdgeToEdgeMode::Enabled.to_wire());

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
///
/// Read back by [`EdgeToEdgeMode`]'s effect: when the mode is
/// [`Immersive`](EdgeToEdgeMode::Immersive) both bars are hidden regardless of
/// this value.
pub fn system_bars() -> SystemBars {
    let bits = VISIBLE.load(Ordering::Relaxed);
    SystemBars {
        status: bits & STATUS != 0,
        navigation: bits & NAVIGATION != 0,
    }
}

/// Show or hide each system bar. On Android this applies right away from any
/// thread and can be called again at any time to toggle while the app runs.
///
/// Ignored while the edge-to-edge mode is
/// [`Immersive`](EdgeToEdgeMode::Immersive), which hides both bars.
pub fn set_system_bars_visible(bars: SystemBars) {
    VISIBLE.store(bars.bits(), Ordering::Relaxed);
    apply_to_window();
}

/// The current edge-to-edge mode. Defaults to
/// [`Enabled`](EdgeToEdgeMode::Enabled).
pub fn edge_to_edge() -> EdgeToEdgeMode {
    EdgeToEdgeMode::from_wire(EDGE_TO_EDGE.load(Ordering::Relaxed))
}

/// Choose how the window relates to the system bars. Applies right away, like
/// [`set_system_bars_visible`].
pub fn set_edge_to_edge(mode: EdgeToEdgeMode) {
    EDGE_TO_EDGE.store(mode.to_wire(), Ordering::Relaxed);
    apply_to_window();
}

/// Hide both system bars and keep them hidden until a swipe reveals them
/// transiently. Equivalent to setting
/// [`Immersive`](EdgeToEdgeMode::Immersive), which also draws behind them.
pub fn set_immersive_sticky(hide: bool) {
    set_edge_to_edge(if hide {
        EdgeToEdgeMode::Immersive
    } else {
        EdgeToEdgeMode::Enabled
    });
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
    apply_to_window();
}

/// Whether a window change needs to reach the Activity. Off Android, and without
/// the `jni-bridge` feature, the state is only recorded.
fn apply_to_window() {
    #[cfg(all(feature = "jni-bridge", target_os = "android"))]
    post_to_activity();
}

/// The Activity owns the window, so only it can change the bar state. It reads
/// [`system_bars`], [`edge_to_edge`] and [`bar_icons`] itself; this just tells the
/// running one to re-apply. A class that is not `RlobKitMainActivity` yields a
/// logged warning instead of the abort that passing the wrong context type would
/// cause.
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
