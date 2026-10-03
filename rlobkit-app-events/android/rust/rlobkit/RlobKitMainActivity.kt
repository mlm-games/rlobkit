package rust.rlobkit

import android.app.NativeActivity
import android.app.WallpaperManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.media.VibrationEffect
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import java.lang.ref.WeakReference

/**
 * Shared NativeActivity subclass used by all rlobkit-based apps.
 *
 * Handles:
 * - ACTION_VIEW intents (writes to pending_intent file)
 * - Window insets / IME (calls nativeOnWindowInsets via JNI)
 * - The dynamic theme palette (calls nativeOnTheme via JNI)
 * - System bars, re-applied on request from the native side (refreshSystemBars)
 * - Device vibration on request from the native side (rumble / rumbleStop)
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

    override fun onCreate(savedInstanceState: Bundle?) {
        RlobKitIntentBridge.captureViewIntent(intent, contentResolver, filesDir)
        super.onCreate(savedInstanceState)
        current = WeakReference(this)
        loadLibraryForJni()
        setupWindowInsetsListener()
        applySystemBars()
        pushDynamicTheme()
        watchWallpaperColors()
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        pushDynamicTheme()
    }

    override fun onResume() {
        super.onResume()
        applySystemBars()
    }

    override fun onDestroy() {
        super.onDestroy()
        unwatchWallpaperColors()
        if (current?.get() === this) current = null
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) applySystemBars()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        RlobKitIntentBridge.captureViewIntent(intent, contentResolver, filesDir)
        setIntent(Intent(Intent.ACTION_MAIN))
    }

    private fun loadLibraryForJni() {
        try {
            val ai = packageManager.getActivityInfo(
                ComponentName(this, javaClass),
                PackageManager.GET_META_DATA,
            )
            val libName = ai.metaData?.getString("android.app.lib_name") ?: "main"
            System.loadLibrary(libName)
        } catch (e: Exception) {
            Log.e(TAG, "loadLibraryForJni failed", e)
        }
    }

    private fun applySystemBars() {
        val (statusVisible, navigationVisible, lightIcons) = nativeBarState()
        val immersive = !statusVisible && !navigationVisible
        if (Build.VERSION.SDK_INT >= 30) {
            // Only a fully immersive window draws behind the bars; with one bar
            // left showing, fitting the window keeps the visible bar's space
            // reserved instead of moving the layout under it.
            window.setDecorFitsSystemWindows(!immersive)
            if (immersive && Build.VERSION.SDK_INT >= 28) {
                window.attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
            val controller = window.insetsController ?: return
            val hidden = (if (statusVisible) 0 else WindowInsets.Type.statusBars()) or
                (if (navigationVisible) 0 else WindowInsets.Type.navigationBars())
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
            window.decorView.systemUiVisibility =
                View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                (if (statusVisible) 0 else View.SYSTEM_UI_FLAG_FULLSCREEN) or
                (if (navigationVisible) 0 else View.SYSTEM_UI_FLAG_HIDE_NAVIGATION) or
                (if (immersive) {
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                        View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                        View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                } else {
                    0
                }) or
                (if (lightIcons) {
                    0
                } else {
                    View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
                        (if (Build.VERSION.SDK_INT >= 26) {
                            View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
                        } else {
                            0
                        })
                })
        }
    }

    private fun setupWindowInsetsListener() {
        if (Build.VERSION.SDK_INT >= 30) {
            window.decorView.setOnApplyWindowInsetsListener { view, insets ->
                val systemBars = insets.getInsets(WindowInsets.Type.systemBars())
                val ime = insets.getInsets(WindowInsets.Type.ime())
                nativeOnWindowInsets(
                    systemBars.top.toFloat(),
                    systemBars.bottom.toFloat(),
                    systemBars.left.toFloat(),
                    systemBars.right.toFloat(),
                    ime.bottom.toFloat(),
                )
                view.onApplyWindowInsets(insets)
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.setOnApplyWindowInsetsListener { view, insets ->
                nativeOnWindowInsets(
                    insets.systemWindowInsetTop.toFloat(),
                    insets.systemWindowInsetBottom.toFloat(),
                    insets.systemWindowInsetLeft.toFloat(),
                    insets.systemWindowInsetRight.toFloat(),
                    insets.systemWindowInsetBottom.toFloat(),
                )
                view.onApplyWindowInsets(insets)
            }
        }
    }

    external fun nativeOnWindowInsets(
        topPx: Float, bottomPx: Float,
        leftPx: Float, rightPx: Float,
        imeBottomPx: Float,
    )

    external fun nativeOnTheme(data: ByteArray)

    /**
     * Hands the current system palette to the native side. Silent when the
     * host app does not link the JNI bridge or runs below Android 12.
     */
    private fun pushDynamicTheme() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return
        val dark = DynamicTheme.isNight(resources.configuration)
        val bytes = DynamicTheme.build(resources, dark) ?: return
        try {
            nativeOnTheme(bytes)
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    private fun watchWallpaperColors() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || wallpaperColorsListener != null) return
        val listener = WallpaperManager.OnColorsChangedListener { _, _ ->
            runOnUiThread { pushDynamicTheme() }
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

    external fun nativeStatusBarVisible(): Boolean

    external fun nativeNavigationBarVisible(): Boolean

    external fun nativeLightBarIcons(): Boolean

    /**
     * (statusVisible, navigationVisible, lightIcons), defaulting to both bars
     * visible with light icons if the native library has no symbol.
     */
    private fun nativeBarState(): Triple<Boolean, Boolean, Boolean> =
        try {
            Triple(
                nativeStatusBarVisible(),
                nativeNavigationBarVisible(),
                nativeLightBarIcons(),
            )
        } catch (_: UnsatisfiedLinkError) {
            Triple(true, true, true)
        }

    companion object {
        private const val TAG = "RlobKitMainActivity"

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
