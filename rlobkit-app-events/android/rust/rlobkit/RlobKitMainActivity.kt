package rust.rlobkit

import android.app.NativeActivity
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
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
 * - System bars, on request from the native side (postImmersiveSticky)
 *
 * Reference this activity in your Cargo.toml:
 *
 *   [[package.metadata.android.application.activity]]
 *   name = "rust.rlobkit.RlobKitMainActivity"
 *   exported = true
 *   launch_mode = "singleTask"
 */
class RlobKitMainActivity : NativeActivity() {
    private var immersiveSticky: Boolean = false

    override fun onCreate(savedInstanceState: Bundle?) {
        RlobKitIntentBridge.captureViewIntent(intent, contentResolver, filesDir)
        super.onCreate(savedInstanceState)
        current = WeakReference(this)
        loadLibraryForJni()
        immersiveSticky = nativeImmersiveStickyOrFalse()
        setupWindowInsetsListener()
        applySystemBars()
    }

    override fun onResume() {
        super.onResume()
        applySystemBars()
    }

    override fun onDestroy() {
        super.onDestroy()
        if (current?.get() === this) current = null
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) applySystemBars()
    }

    fun setImmersiveSticky(hide: Boolean) {
        immersiveSticky = hide
        runOnUiThread { applySystemBars() }
    }

    fun setSystemBarsVisible(visible: Boolean) {
        setImmersiveSticky(!visible)
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
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(!immersiveSticky)
            if (immersiveSticky && Build.VERSION.SDK_INT >= 28) {
                window.attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
            val controller = window.insetsController ?: return
            if (immersiveSticky) {
                controller.hide(WindowInsets.Type.systemBars())
                controller.systemBarsBehavior =
                    WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            } else {
                controller.show(WindowInsets.Type.systemBars())
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = if (immersiveSticky) {
                (View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                    or View.SYSTEM_UI_FLAG_FULLSCREEN
                    or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                    or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                    or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION)
            } else {
                View.SYSTEM_UI_FLAG_LAYOUT_STABLE
            }
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

    external fun nativeImmersiveSticky(): Boolean

    private fun nativeImmersiveStickyOrFalse(): Boolean =
        try {
            nativeImmersiveSticky()
        } catch (_: UnsatisfiedLinkError) {
            false
        }

    companion object {
        private const val TAG = "RlobKitMainActivity"

        @Volatile
        private var current: WeakReference<RlobKitMainActivity>? = null

        /** Entry point for `rlobkit_app_events::system_bars`, callable from any thread. */
        @JvmStatic
        fun postImmersiveSticky(hide: Boolean) {
            val activity = current?.get() ?: return
            activity.runOnUiThread { activity.setImmersiveSticky(hide) }
        }
    }
}
