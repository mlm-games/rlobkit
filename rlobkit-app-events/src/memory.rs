//! Memory pressure, pushed from Android.
//!
//! Deliberately narrow. The Activity lifecycle and window focus are **not** here:
//! winit already delivers those as `ApplicationHandler::{resumed, suspended}` and
//! the `Focused` / `Resumed` window events, and every app using this crate runs
//! on winit.
//!
//! What winit does not expose is `onTrimMemory` / `onLowMemory`. Android sends
//! those when the system wants caches dropped or the process is a candidate for
//! death, and an app that renders to a GPU texture cache or holds a large
//! decoded asset needs to hear about it. Nothing else in the stack reports it.

use crate::subscriber::Subscriber;
use std::sync::Mutex;

/// How urgently the app should release memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    /// The UI is gone or the pressure is mild. Drop caches and keep running.
    TrimCaches,
    /// The process is a hidden candidate for death. Persist anything expensive to
    /// recreate now.
    TrimRunning,
    /// Death is imminent unless memory is freed. Release everything optional.
    TrimCritical,
}

static SUBSCRIBER: Subscriber<MemoryPressure> = Subscriber::new();
static HIGHEST: Mutex<Option<MemoryPressure>> = Mutex::new(None);

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn severity(pressure: MemoryPressure) -> u8 {
    match pressure {
        MemoryPressure::TrimCaches => 0,
        MemoryPressure::TrimRunning => 1,
        MemoryPressure::TrimCritical => 2,
    }
}

/// The most severe pressure reported so far.
///
/// Monotonic: a later, milder notification does not clear it, because Android
/// sends those too and forgetting that the process was nearly killed would lose
/// the signal that mattered. Reset by [`clear_memory_pressure`].
pub fn highest_memory_pressure() -> Option<MemoryPressure> {
    *HIGHEST.lock().unwrap_or_else(|e| e.into_inner())
}

/// Forget the recorded pressure, e.g. once caches have actually been dropped.
pub fn clear_memory_pressure() {
    *HIGHEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Register a callback invoked on every memory-pressure notification.
///
/// Replaces any previous subscriber. There is no replay: pressure that has
/// already been relieved is not worth reporting again, and
/// [`highest_memory_pressure`] covers the case where it was not. Called from the
/// JNI thread (the Java main thread), so it must be `Send + Sync` and should hand
/// off to the thread that owns the GPU resources.
pub fn set_on_memory_pressure(cb: impl Fn(MemoryPressure) + Send + Sync + 'static) {
    SUBSCRIBER.set(cb);
}

/// Called by the JNI `nativeOnMemoryPressure` bridge.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub(crate) fn dispatch(pressure: MemoryPressure) {
    let mut highest = HIGHEST.lock().unwrap_or_else(|e| e.into_inner());
    let record = match *highest {
        Some(current) if severity(current) >= severity(pressure) => false,
        _ => true,
    };
    if record {
        *highest = Some(pressure);
    }
    drop(highest);
    SUBSCRIBER.notify(pressure);
}
