package rust.rlobkit

import android.app.Activity

class SystemBarsRunnable(
    private val activity: Activity,
    private val method: String,
    private val value: Boolean,
) : Runnable {
    override fun run() {
        try {
            val target =
                if (activity is RlobKitMainActivity) activity
                else return
            when (method) {
                "setImmersiveSticky" -> target.setImmersiveSticky(value)
                "setSystemBarsVisible" -> target.setSystemBarsVisible(value)
            }
        } catch (_: Exception) {
        }
    }
}
