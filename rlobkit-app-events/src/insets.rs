//! Window insets state and callback.
//!
//! The Android `RlobKitMainActivity` calls `nativeOnWindowInsets` via JNI,
//! which feeds into this module.  Framework integrations (e.g. repose-platform)
//! subscribe via [`set_on_insets`] to forward the values into their own layout
//! system.
//!
//! An app that draws its own UI edge to edge needs more than the bar and IME
//! rectangles: on a display with a cutout, or with a gesture navigation bar, the
//! system-bar insets alone do not say where content may safely go. This carries
//! the display cutout, the gesture exclusion regions, and per-region visibility
//! alongside them.

use crate::subscriber::Subscriber;
use std::sync::Mutex;

/// Whether a region is currently on screen.
///
/// [`Unset`](RegionVisibility::Unset) is distinct from
/// [`Hidden`](RegionVisibility::Hidden): a hidden IME still reserves no space,
/// but a region whose visibility Android declined to report is unknown, and a
/// layout that assumes "not visible" for an unknown region can end up under the
/// bar it was avoiding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionVisibility {
    Visible,
    Hidden,
    Unset,
}

impl Default for RegionVisibility {
    /// [`Unset`](Self::Unset): an all-zero insets value says nothing about
    /// visibility, and reporting "visible" would hide content unnecessarily.
    fn default() -> Self {
        Self::Unset
    }
}

impl RegionVisibility {
    pub const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }
}

/// A rectangle in physical pixels, as reported by the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.left == 0 && self.top == 0 && self.right == 0 && self.bottom == 0
    }
}

/// The region of the display obscured or reserved by the system, in physical
/// pixels.
///
/// The five rectangle fields keep their original meaning. The rest are added for
/// edge-to-edge drawing and are zero/unset where Android did not report them.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowInsets {
    /// Status and navigation bars, combined.
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    /// Bottom of the soft keyboard. Equals [`bottom`](Self::bottom) before
    /// Android 30, where the two were not distinguished.
    pub ime_bottom: f32,

    /// The portion of the display cutout intruding on the window, or zero on a
    /// display without one.
    pub display_cutout: Insets,
    /// Areas where a swipe is interpreted as a system gesture, such as the edge
    /// of a gesture navigation bar.
    pub system_gestures: Insets,
    /// The subset of [`system_gestures`](Self::system_gestures) the system
    /// requires the app to keep clear.
    pub mandatory_system_gestures: Insets,
    /// The touchable region left after excluding gestures, cutouts and bars: a
    /// tap outside it is not delivered to the app.
    pub tappable_element: Insets,

    /// Whether each region is currently on screen.
    pub status_bar: RegionVisibility,
    pub navigation_bar: RegionVisibility,
    pub ime: RegionVisibility,
    pub caption_bar: RegionVisibility,

    /// Progress of the IME show/hide animation, 0.0 to 1.0, or 0.0 when the IME
    /// is not animating. Lets a custom text input track the keyboard instead of
    /// jumping when the animation ends.
    pub ime_animation_progress: f32,
}

impl WindowInsets {
    /// The safe area for interactive content: the display minus everything the
    /// system overlays or reserves.
    pub const fn safe(&self) -> Insets {
        Insets::new(
            self.left as i32,
            self.top as i32,
            self.right as i32,
            self.bottom.max(self.ime_bottom) as i32,
        )
    }

    /// Whether the soft keyboard is on screen.
    pub const fn ime_is_visible(&self) -> bool {
        self.ime.is_visible()
    }

    /// Where the keyboard currently covers the window, for laying out above it.
    ///
    /// This is the animated position while the IME moves, which is what an input
    /// field should track, rather than the settled
    /// [`ime_bottom`](Self::ime_bottom).
    pub const fn ime_overlap(&self) -> f32 {
        if self.ime_animation_progress > 0.0 {
            (self.bottom.max(self.ime_bottom) * self.ime_animation_progress).max(self.ime_bottom)
        } else {
            self.ime_bottom
        }
    }
}

static SUBSCRIBER: Subscriber<WindowInsets> = Subscriber::new();
static LAST_INSETS: Mutex<Option<WindowInsets>> = Mutex::new(None);

/// Register a callback invoked on every window-insets change.
///
/// Replaces any previous subscriber and first replays the current insets if any
/// have arrived, so a late subscriber catches up. Called from the JNI thread
/// (Java main thread).  The callback **must** be `Send + Sync` and should forward
/// to the UI thread / layout system.
pub fn set_on_insets(cb: impl Fn(WindowInsets) + Send + Sync + 'static) {
    let current = last_window_insets();
    SUBSCRIBER.set(cb);
    if let Some(current) = current {
        SUBSCRIBER.notify(current);
    }
}

/// Stores the latest insets and notifies the registered callback (if any).
///
/// Called by the JNI `nativeOnWindowInsets` bridge on Android; also public so a
/// host with its own Activity can drive the same state.
pub fn set_window_insets(insets: WindowInsets) {
    *LAST_INSETS.lock().unwrap_or_else(|e| e.into_inner()) = Some(insets);
    SUBSCRIBER.notify(insets);
}

/// Return the last reported window insets, if any.
pub fn last_window_insets() -> Option<WindowInsets> {
    *LAST_INSETS.lock().unwrap_or_else(|e| e.into_inner())
}
