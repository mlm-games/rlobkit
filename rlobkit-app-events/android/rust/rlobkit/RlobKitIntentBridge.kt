package rust.rlobkit

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import android.util.Log
import java.io.File

/**
 * Captures ACTION_VIEW / ACTION_SEND / ACTION_SEND_MULTIPLE intents delivered to
 * a NativeActivity and queues them for the Rust side.
 *
 * What is written to disk is a *record* of the intent — the content URIs, names,
 * types, sizes and grant flags — not the files' contents. A shared video can be
 * gigabytes, and reading it on the main thread blocked the Activity for as long
 * as the copy took while holding the whole thing in memory. The native side opens
 * the URI when it actually wants the bytes.
 *
 * Records are queued rather than overwritten, so a second share arriving before
 * the app has consumed the first does not silently drop it.
 *
 * Usage from a NativeActivity subclass:
 *
 *   class MyActivity : NativeActivity() {
 *       override fun onCreate(savedInstanceState: Bundle?) {
 *           RlobKitIntentBridge.capture(intent, contentResolver, filesDir)
 *           super.onCreate(savedInstanceState)
 *       }
 *       override fun onNewIntent(intent: Intent) {
 *           super.onNewIntent(intent)
 *           RlobKitIntentBridge.capture(intent, contentResolver, filesDir)
 *       }
 *   }
 */
object RlobKitIntentBridge {
    private const val TAG = "RlobKitIntentBridge"
    private const val QUEUE_DIR = "pending_intents"
    private const val SUFFIX = ".bin"

    /** Record layout, mirrored by `rlobkit_app_events::intents`. */
    private const val MAGIC = 0x514B4C52 // "RLKQ" little-endian
    private const val VERSION = 1
    private const val NONE = -1

    private const val ACTION_VIEW = 0
    private const val ACTION_SEND = 1
    private const val ACTION_SEND_MULTIPLE = 2

    /**
     * Queue an incoming share for the native side. A no-op for an intent with no
     * action this app handles.
     *
     * Runs on the main thread but only queries the provider for names and sizes,
     * so it stays cheap regardless of how large the shared files are.
     */
    @JvmStatic
    fun capture(intent: Intent?, resolver: ContentResolver, filesDir: File) {
        if (intent == null) return
        val action = when (intent.action) {
            Intent.ACTION_VIEW -> ACTION_VIEW
            Intent.ACTION_SEND -> ACTION_SEND
            Intent.ACTION_SEND_MULTIPLE -> ACTION_SEND_MULTIPLE
            else -> return
        }

        val uris = collectUris(intent)
        // A share of plain text carries no file; ACTION_SEND with EXTRA_TEXT is
        // still worth forwarding, so only bail when there is nothing at all.
        val text = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        } else {
            @Suppress("DEPRECATION")
            intent.getStringExtra(Intent.EXTRA_TEXT)
        }
        if (uris.isEmpty() && text.isNullOrEmpty()) return

        val grantFlags = intent.flags and PERMISSION_GRANTS
        persistPermissions(resolver, uris, grantFlags)

        val record = ByteWriter()
        record.putInt(MAGIC)
        record.putByte(VERSION)
        record.putByte(action)
        record.putString(intent.type)
        record.putString(text)
        record.putInt(grantFlags)
        record.putInt(uris.size)
        for (uri in uris) {
            val meta = queryMetadata(uri, resolver)
            record.putString(uri.toString())
            record.putString(meta.name)
            record.putString(meta.mimeType ?: intent.type)
            record.putLong(meta.size)
        }

        enqueue(record.toByteArray(), filesDir, action, uris.size)
    }

    /**
     * Every content URI the intent carries: `ACTION_VIEW`'s data, or the
     * `ACTION_SEND` payload from either `EXTRA_STREAM` or `ClipData`.
     *
     * Both are read because senders disagree about which to use, and a share
     * sheet may supply either.
     */
    private fun collectUris(intent: Intent): List<Uri> {
        val uris = LinkedHashSet<Uri>()
        if (intent.action == Intent.ACTION_VIEW) {
            intent.data?.let(uris::add)
            return uris.toList()
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)?.let(uris::add)
        } else {
            @Suppress("DEPRECATION")
            (intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri)?.let(uris::add)
        }
        intent.clipData?.let { clip ->
            for (i in 0 until clip.itemCount) {
                clip.getItemAt(i).uri?.let(uris::add)
            }
        }
        return uris.toList()
    }

    /**
     * Take the persistable grant the sender offered, so the URI stays readable
     * after this Activity finishes.
     *
     * Best effort by nature: a sender that set `PERSISTABLE` without actually
     * granting it throws here, and plenty of senders never set the flag at all.
     * The native side treats `is_persisted` as permission to ask for, not proof.
     */
    private fun persistPermissions(
        resolver: ContentResolver,
        uris: List<Uri>,
        grantFlags: Int,
    ) {
        if (uris.isEmpty()) return
        if (grantFlags and Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION == 0) return
        val takeFlags = grantFlags and
            (Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        for (uri in uris) {
            try {
                resolver.takePersistableUriPermission(uri, takeFlags)
            } catch (e: SecurityException) {
                Log.w(TAG, "no persistable grant for $uri: ${e.message}")
            } catch (e: UnsupportedOperationException) {
                Log.w(TAG, "provider does not persist $uri: ${e.message}")
            }
        }
    }

    private class Metadata(val name: String, val mimeType: String?, val size: Long)

    /**
     * The provider's display name and size.
     *
     * A `content://` path segment is usually an opaque id rather than a filename,
     * so `DISPLAY_NAME` is the only trustworthy source; the MIME type is the
     * fallback for naming, since it at least tells the user what they shared.
     */
    private fun queryMetadata(uri: Uri, resolver: ContentResolver): Metadata {
        var name: String? = null
        var size = -1L
        var mimeType: String? = null
        try {
            resolver.query(
                uri,
                arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE),
                null,
                null,
                null,
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val nameColumn = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (nameColumn >= 0 && !cursor.isNull(nameColumn)) {
                        name = cursor.getString(nameColumn)
                    }
                    val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
                    if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) {
                        size = cursor.getLong(sizeColumn)
                    }
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "queryMetadata failed for $uri: ${e.message}")
        }
        try {
            mimeType = resolver.getType(uri)
        } catch (e: Exception) {
            Log.w(TAG, "getType failed for $uri: ${e.message}")
        }
        if (name.isNullOrBlank()) {
            name = mimeType?.let { "shared-${it.substringAfter('/')}" } ?: "shared-file"
        }
        return Metadata(name, mimeType, size)
    }

    /**
     * Append a record to the queue.
     *
     * The name sorts lexicographically into capture order because it is
     * milliseconds zero-padded to 13 digits followed by a per-process counter;
     * the native side relies on that to return the oldest intent first.
     */
    private fun enqueue(bytes: ByteArray, filesDir: File, action: Int, fileCount: Int) {
        try {
            val dir = File(filesDir, QUEUE_DIR)
            if (!dir.isDirectory && !dir.mkdirs()) {
                throw java.io.IOException("cannot create $dir")
            }
            val name = "%013d%04d%s".format(
                System.currentTimeMillis(),
                SEQUENCE.getAndIncrement() and 0xFFFF,
                SUFFIX,
            )
            val target = File(dir, name)
            val tmp = File(dir, "$name.tmp")
            tmp.writeBytes(bytes)
            if (!tmp.renameTo(target)) {
                throw java.io.IOException("rename failed")
            }
            Log.i(TAG, "queued action=$action files=$fileCount as ${target.name} (${bytes.size} B)")
        } catch (e: Exception) {
            Log.e(TAG, "enqueue failed: ${e.message}", e)
        }
    }

    private val SEQUENCE = java.util.concurrent.atomic.AtomicInteger()

    private const val PERMISSION_GRANTS =
        Intent.FLAG_GRANT_READ_URI_PERMISSION or
            Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
            Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
            Intent.FLAG_GRANT_PREFIX_URI_PERMISSION

    /** Little-endian writer matching `rlobkit_app_events::intents`. */
    private class ByteWriter {
        private var out = java.io.ByteArrayOutputStream(128)

        fun putByte(value: Int) {
            out.write(value and 0xFF)
        }

        fun putInt(value: Int) {
            putByte(value)
            putByte(value shr 8)
            putByte(value shr 16)
            putByte(value shr 24)
        }

        fun putLong(value: Long) {
            putInt((value and 0xFFFFFFFFL).toInt())
            putInt((value shr 32).toInt())
        }

        fun putString(value: String?) {
            if (value == null) {
                putInt(NONE)
                return
            }
            val encoded = value.toByteArray(Charsets.UTF_8)
            putInt(encoded.size)
            out.write(encoded)
        }

        fun toByteArray(): ByteArray = out.toByteArray()
    }
}