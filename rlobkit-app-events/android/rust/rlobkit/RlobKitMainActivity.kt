package rust.rlobkit

import android.app.NativeActivity
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.WindowInsets

/**
 * Shared NativeActivity subclass used by all rlobkit-based apps.
 *
 * Handles:
 * - ACTION_VIEW intents (writes to pending_intent file)
 * - Window insets / IME (calls nativeOnWindowInsets via JNI)
 *
 * Reference this activity in your Cargo.toml:
 *
 *   [[package.metadata.android.application.activity]]
 *   name = "rust.rlobkit.RlobKitMainActivity"
 *   exported = true
 *   launch_mode = "singleTask"
 */
class RlobKitMainActivity : NativeActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        RlobKitIntentBridge.captureViewIntent(intent, contentResolver, filesDir)
        super.onCreate(savedInstanceState)
        // NativeActivity loads the .so via dlopen internally, but that does
        // NOT register JNI functions.  We need System.loadLibrary so the VM
        // can resolve our nativeOnWindowInsets JNI symbol.
        loadLibraryForJni()
        setupWindowInsetsListener()
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

    private fun setupWindowInsetsListener() {
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false)
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

    companion object {
        private const val TAG = "RlobKitMainActivity"
    }
}
