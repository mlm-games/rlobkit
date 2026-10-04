//! Back-button handling, including the predictive-back gesture.
//!
//! Android delivers back through `OnBackInvokedDispatcher` from API 33, and
//! reports the gesture's progress through `OnBackAnimationCallback` from API 34.
//! Both are driven by the shared Activity and land here.
//!
//! Two separate hooks, mirroring Android's own split:
//!
//! - [`set_on_back`] decides what a back press does. Returning
//!   [`BackOutcome::PropagateToSystem`] lets the platform finish the Activity or
//!   go home, which is what should happen when the app has no internal
//!   navigation left to unwind.
//! - [`set_on_back_event`] observes the gesture so a custom UI can animate with
//!   it. It cannot change the outcome.

use crate::subscriber::Subscriber;
use std::sync::Mutex;

/// What the app wants a back press to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackOutcome {
    /// The app handled it; nothing further happens.
    Consumed,
    /// The app has nothing to go back to. The platform finishes the Activity or
    /// returns to the launcher, with the normal predictive-back animation.
    PropagateToSystem,
    /// The app has started a transition of its own and the system should animate
    /// the gesture as if it were completing. Requires Android 14 or later;
    /// earlier versions get the same behaviour as
    /// [`Consumed`](BackOutcome::Consumed).
    CrossActivity,
}

/// A phase of the predictive-back gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackEvent {
    /// The gesture began. There is no cancellation yet.
    Started,
    /// The gesture moved, `0.0` at the start and `1.0` fully committed.
    Progress(f32),
    /// The gesture was abandoned and the app is staying put.
    Cancelled,
    /// The gesture completed; the back was dispatched.
    Invoked,
}

static HANDLER: Mutex<Option<Box<dyn Fn() -> BackOutcome + Send + Sync>>> = Mutex::new(None);
static EVENTS: Subscriber<BackEvent> = Subscriber::new();

/// Register the app's back handler, replacing any previous one.
///
/// The handler runs on the JNI thread (the Java main thread) while Android waits
/// for its answer, so it must return promptly and must be `Send + Sync`. Do not
/// block on the render loop from inside it; hand the decision to the owning
/// thread and return [`PropagateToSystem`](BackOutcome::PropagateToSystem) if
/// the answer is not ready in time.
///
/// With no handler registered, back propagates to the system.
pub fn set_on_back(handler: impl Fn() -> BackOutcome + Send + Sync + 'static) {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(handler));
}

/// Drop the back handler, so back propagates to the system again.
pub fn clear_on_back() {
    HANDLER.lock().unwrap_or_else(|e| e.into_inner()).take();
}

/// Register a callback for predictive-back gesture phases.
///
/// Replaces any previous subscriber. Only fires on Android 14 and later, and
/// only when the app opted in with
/// [`OnBackInvokedCallback.PRIORITY`](BackOutcome::CrossActivity) by handling
/// back at least once. Runs on the JNI thread, so it must be `Send + Sync` and
/// should forward to the thread that draws.
pub fn set_on_back_event(cb: impl Fn(BackEvent) + Send + Sync + 'static) {
    EVENTS.set(cb);
}

/// Called by the JNI `nativeOnBack` bridge.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub(crate) fn invoke() -> BackOutcome {
    let handler = { HANDLER.lock().unwrap_or_else(|e| e.into_inner()).take() };
    match handler {
        Some(handler) => handler(),
        None => BackOutcome::PropagateToSystem,
    }
}

/// Called by the JNI `nativeBackEvent` bridge.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub(crate) fn dispatch(event: BackEvent) {
    EVENTS.notify(event);
}
