package rust.rlobkit

import android.app.NativeActivity
import android.app.WallpaperManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsAnimation
import android.view.WindowInsetsController
import android.view.WindowManager
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher
import java.lang.ref.WeakReference

/**
 * Shared NativeActivity subclass used by all rlobkit-based apps.
 *
 * Handles:
 * - Incoming ACTION_VIEW / ACTION_SEND / ACTION_SEND_MULTIPLE intents
 * - Window insets, IME, cutouts and gesture areas (nativeOnWindowInsets)
 * - The dynamic theme palette (nativeOnTheme); light/dark comes from winit
 * - Memory pressure (nativeOnMemoryPressure); the lifecycle comes from winit
 * - Back invocation and the predictive-back gesture (nativeOnBack)
 * - System bars and the edge-to-edge policy, re-applied on request (refreshSystemBars)
 * - Device vibration on request from the native side (rumble / rumbleStop)
 *
 * Every `external fun` here is optional: a host app may package this Activity
 * without linking the crate that provides the symbols. Each call site therefore
 * tolerates `UnsatisfiedLinkError` rather than assuming the library is present.
 *
 * Reference this activity in your Cargo.toml:
 *
 *   [[package.metadata.android.application.activity]]
 *   name = "rust.rlobkit.RlobKitMainActivity"
 *   exported = true
 *   launch_mode = "singleTask"
 */
class RlobKitMainActivity : NativeActivity() {
    private var wallpaperColorsListener: WallpaperManager.OnColorsChangedListener? = null
    private var backCallback: OnBackInvokedCallback? = null
    private var imeAnimationProgress = 0f

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        current = WeakReference(this)
        RlobKitIntentBridge.capture(intent, contentResolver, filesDir)
        loadLibraryForJni()
        setupWindowInsetsListener()
        setupImeAnimationListener()
        setupBackHandling()
        applySystemBars()
        pushAppearance()
        watchWallpaperColors()
    }

    override fun onResume() {
        super.onResume()
        applySystemBars()
    }

    override fun onDestroy() {
        unwatchWallpaperColors()
        if (Build.VERSION.SDK_INT >= 33) releaseBackHandling()
        super.onDestroy()
        if (current?.get() === this) current = null
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        pushAppearance()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) applySystemBars()
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        reportMemoryPressure(level)
    }

    @Deprecated("Superseded by onTrimMemory, which Android still calls first")
    override fun onLowMemory() {
        super.onLowMemory()
        reportMemoryPressure(TRIM_MEMORY_COMPLETE)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        RlobKitIntentBridge.capture(intent, contentResolver, filesDir)
        setIntent(Intent(Intent.ACTION_MAIN))
        try {
            nativeOnIntentQueued()
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    /**
     * Loads the app's native library so the JNI entry points below resolve.
     *
     * `UnsatisfiedLinkError` is an `Error`, not an `Exception`, so it has to be
     * caught explicitly: a plain `catch (e: Exception)` would let a missing or
     * mismatched library take down the Activity.
     */
    private fun loadLibraryForJni() {
        val libName = try {
            val info = packageManager.getActivityInfo(
                ComponentName(this, javaClass),
                PackageManager.GET_META_DATA,
            )
            info.metaData?.getString("android.app.lib_name") ?: "main"
        } catch (e: Exception) {
            Log.e(TAG, "cannot read lib_name from the manifest", e)
            "main"
        }
        try {
            System.loadLibrary(libName)
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "loadLibrary($libName) failed; native calls are inert", e)
        } catch (e: Exception) {
            Log.e(TAG, "loadLibrary($libName) failed", e)
        }
    }

    /**
     * Reports memory pressure to the native side, tolerating its absence.
     *
     * The Activity lifecycle and window focus are deliberately not reported:
     * winit already delivers those, and two sources for them could disagree.
     */
    private fun reportMemoryPressure(level: Int) {
        try {
            nativeOnMemoryPressure(level)
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    /**
     * Applies the window policy the native side owns: which bars are visible,
     * whether the app draws behind them, and the bar icon contrast.
     */
    private fun applySystemBars() {
        val bars = nativeBarState()
        val statusVisible = bars[0]
        val navigationVisible = bars[1]
        val lightIcons = bars[2]
        // 0 disabled, 1 enabled, 2 immersive; see EdgeToEdgeMode.
        val edgeToEdge = try {
            nativeEdgeToEdge()
        } catch (_: UnsatisfiedLinkError) {
            EDGE_TO_EDGE_ENABLED
        }
        val hideBars = edgeToEdge == EDGE_TO_EDGE_IMMERSIVE
        val drawBehindBars = edgeToEdge != EDGE_TO_EDGE_DISABLED

        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(!drawBehindBars)
            if (drawBehindBars && Build.VERSION.SDK_INT >= 28) {
                window.attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
            val controller = window.insetsController ?: return
            val statusVisibleNow = statusVisible && !hideBars
            val navigationVisibleNow = navigationVisible && !hideBars
            val hidden = (if (statusVisibleNow) 0 else WindowInsets.Type.statusBars()) or
                (if (navigationVisibleNow) 0 else WindowInsets.Type.navigationBars())
            if (hidden != 0) {
                controller.hide(hidden)
                controller.systemBarsBehavior =
                    WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            } else {
                controller.show(WindowInsets.Type.systemBars())
            }
            val appearanceMask = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            controller.setSystemBarsAppearance(if (lightIcons) 0 else appearanceMask, appearanceMask)
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = legacySystemUiVisibility(
                statusVisible, navigationVisible, lightIcons, drawBehindBars, hideBars,
            )
        }
    }

    @Suppress("DEPRECATION")
    private fun legacySystemUiVisibility(
        statusVisible: Boolean,
        navigationVisible: Boolean,
        lightIcons: Boolean,
        drawBehindBars: Boolean,
        hideBars: Boolean,
    ): Int {
        val immersiveFlags = if (hideBars) {
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        } else {
            0
        }
        val layoutFlags = if (drawBehindBars) {
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        } else {
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE
        }
        val barFlags = (if (statusVisible || hideBars) 0 else View.SYSTEM_UI_FLAG_FULLSCREEN) or
            (if (navigationVisible || hideBars) 0 else View.SYSTEM_UI_FLAG_HIDE_NAVIGATION)
        val iconFlags = if (lightIcons) {
            0
        } else {
            View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
                (if (Build.VERSION.SDK_INT >= 26) View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR else 0)
        }
        return layoutFlags or barFlags or iconFlags or immersiveFlags
    }

    /**
     * Pushes every inset region an edge-to-edge layout needs.
     *
     * The older branch cannot report cutouts, gesture areas or per-region
     * visibility, so those go across as zero and unset rather than guessed.
     */
    private fun setupWindowInsetsListener() {
        if (Build.VERSION.SDK_INT >= 30) {
            window.decorView.setOnApplyWindowInsetsListener { view, insets ->
                val bars = insets.getInsets(WindowInsets.Type.systemBars())
                val ime = insets.getInsets(WindowInsets.Type.ime())
                val cutout = insets.getInsets(WindowInsets.Type.displayCutout())
                val gestures = insets.getInsets(WindowInsets.Type.systemGestures())
                val mandatory = insets.getInsets(WindowInsets.Type.mandatorySystemGestures())
                val tappable = insets.getInsets(WindowInsets.Type.tappableElement())
                try {
                    nativeOnWindowInsets(
                        bars.top.toFloat(), bars.bottom.toFloat(),
                        bars.left.toFloat(), bars.right.toFloat(),
                        ime.bottom.toFloat(),
                        cutout.left, cutout.top, cutout.right, cutout.bottom,
                        gestures.left, gestures.top, gestures.right, gestures.bottom,
                        mandatory.left, mandatory.top, mandatory.right, mandatory.bottom,
                        tappable.left, tappable.top, tappable.right, tappable.bottom,
                        visibility(insets, WindowInsets.Type.statusBars()),
                        visibility(insets, WindowInsets.Type.navigationBars()),
                        visibility(insets, WindowInsets.Type.ime()),
                        visibility(insets, WindowInsets.Type.captionBar()),
                        imeAnimationProgress,
                    )
                } catch (_: UnsatisfiedLinkError) {
                }
                view.onApplyWindowInsets(insets)
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.setOnApplyWindowInsetsListener { view, insets ->
                val bottom = insets.systemWindowInsetBottom
                try {
                    nativeOnWindowInsets(
                        insets.systemWindowInsetTop.toFloat(), bottom.toFloat(),
                        insets.systemWindowInsetLeft.toFloat(), insets.systemWindowInsetRight.toFloat(),
                        bottom.toFloat(),
                        0, 0, 0, 0,
                        0, 0, 0, 0,
                        0, 0, 0, 0,
                        0, 0, 0, 0,
                        VISIBILITY_UNSET, VISIBILITY_UNSET, VISIBILITY_UNSET, VISIBILITY_UNSET,
                        0f,
                    )
                } catch (_: UnsatisfiedLinkError) {
                }
                view.onApplyWindowInsets(insets)
            }
        }
    }

    /**
     * Reports the keyboard's animation so the native side can move its input
     * field with the keyboard instead of jumping when it settles.
     *
     * The listener returns the insets unchanged: this observes the animation, it
     * does not drive it.
     */
    private fun setupImeAnimationListener() {
        if (Build.VERSION.SDK_INT < 30) return
        window.decorView.setWindowInsetsAnimationCallback(
            object : WindowInsetsAnimation.Callback(
                WindowInsetsAnimation.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE,
            ) {
                override fun onPrepare(animation: WindowInsetsAnimation) {
                    if (animation.typeMask and WindowInsets.Type.ime() != 0) {
                        imeAnimationProgress = 0f
                    }
                }

                override fun onProgress(
                    insets: WindowInsets,
                    runningAnimations: MutableList<WindowInsetsAnimation>,
                ): WindowInsets {
                    val imeAnimation = runningAnimations.firstOrNull {
                        it.typeMask and WindowInsets.Type.ime() != 0
                    }
                    if (imeAnimation != null) {
                        imeAnimationProgress = imeAnimation.interpolatedFraction
                    }
                    return insets
                }

                override fun onEnd(animation: WindowInsetsAnimation) {
                    if (animation.typeMask and WindowInsets.Type.ime() != 0) {
                        imeAnimationProgress = 0f
                        // onProgress does not re-dispatch insets, so ask for a
                        // fresh pass to publish the settled IME height.
                        window.decorView.requestApplyInsets()
                    }
                }
            },
        )
    }

    /**
     * Wires back invocation, preferring the predictive-back APIs.
     *
     * `OnBackInvokedDispatcher` is available from Android 13. The predictive-back
     * *gesture progress* is not reachable here: `OnBackAnimationCallback` is a
     * public interface with no public registration method, so an app that needs
     * the phases registers it through AndroidX and forwards them via
     * [nativeOnBackEvent]. The invocation callback is always registered at the
     * default priority, and the decision it returns is acted on immediately,
     * which is what lets an app with no internal navigation fall through to the
     * system's own back-to-home.
     */
    private fun setupBackHandling() {
        if (Build.VERSION.SDK_INT < 33) return
        val callback = OnBackInvokedCallback { handleBack() }
        backCallback = callback
        onBackInvokedDispatcher.registerOnBackInvokedCallback(
            OnBackInvokedDispatcher.PRIORITY_DEFAULT,
            callback,
        )
    }

    private fun releaseBackHandling() {
        backCallback?.let(onBackInvokedDispatcher::unregisterOnBackInvokedCallback)
        backCallback = null
    }

    /**
     * Asks the native side what back should do and carries it out.
     *
     * An unlinked library, or no registered handler, propagates to the platform
     * so the Activity finishes as it otherwise would.
     */
    private fun handleBack() {
        val outcome = try {
            nativeOnBack()
        } catch (_: UnsatisfiedLinkError) {
            BACK_PROPAGATE
        }
        when (outcome) {
            BACK_CONSUMED -> Unit
            BACK_CROSS_ACTIVITY -> Unit
            else -> finish()
        }
    }

    /**
     * Hands the system's light/dark mode and, where Android publishes one, the
     * dynamic palette to the native side.
     *
     * The mode is pushed on every Android version. The palette needs Android 12,
     * so below that a null array tells the native side there is none rather than
     * leaving it to infer one from a missing value.
     */
    private fun pushAppearance() {
        val dynamicAvailable = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        val palette = if (dynamicAvailable) {
            DynamicTheme.build(resources, DynamicTheme.isNight(resources.configuration))
        } else {
            null
        }
        try {
            nativeOnTheme(dynamicAvailable, palette)
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    private fun watchWallpaperColors() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || wallpaperColorsListener != null) return
        val listener = WallpaperManager.OnColorsChangedListener { _, _ ->
            runOnUiThread { pushAppearance() }
        }
        WallpaperManager.getInstance(applicationContext)
            .addOnColorsChangedListener(listener, Handler(Looper.getMainLooper()))
        wallpaperColorsListener = listener
    }

    private fun unwatchWallpaperColors() {
        val listener = wallpaperColorsListener ?: return
        wallpaperColorsListener = null
        WallpaperManager.getInstance(applicationContext)
            .removeOnColorsChangedListener(listener)
    }

    external fun nativeOnWindowInsets(
        topPx: Float, bottomPx: Float,
        leftPx: Float, rightPx: Float,
        imeBottomPx: Float,
        cutoutLeft: Int, cutoutTop: Int, cutoutRight: Int, cutoutBottom: Int,
        gestureLeft: Int, gestureTop: Int, gestureRight: Int, gestureBottom: Int,
        mandatoryGestureLeft: Int, mandatoryGestureTop: Int,
        mandatoryGestureRight: Int, mandatoryGestureBottom: Int,
        tappableLeft: Int, tappableTop: Int, tappableRight: Int, tappableBottom: Int,
        statusVisible: Int, navigationVisible: Int, imeVisible: Int, captionVisible: Int,
        imeAnimationProgress: Float,
    )

    external fun nativeOnTheme(dynamicAvailable: Boolean, palette: ByteArray?)

    /** Memory pressure; the Activity lifecycle comes from winit instead. */
    external fun nativeOnMemoryPressure(level: Int)

    external fun nativeOnBack(): Int

    /**
     * Predictive-back gesture progress, for a subclass to forward.
     *
     * The platform SDK has no public way to register `OnBackAnimationCallback`,
     * so this Activity cannot observe the gesture itself; an app that needs the
     * phases registers the callback through AndroidX and calls this from it.
     */
    external fun nativeOnBackEvent(phase: Int, progress: Float)

    external fun nativeStatusBarVisible(): Boolean

    external fun nativeNavigationBarVisible(): Boolean

    external fun nativeLightBarIcons(): Boolean

    external fun nativeEdgeToEdge(): Int

    /**
     * An intent record was just written to the queue; wakes the native side so
     * it can drain it. Optional, like every other external here.
     */
    external fun nativeOnIntentQueued()

    /** `WindowInsets.isVisible` as the wire values the native side expects. */
    private fun visibility(insets: WindowInsets, typeMask: Int): Int = when {
        Build.VERSION.SDK_INT < 30 -> VISIBILITY_UNSET
        insets.isVisible(typeMask) -> VISIBILITY_VISIBLE
        else -> VISIBILITY_HIDDEN
    }

    /**
     * The bar state the native side owns, defaulting to both bars visible with
     * light icons if the library has no symbol.
     */
    private fun nativeBarState(): BooleanArray =
        try {
            booleanArrayOf(
                nativeStatusBarVisible(),
                nativeNavigationBarVisible(),
                nativeLightBarIcons(),
            )
        } catch (_: UnsatisfiedLinkError) {
            booleanArrayOf(true, true, true)
        }

    companion object {
        private const val TAG = "RlobKitMainActivity"

        // Mirrors rlobkit_app_events::system_bars::EdgeToEdgeMode.
        private const val EDGE_TO_EDGE_DISABLED = 0
        private const val EDGE_TO_EDGE_ENABLED = 1
        private const val EDGE_TO_EDGE_IMMERSIVE = 2

        // Mirrors rlobkit_app_events::back::BackOutcome.
        private const val BACK_CONSUMED = 0
        private const val BACK_PROPAGATE = 1
        private const val BACK_CROSS_ACTIVITY = 2

        // Mirrors rlobkit_app_events::back::EVENT_*.
        private const val EVENT_STARTED = 0
        private const val EVENT_PROGRESS = 1
        private const val EVENT_CANCELLED = 2
        private const val EVENT_INVOKED = 3

        // Mirrors rlobkit_app_events::insets::RegionVisibility.
        private const val VISIBILITY_VISIBLE = 0
        private const val VISIBILITY_HIDDEN = 1
        private const val VISIBILITY_UNSET = 2

        private const val TRIM_MEMORY_COMPLETE = 80

        @Volatile
        private var current: WeakReference<RlobKitMainActivity>? = null

        /**
         * Re-applies the system-bar state the native side owns. Callable from
         * any thread; the call itself does not change the state.
         */
        @JvmStatic
        fun refreshSystemBars() {
            val activity = current?.get() ?: return
            activity.runOnUiThread { activity.applySystemBars() }
        }

        /**
         * Fires the device vibrator for `millis` at `amplitude` (1..255).
         * Android has a single device vibrator shared by every gamepad, so
         * this is device-wide haptics rather than per-pad rumble. Callable
         * from any thread; a call before the Activity exists is a no-op.
         */
        @JvmStatic
        fun rumble(millis: Long, amplitude: Int) {
            val activity = current?.get() ?: return
            if (millis <= 0) return
            activity.runOnUiThread {
                try {
                    val vibrator = deviceVibrator(activity) ?: return@runOnUiThread
                    if (Build.VERSION.SDK_INT >= 26) {
                        vibrator.vibrate(VibrationEffect.createOneShot(millis, amplitude.coerceIn(1, 255)))
                    } else {
                        // Pre-26 has no VibrationEffect and no amplitude control.
                        @Suppress("DEPRECATION")
                        vibrator.vibrate(millis)
                    }
                } catch (e: Exception) {
                    // A throw would reach the JNI caller or the UI thread's
                    // uncaught handler; missing VIBRATE permission lands here.
                    Log.w(TAG, "vibrate failed", e)
                }
            }
        }

        /**
         * Cancels any vibration [rumble] started. Same threading and
         * pre-Activity rules as [rumble].
         */
        @JvmStatic
        fun rumbleStop() {
            val activity = current?.get() ?: return
            activity.runOnUiThread {
                try {
                    val vibrator = deviceVibrator(activity) ?: return@runOnUiThread
                    vibrator.cancel()
                } catch (e: Exception) {
                    Log.w(TAG, "rumbleStop failed", e)
                }
            }
        }

        @Suppress("DEPRECATION")
        private fun deviceVibrator(activity: RlobKitMainActivity): Vibrator? =
            if (Build.VERSION.SDK_INT >= 31) {
                activity.getSystemService(VibratorManager::class.java)?.defaultVibrator
            } else {
                activity.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
            }
    }
}