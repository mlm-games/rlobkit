package rust.rlobkit;

import android.app.Activity;
import android.content.ComponentName;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.util.Log;

import java.io.File;

public final class RlobKitPickerActivity extends Activity {
    private static final String TAG = "RlobKitPickerActivity";

    private static final String EXTRA_TARGET_INTENT = "rust.rlobkit.extra.TARGET_INTENT";
    private static final String EXTRA_REQUEST_CODE = "rust.rlobkit.extra.REQUEST_CODE";
    private static final String EXTRA_OPEN_FD = "rust.rlobkit.extra.OPEN_FD";
    private static final String NATIVE_LIB_META_KEY = "android.app.lib_name";
    private static final int REQUEST_CREATE_DOCUMENT = 41004;

    private static final int RESULT_CANCELED = 0;
    private static final int FLAG_GRANT_READ_URI_PERMISSION = Intent.FLAG_GRANT_READ_URI_PERMISSION;
    private static final int FLAG_GRANT_WRITE_URI_PERMISSION = Intent.FLAG_GRANT_WRITE_URI_PERMISSION;
    private static final int FLAG_GRANT_PERSISTABLE_URI_PERMISSION = Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION;
    private static final int FLAG_GRANT_PREFIX_URI_PERMISSION = Intent.FLAG_GRANT_PREFIX_URI_PERMISSION;
    private static volatile boolean nativeLibraryLoaded = false;

    private static native void nativeOnActivityResult(int requestCode, int resultCode, Intent data);

    private String resolveNativeLibraryName() {
        try {
            ComponentName componentName = new ComponentName(getPackageName(), "android.app.NativeActivity");
            ActivityInfo info = getPackageManager().getActivityInfo(componentName, PackageManager.GET_META_DATA);
            if (info.metaData != null) {
                String value = info.metaData.getString(NATIVE_LIB_META_KEY);
                if (value != null && !value.isEmpty()) {
                    return value;
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "Failed to resolve native lib name from NativeActivity metadata", e);
        }
        return null;
    }

    private void ensureNativeLibraryLoaded() {
        if (nativeLibraryLoaded) {
            return;
        }

        String libName = resolveNativeLibraryName();
        if (libName != null) {
            try {
                System.loadLibrary(libName);
                nativeLibraryLoaded = true;
                return;
            } catch (UnsatisfiedLinkError e) {
                Log.w(TAG, "Failed to load native library by name: " + libName, e);
            }
        }

        String nativeDir = getApplicationInfo().nativeLibraryDir;
        if (nativeDir == null || nativeDir.isEmpty()) {
            return;
        }

        File dir = new File(nativeDir);
        File[] files = dir.listFiles();
        if (files == null) {
            return;
        }

        for (File file : files) {
            String name = file.getName();
            if (!name.endsWith(".so")) {
                continue;
            }
            try {
                System.load(file.getAbsolutePath());
                nativeLibraryLoaded = true;
                return;
            } catch (UnsatisfiedLinkError ignored) {
                // keep trying other libraries
            }
        }
    }

    private int grantFlagsFromResult(int intentFlags) {
        return intentFlags & (FLAG_GRANT_READ_URI_PERMISSION
                | FLAG_GRANT_WRITE_URI_PERMISSION
                | FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                | FLAG_GRANT_PREFIX_URI_PERMISSION);
    }

    private void takePersistableGrant(Uri uri, int grantFlags) {
        if (uri == null) {
            return;
        }
        if ((grantFlags & FLAG_GRANT_PERSISTABLE_URI_PERMISSION) == 0) {
            Log.i(TAG, "Skipping persist grant (no persistable flag) for uri=" + uri + " flags=0x" + Integer.toHexString(grantFlags));
            return;
        }

        int rwFlags = grantFlags & (FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION);
        if (rwFlags == 0) {
            rwFlags = FLAG_GRANT_READ_URI_PERMISSION;
        }

        try {
            Log.i(TAG, "takePersistableUriPermission uri=" + uri + " flags=0x" + Integer.toHexString(grantFlags) + " rw=0x" + Integer.toHexString(rwFlags));
            safResolver().takePersistableUriPermission(uri, rwFlags);
        } catch (SecurityException e) {
            Log.w(TAG, "Failed to persist URI permission for " + uri, e);
        }
    }

    private void grantSelfUriPermission(Uri uri, int grantFlags) {
        if (uri == null) {
            return;
        }

        int uriGrantFlags = grantFlags & (
                FLAG_GRANT_READ_URI_PERMISSION
                        | FLAG_GRANT_WRITE_URI_PERMISSION
                        | FLAG_GRANT_PREFIX_URI_PERMISSION
        );
        if ((uriGrantFlags & (FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION)) == 0) {
            uriGrantFlags |= FLAG_GRANT_READ_URI_PERMISSION;
        }

        try {
            grantUriPermission(getPackageName(), uri, uriGrantFlags);
            Log.i(TAG, "grantUriPermission(self) uri=" + uri + " flags=0x" + Integer.toHexString(uriGrantFlags));
        } catch (SecurityException e) {
            Log.w(TAG, "Failed to grant self URI permission for " + uri, e);
        }
    }

    private void persistResultGrants(Intent data) {
        if (data == null) {
            return;
        }

        int grantFlags = grantFlagsFromResult(data.getFlags());

        Uri dataUri = data.getData();
        if (dataUri != null) {
            grantSelfUriPermission(dataUri, grantFlags);
            takePersistableGrant(dataUri, grantFlags);
        }

        if (data.getClipData() != null) {
            int count = data.getClipData().getItemCount();
            for (int i = 0; i < count; i++) {
                Uri clipUri = data.getClipData().getItemAt(i).getUri();
                if (clipUri != null) {
                    grantSelfUriPermission(clipUri, grantFlags);
                    takePersistableGrant(clipUri, grantFlags);
                }
            }
        }
    }

    private void attachWritableFdForCreateDocument(int requestCode, Intent data) {
        if (requestCode != REQUEST_CREATE_DOCUMENT || data == null) {
            return;
        }

        Uri dataUri = data.getData();
        if (dataUri == null) {
            return;
        }

        ContentResolver resolver = safResolver();

        for (String mode : new String[] {"wt", "w", "rwt", "rw"}) {
            try {
                ParcelFileDescriptor pfd = resolver.openFileDescriptor(dataUri, mode);
                if (pfd == null) {
                    Log.w(TAG, "openFileDescriptor returned null for " + dataUri + " mode=" + mode);
                    continue;
                }

                int detachedFd = pfd.detachFd();
                data.putExtra(EXTRA_OPEN_FD, detachedFd);
                Log.i(TAG, "Attached writable fd for uri=" + dataUri + " fd=" + detachedFd + " mode=" + mode);
                return;
            } catch (SecurityException e) {
                Log.w(TAG, "Failed to open writable file descriptor for " + dataUri + " mode=" + mode, e);
            } catch (Exception e) {
                Log.w(TAG, "Unexpected failure opening writable file descriptor for " + dataUri + " mode=" + mode, e);
            }
        }
    }

    private void dispatchNativeResult(int requestCode, int resultCode, Intent data) {
        ensureNativeLibraryLoaded();
        try {
            nativeOnActivityResult(requestCode, resultCode, data);
        } catch (UnsatisfiedLinkError e) {
            Log.e(TAG, "nativeOnActivityResult is unavailable", e);
        }
    }

    private ContentResolver safResolver() {
        try {
            Context packageContext = createPackageContext(getPackageName(), 0);
            return packageContext.getContentResolver();
        } catch (Exception e) {
            Log.w(TAG, "Falling back to activity content resolver", e);
            return getContentResolver();
        }
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        ensureNativeLibraryLoaded();

        Intent launcherIntent = getIntent();
        if (launcherIntent == null) {
            finish();
            return;
        }

        Intent targetIntent = launcherIntent.getParcelableExtra(EXTRA_TARGET_INTENT);
        int requestCode = launcherIntent.getIntExtra(EXTRA_REQUEST_CODE, -1);

        if (targetIntent == null || requestCode < 0) {
            finish();
            return;
        }

        startActivityForResult(targetIntent, requestCode);
    }

    @Override
    @SuppressWarnings("deprecation")
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        Log.i(TAG, "onActivityResult request=" + requestCode + " result=" + resultCode + " hasData=" + (data != null) + (data != null ? " flags=0x" + Integer.toHexString(data.getFlags()) : ""));
        persistResultGrants(data);
        attachWritableFdForCreateDocument(requestCode, data);
        dispatchNativeResult(requestCode, resultCode, data);

        setResult(RESULT_CANCELED);
        finish();
    }
}
