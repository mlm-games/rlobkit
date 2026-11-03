//! Back-button handling, including the predictive-back gesture.
//!
//! Android delivers back through `OnBackInvokedDispatcher` from API 33, and
//! reports the gesture's progress through `OnBackAnimationCallback` from API 34.
//! Both are driven by the shared Activity and land here.
//!
//! Three separate hooks, mirroring Android's own split:
//!
//! - [`set_handled`] tells the Activity whether back has anywhere to go. While
//!   it is `false` the callback stays unregistered, so the system plays its own
//!   back-to-home animation at a root screen.
//! - [`set_on_back`] decides what a back press does. Returning
//!   [`BackOutcome::PropagateToSystem`] lets the platform finish the Activity or
//!   go home, which is what should happen when the app has no internal
//!   navigation left to unwind.
//! - [`set_on_back_event`] observes the gesture so a custom UI can animate with
//!   it. It cannot change the outcome.
//!
//! [`progress`] and [`gesture_active`] expose the latest phase without a
//! callback, for a render loop that polls once per frame.

use crate::subscriber::Subscriber;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

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

/// The screen edge a predictive-back gesture started from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SwipeEdge {
    /// The user dragged inwards from the left edge, so their finger moved right.
    #[default]
    Left,
    /// The user dragged inwards from the right edge, so their finger moved left.
    Right,
}

/// A phase of the predictive-back gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackEvent {
    /// The gesture began. There is no cancellation yet.
    Started { edge: SwipeEdge },
    /// The gesture moved, `0.0` at the start and `1.0` fully committed.
    Progress { progress: f32, edge: SwipeEdge },
    /// The gesture was abandoned and the app is staying put.
    Cancelled,
    /// The gesture completed; the back was dispatched.
    Invoked,
}

type Handler = Arc<dyn Fn() -> BackOutcome + Send + Sync>;

static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);
static EVENTS: Subscriber<BackEvent> = Subscriber::new();
static HANDLED: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static PROGRESS_BITS: AtomicU32 = AtomicU32::new(0);
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
static BRIDGE_AVAILABLE: AtomicBool = AtomicBool::new(true);

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
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(handler));
}

/// Drop the back handler, so back propagates to the system again.
pub fn clear_on_back() {
    HANDLER.lock().unwrap_or_else(|e| e.into_inner()).take();
}

/// Register a callback for predictive-back gesture phases.
///
/// Replaces any previous subscriber. Only fires on Android 14 and later, and
/// only while [`set_handled`] is `true`. Runs on the JNI thread, so it must be
/// `Send + Sync` and should forward to the thread that draws.
pub fn set_on_back_event(cb: impl Fn(BackEvent) + Send + Sync + 'static) {
    EVENTS.set(cb);
}

/// Tell the Activity whether back has somewhere to go.
///
/// `false` unregisters the callback so Android can play its own back-to-home
/// animation at a root screen; `true` registers it, which is what enables the
/// predictive gesture. Call this whenever the navigation depth changes.
///
/// Safe to call before the Activity exists: the requested state is applied
/// when it is created.
pub fn set_handled(handled: bool) {
    let previous = HANDLED.swap(handled, Ordering::Relaxed);
    if previous == handled {
        return;
    }
    #[cfg(all(feature = "jni-bridge", target_os = "android"))]
    if !post_handled(handled) && bridge_available() {
        HANDLED.store(previous, Ordering::Relaxed);
    }
}

/// Whether the last [`set_handled`] call said back is handled.
pub fn handled() -> bool {
    HANDLED.load(Ordering::Relaxed)
}

/// Finish the Activity after a committed gesture found nothing to unwind.
///
/// Platform runners call this only when they told [`set_handled`] that back was
/// handled but the handler declined at invocation time, so the user is not left
/// with a dead gesture.
pub fn finish() {
    #[cfg(all(feature = "jni-bridge", target_os = "android"))]
    post_finish();
}

/// Whether a predictive-back gesture is in progress.
pub fn gesture_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// The gesture's progress, `0.0` at the start and `1.0` fully committed.
///
/// Always `0.0` when [`gesture_active`] is false, so multiplying by a
/// distance is safe outside a gesture.
pub fn progress() -> f32 {
    if gesture_active() {
        f32::from_bits(PROGRESS_BITS.load(Ordering::Relaxed))
    } else {
        0.0
    }
}

/// Called by the JNI `nativeOnBack` bridge.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub(crate) fn invoke() -> BackOutcome {
    let handler = HANDLER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match handler {
        Some(handler) => handler(),
        None => BackOutcome::PropagateToSystem,
    }
}

/// Called by the JNI `nativeOnBackEvent` bridge.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub(crate) fn dispatch(event: BackEvent) {
    match event {
        BackEvent::Started { .. } => {
            ACTIVE.store(true, Ordering::Relaxed);
            PROGRESS_BITS.store(0.0f32.to_bits(), Ordering::Relaxed);
        }
        BackEvent::Progress { progress, .. } => {
            let progress = if progress.is_finite() {
                progress.clamp(0.0, 1.0)
            } else {
                0.0
            };
            ACTIVE.store(true, Ordering::Relaxed);
            PROGRESS_BITS.store(progress.to_bits(), Ordering::Relaxed);
        }
        BackEvent::Cancelled | BackEvent::Invoked => {
            ACTIVE.store(false, Ordering::Relaxed);
            PROGRESS_BITS.store(0.0f32.to_bits(), Ordering::Relaxed);
        }
    }
    EVENTS.notify(event);
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn bridge_available() -> bool {
    BRIDGE_AVAILABLE.load(Ordering::Relaxed)
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn note_failure(error: &jni::errors::Error) -> bool {
    use jni::errors::Error;
    let permanent = matches!(
        error,
        Error::ClassNotFound { .. }
            | Error::NoClassDefFound { .. }
            | Error::MethodNotFound { .. }
            | Error::NoSuchMethod(_)
    );
    if permanent {
        BRIDGE_AVAILABLE.store(false, Ordering::Relaxed);
        log::warn!("rlobkit_app_events::back: Activity bridge unavailable: {error}");
    } else {
        log::warn!("rlobkit_app_events::back: Activity bridge call failed: {error}");
    }
    !permanent
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn post_finish() {
    if !bridge_available() {
        return;
    }
    use jni::{jni_sig, jni_str};
    let result = jni_min_helper::jni_with_env(|env| {
        env.call_static_method(
            jni_str!("rust/rlobkit/RlobKitMainActivity"),
            jni_str!("finishActivity"),
            jni_sig!("()V"),
            &[],
        )?;
        env.exception_catch()?;
        Ok(())
    });
    if let Err(e) = result {
        note_failure(&e);
    }
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn post_handled(handled: bool) -> bool {
    if !bridge_available() {
        return false;
    }
    use jni::objects::JValue;
    use jni::{jni_sig, jni_str};
    let result = jni_min_helper::jni_with_env(|env| {
        env.call_static_method(
            jni_str!("rust/rlobkit/RlobKitMainActivity"),
            jni_str!("setBackHandled"),
            jni_sig!("(Z)V"),
            &[JValue::Bool(handled)],
        )?;
        env.exception_catch()?;
        Ok(())
    });
    match result {
        Ok(()) => true,
        Err(error) => {
            note_failure(&error);
            false
        }
    }
}
