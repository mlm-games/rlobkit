//! JNI implementations for `RlobKitMainActivity`.
//!
//! These symbols are referenced by the Kotlin `RlobKitMainActivity` and must
//! be linked into any app that uses that Activity class.
//!
//! # Keeping the symbols in the link
//!
//! The Activity looks these up by name at runtime, so nothing in the Rust call
//! graph names them. In practice they are still linked: an app that uses any part
//! of this crate pulls in the object file they share a codegen unit with, and
//! they are exported from there. The shipped apps all rely on that.
//!
//! The case that does *not* link is an app that depends on this crate and calls
//! nothing in it, where the linker has no reason to extract the archive member at
//! all. That is not a crash — every native call fails with
//! `UnsatisfiedLinkError`, which the Activity deliberately tolerates — so the app
//! would run while never reporting an inset, a palette or a back event, and
//! nothing anywhere would say why.
//!
//! [`verify_linked`] turns that into one line in logcat. Calling it also
//! references the module, which keeps the symbols regardless.
//!
//! # Shape of the bridge
//!
//! Every symbol here is one-way: the Activity pushes state in, and the modules
//! that own the state push changes back out through `jni-min-helper` calls to
//! static methods on the same class. That is why the Kotlin side guards its
//! native calls: it can be packaged into an app that does not link this crate.

use crate::back::BackEvent;
use crate::insets::{Insets, WindowInsets};
use crate::memory::MemoryPressure;
use crate::system_bars;
use crate::theme::ThemeColors;
use jni::objects::JByteArray;
use jni::sys::{jboolean, jbyteArray, jfloat, jint, jobject};
use jni::{EnvUnowned, errors::ThrowRuntimeExAndDefault};

/// Wire encoding shared with the Kotlin `RlobKitMainActivity`. It lives here
/// rather than on the enums because it exists only at the JNI boundary.

/// `BackEvent` phases.
const EVENT_STARTED: i32 = 0;
const EVENT_PROGRESS: i32 = 1;
const EVENT_CANCELLED: i32 = 2;
const EVENT_INVOKED: i32 = 3;

/// `BackOutcome` discriminants.
const BACK_CONSUMED: i32 = 0;
const BACK_PROPAGATE: i32 = 1;
const BACK_CROSS_ACTIVITY: i32 = 2;

/// `RegionVisibility` discriminants. A value outside these is treated as unset,
/// which is what Android's own -1 means.
const VISIBILITY_VISIBLE: i32 = 0;
const VISIBILITY_HIDDEN: i32 = 1;

/// `WindowInsets.isVisible` as the wire values the native side expects.
fn region_visibility(value: i32) -> crate::insets::RegionVisibility {
    use crate::insets::RegionVisibility;
    match value {
        VISIBILITY_VISIBLE => RegionVisibility::Visible,
        VISIBILITY_HIDDEN => RegionVisibility::Hidden,
        // Android sends -1 when it declines to say, and a newer Activity could
        // send anything else; neither is grounds for claiming a region is hidden.
        _ => RegionVisibility::Unset,
    }
}

/// `ComponentCallbacks2.TRIM_MEMORY_*` levels, collapsed to the response the
/// native side should take. An unrecognised or newer level is treated as the most
/// severe, since over-releasing is cheaper than being killed.
fn memory_pressure(level: i32) -> MemoryPressure {
    match level {
        5 | 20 => MemoryPressure::TrimCaches, // RUNNING_MODERATE, UI_HIDDEN
        10 | 40 => MemoryPressure::TrimRunning, // RUNNING_LOW, BACKGROUND
        _ => MemoryPressure::TrimCritical,    // RUNNING_CRITICAL, MODERATE, COMPLETE
    }
}

fn back_event(phase: i32, progress: f32) -> Option<BackEvent> {
    match phase {
        EVENT_STARTED => Some(BackEvent::Started),
        EVENT_PROGRESS => Some(BackEvent::Progress(progress)),
        EVENT_CANCELLED => Some(BackEvent::Cancelled),
        EVENT_INVOKED => Some(BackEvent::Invoked),
        _ => None,
    }
}

fn back_outcome(outcome: crate::back::BackOutcome) -> jint {
    use crate::back::BackOutcome;
    match outcome {
        BackOutcome::Consumed => BACK_CONSUMED,
        BackOutcome::PropagateToSystem => BACK_PROPAGATE,
        BackOutcome::CrossActivity => BACK_CROSS_ACTIVITY,
    }
}

/// Called by `RlobKitMainActivity` when it applies the system-bar state, which
/// the native side owns so a toggle made before the Activity existed survives.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeStatusBarVisible(
    _env: EnvUnowned,
    _this: jobject,
) -> jboolean {
    system_bars::system_bars().status
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeNavigationBarVisible(
    _env: EnvUnowned,
    _this: jobject,
) -> jboolean {
    system_bars::system_bars().navigation
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeLightBarIcons(
    _env: EnvUnowned,
    _this: jobject,
) -> jboolean {
    system_bars::bar_icons() == system_bars::BarIcons::Light
}

/// `EdgeToEdgeMode::to_wire`: 0 disabled, 1 enabled, 2 immersive.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeEdgeToEdge(
    _env: EnvUnowned,
    _this: jobject,
) -> jint {
    i32::from(system_bars::edge_to_edge().to_wire())
}

/// Called by `RlobKitMainActivity`'s `OnApplyWindowInsetsListener`.
///
/// `vis_*` are the `WindowInsets.isVisible` results, where Android reports -1
/// when it does not know.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeOnWindowInsets(
    _env: EnvUnowned,
    _this: jobject,
    top_px: jfloat,
    bottom_px: jfloat,
    left_px: jfloat,
    right_px: jfloat,
    ime_bottom_px: jfloat,
    cutout_left: jint,
    cutout_top: jint,
    cutout_right: jint,
    cutout_bottom: jint,
    gesture_left: jint,
    gesture_top: jint,
    gesture_right: jint,
    gesture_bottom: jint,
    mandatory_gesture_left: jint,
    mandatory_gesture_top: jint,
    mandatory_gesture_right: jint,
    mandatory_gesture_bottom: jint,
    tappable_left: jint,
    tappable_top: jint,
    tappable_right: jint,
    tappable_bottom: jint,
    status_visible: jint,
    navigation_visible: jint,
    ime_visible: jint,
    caption_visible: jint,
    ime_animation_progress: jfloat,
) {
    crate::insets::set_window_insets(WindowInsets {
        top: top_px,
        bottom: bottom_px,
        left: left_px,
        right: right_px,
        ime_bottom: ime_bottom_px,
        display_cutout: Insets::new(cutout_left, cutout_top, cutout_right, cutout_bottom),
        system_gestures: Insets::new(gesture_left, gesture_top, gesture_right, gesture_bottom),
        mandatory_system_gestures: Insets::new(
            mandatory_gesture_left,
            mandatory_gesture_top,
            mandatory_gesture_right,
            mandatory_gesture_bottom,
        ),
        tappable_element: Insets::new(tappable_left, tappable_top, tappable_right, tappable_bottom),
        status_bar: region_visibility(status_visible),
        navigation_bar: region_visibility(navigation_visible),
        ime: region_visibility(ime_visible),
        caption_bar: region_visibility(caption_visible),
        ime_animation_progress,
    });
}

/// Called by `RlobKitMainActivity`'s theme listener.
///
/// `data` is the packed RGBA bytes as produced by [`ThemeColors::as_bytes`]:
/// 38 × 4 bytes, little-endian RGBA per color, in the order documented on
/// [`crate::theme::THEME_COLOR_COUNT`]. A null array is how the Activity reports
/// that this Android version has no palette at all, which is distinct from a
/// malformed one.
///
/// Light/dark is not sent: winit reports that as `Window::system_theme()`.
///
/// `dynamic_available` says whether Android publishes a palette here at all, so
/// "cannot do dynamic color" is distinguishable from "nothing pushed yet".
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeOnTheme(
    mut env: EnvUnowned,
    _this: jobject,
    dynamic_available: jboolean,
    data: jbyteArray,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let colors = if data.is_null() {
            None
        } else {
            let array = unsafe { JByteArray::from_raw(env, data) };
            let bytes = env.convert_byte_array(&array)?;
            match ThemeColors::from_bytes(&bytes) {
                Some(colors) => Some(colors),
                None => {
                    log::warn!(
                        "rlobkit_app_events::jni: nativeOnTheme got {} bytes, want {}",
                        bytes.len(),
                        crate::theme::THEME_COLOR_COUNT * 4
                    );
                    None
                }
            }
        };
        crate::theme::set_dynamic_colors_available(dynamic_available);
        if let Some(colors) = colors {
            crate::theme::set_theme(colors);
        }
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// Called from `onTrimMemory` and `onLowMemory`, with the raw
/// `ComponentCallbacks2.TRIM_MEMORY_*` level.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeOnMemoryPressure(
    _env: EnvUnowned,
    _this: jobject,
    level: jint,
) {
    crate::memory::dispatch(memory_pressure(level));
}

/// Called by `RlobKitMainActivity`'s `OnBackInvokedCallback`. Returns a
/// `BackOutcome` wire value: 0 consumed, 1 propagate to system, 2 cross-activity.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeOnBack(
    _env: EnvUnowned,
    _this: jobject,
) -> jint {
    back_outcome(crate::back::invoke())
}

/// Called from `OnBackAnimationCallback` on Android 14 and later. `phase` is one
/// of the `EVENT_*` discriminants and `progress` is meaningful only for
/// `EVENT_PROGRESS`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeOnBackEvent(
    _env: EnvUnowned,
    _this: jobject,
    phase: jint,
    progress: jfloat,
) {
    if let Some(event) = back_event(phase, progress) {
        crate::back::dispatch(event);
    }
}

/// Every entry point above, named.
///
/// They are `#[no_mangle]` and exported, but nothing in the Rust call graph
/// refers to them: the Kotlin Activity resolves them by name at runtime. See the
/// module docs for what an app has to do to keep them in the link.
///
/// Not required for a normal app: anything that calls into this crate already
/// keeps them linked. It is for the failure that produces no other symptom — an
/// app that gets no inset, palette or back event and no error. Call it once from
/// `android_main`:
///
/// ```ignore
/// if !rlobkit_app_events::jni::verify_linked() {
///     // No appearance, inset or back events will arrive.
/// }
/// ```
pub fn verify_linked() -> bool {
    // A real static call, not a no-op: the answer depends on this crate's code
    // being present in the library, which is the same condition the Activity's
    // `external fun`s need.
    let linked = activity_present();
    if !linked {
        log::warn!(
            "rlobkit_app_events::jni: the Activity cannot reach the native side; \
             the jni-bridge feature or a reference to this module is missing"
        );
    }
    linked
}

/// Whether the shared Activity class is loaded and its JNI symbols resolved.
///
/// `NoClassDefFoundError` means the Kotlin class is absent, which is the same
/// practical outcome for the app as a missing symbol: nothing is pushed.
fn activity_present() -> bool {
    use jni::{jni_sig, jni_str};
    jni_min_helper::jni_with_env(|env| {
        let _ = env.find_class(jni_str!("rust/rlobkit/RlobKitMainActivity"))?;
        let _ = env.call_static_method(
            jni_str!("rust/rlobkit/RlobKitMainActivity"),
            jni_str!("refreshSystemBars"),
            jni_sig!("()V"),
            &[],
        )?;
        Ok(())
    })
    .is_ok()
}

/// Called by the shared `RlobKitMainActivity` after it has written an intent
/// record to the queue, so an app whose frame loop is asleep can wake and
/// drain it. Only the doorbell: the record itself is read back through
/// [`crate::intents::take_pending_intent`] or
/// [`crate::intents::drain_intents_from`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeOnIntentQueued(
    _env: EnvUnowned,
    _this: jobject,
) {
    crate::intents::notify_new_intent();
}

/// Optional JNI hook an app's own Activity subclass can declare to get intents
/// as soon as they arrive, instead of waiting for the file queue to be polled.
///
/// This is **not** declared in the shared Activity: adding an `external fun` to a
/// subclass means the symbol must exist or the class will not load. Point it at
/// this function and hand it the record bytes the bridge writes.
///
/// # Safety
/// `data` must be the `byte[]` local reference the JNI frame passed in, and must
/// stay valid for the duration of that frame.
pub unsafe fn decode_and_push_intent(env: &mut jni::EnvUnowned, data: jbyteArray) {
    env.with_env(|env| {
        if data.is_null() {
            return Ok(());
        }
        let array = unsafe { JByteArray::from_raw(env, data) };
        let bytes = env.convert_byte_array(&array)?;
        match crate::intents::decode(&bytes) {
            Some(intent) => {
                log::info!(
                    "rlobkit_app_events::jni: new intent {:?} with {} file(s)",
                    intent.action,
                    intent.files.len()
                );
                crate::intents::push_intent(intent);
                crate::intents::notify_new_intent();
            }
            None => log::warn!("rlobkit_app_events::jni: unreadable intent record"),
        }
        Ok::<_, jni::errors::Error>(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
