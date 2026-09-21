package rust.rlobkit

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.util.Log
import java.io.File

/**
 * Captures ACTION_VIEW intents delivered to a NativeActivity and persists
 * them so the Rust side can consume them after the native library loads.
 *
 * Usage from a NativeActivity subclass:
 *
 *   class MyActivity : NativeActivity() {
 *       override fun onCreate(savedInstanceState: Bundle?) {
 *           RlobKitIntentBridge.captureViewIntent(intent, contentResolver, filesDir)
 *           super.onCreate(savedInstanceState)
 *       }
 *       override fun onNewIntent(intent: Intent) {
 *           super.onNewIntent(intent)
 *           RlobKitIntentBridge.captureViewIntent(intent, contentResolver, filesDir)
 *       }
 *   }
 */
object RlobKitIntentBridge {
    private const val TAG = "RlobKitIntentBridge"
    private const val PENDING_FILE = "pending_intent"

    /** Read the content:// URI from a VIEW intent and save it atomically. */
    @JvmStatic
    fun captureViewIntent(intent: Intent?, resolver: ContentResolver, filesDir: File) {
        if (intent?.action != Intent.ACTION_VIEW) return
        val uri = intent.data ?: return

        @Suppress("DEPRECATION")
        val mimeType = intent.type
        val displayName = queryDisplayName(uri, resolver)
        Log.i(TAG, "captureViewIntent uri=$uri mime=$mimeType name=$displayName")

        val bytes = readUriBytes(uri, resolver) ?: return
        savePendingIntent(bytes, filesDir, displayName)
    }

    private fun queryDisplayName(uri: Uri, resolver: ContentResolver): String? {
        return try {
            resolver.query(uri, arrayOf("_display_name"), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            } ?: uri.lastPathSegment
        } catch (e: Exception) {
            Log.w(TAG, "queryDisplayName failed: ${e.message}")
            uri.lastPathSegment
        }
    }

    private fun readUriBytes(uri: Uri, resolver: ContentResolver): ByteArray? {
        return try {
            resolver.openInputStream(uri)?.use { stream ->
                stream.readBytes()
            }
        } catch (e: Exception) {
            Log.e(TAG, "readUriBytes failed: ${e.message}", e)
            null
        }
    }

    private fun savePendingIntent(data: ByteArray, filesDir: File, displayName: String? = null) {
        try {
            val tmp = File(filesDir, "${PENDING_FILE}.tmp")
            val dst = File(filesDir, PENDING_FILE)
            tmp.writeBytes(data)
            if (!tmp.renameTo(dst)) {
                throw java.io.IOException("rename failed")
            }
            if (!displayName.isNullOrEmpty()) {
                File(filesDir, "$PENDING_FILE.name").writeText(displayName)
            }
            Log.i(TAG, "savePendingIntent: saved ${data.size} bytes name=$displayName")
        } catch (e: Exception) {
            Log.e(TAG, "savePendingIntent failed: ${e.message}", e)
        }
    }
}
