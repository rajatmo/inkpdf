package app.inkpdf.test;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.util.Log;
import android.view.PointerIcon;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.Toast;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * InkPDF's activity: the NativeActivity that loads the Rust library, with the few things only Java can do for it: the system's file picker (the
 * Storage Access Framework), reading and writing what it hands out, the clipboard and opening links. The Rust side (platform/android/host.rs)
 * calls the public methods below through JNI and polls for answers (takePicked, takeLaunch), so no native callback is needed.
 */
public class Host extends NativeActivity {
    private static final int PICK = 7001;

    /** What the picker answered: "kind TAB address" (kind 0 open, 1 save; no address: the person gave up). Taken once. */
    private volatile String picked;
    /** The address of a document another app handed this one. Taken once. */
    private volatile String launched;
    volatile String lastError = "";
    private int pendingKind;
    /** Whether the system bars are hidden (the full-screen button): applied again when the window regains focus, as the system shows them after a dialog. */
    private volatile boolean barsHidden;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        takeFrom(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        takeFrom(intent);
    }

    private void takeFrom(Intent intent) {
        if (intent != null && Intent.ACTION_VIEW.equals(intent.getAction()) && intent.getData() != null) {
            Uri uri = intent.getData();
            keep(uri, intent.getFlags());
            launched = uri.toString();
        }
    }

    private void keep(Uri uri, int flags) {
        try {
            int wanted = flags & (Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            if (wanted != 0) {
                getContentResolver().takePersistableUriPermission(uri, wanted);
            }
        } catch (Exception e) {
            // (Not every address can be kept; it still works for as long as the app runs.)
        }
    }

    /** Hide the system bars (status and navigation) so the window fills the screen, or show them. A swipe from the edge shows them for a moment. */
    public void setBars(boolean hidden) {
        barsHidden = hidden;
        runOnUiThread(new Task(this, Task.BARS, hidden ? 1 : 0, null));
    }

    void doBars() {
        boolean hidden = barsHidden;
        Window window = getWindow();
        if (hidden && Build.VERSION.SDK_INT >= 28) {
            // (Under a camera cut-out too: the whole screen.)
            WindowManager.LayoutParams params = window.getAttributes();
            params.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(params);
        }
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController controller = window.getInsetsController();
            if (controller != null) {
                if (hidden) {
                    controller.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                    controller.hide(WindowInsets.Type.systemBars());
                } else {
                    controller.show(WindowInsets.Type.systemBars());
                }
            }
        } else {
            View decor = window.getDecorView();
            int flags = View.SYSTEM_UI_FLAG_LAYOUT_STABLE;
            if (hidden) {
                flags |= View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                        | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION;
            }
            decor.setSystemUiVisibility(flags);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus && barsHidden) {
            doBars();
        }
    }

    /** The file's own name (the provider's DISPLAY_NAME), or null if it will not say. */
    public String displayName(String address) {
        try (android.database.Cursor cursor = getContentResolver().query(Uri.parse(address), new String[] {android.provider.OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                return cursor.getString(0);
            }
        } catch (Exception e) {
            lastError = String.valueOf(e);
        }
        return null;
    }

    /** Show this system pointer icon (a PointerIcon type) over the window, for a mouse or a hovering stylus: a finger has no pointer. */
    public void setPointer(int type) {
        runOnUiThread(new Task(this, Task.POINTER, type, null));
    }

    void doPointer(int type) {
        if (Build.VERSION.SDK_INT >= 24) {
            getWindow().getDecorView().setPointerIcon(PointerIcon.getSystemIcon(this, type));
        }
    }

    /** Ask the system for a PDF to open (kind 0) or for a place and a name to save under (kind 1). */
    public void pick(int kind, String name) {
        runOnUiThread(new Task(this, Task.PICK, kind, name));
    }

    void doPick(int kind, String name) {
        try {
            Intent intent = new Intent(kind == 0 ? Intent.ACTION_OPEN_DOCUMENT : Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("application/pdf");
            if (kind != 0 && name != null && !name.isEmpty()) {
                intent.putExtra(Intent.EXTRA_TITLE, name);
            }
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                    | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            pendingKind = kind;
            startActivityForResult(intent, PICK);
        } catch (Exception e) {
            lastError = String.valueOf(e);
            picked = kind + "\t";
        }
    }

    /** Work that has to be done on the UI thread. */
    private static final class Task implements Runnable {
        static final int PICK = 0;
        static final int COPY = 1;
        static final int URL = 2;
        static final int BARS = 3;
        static final int POINTER = 4;
        private final Host host;
        private final int what;
        private final int kind;
        private final String text;

        Task(Host host, int what, int kind, String text) {
            this.host = host;
            this.what = what;
            this.kind = kind;
            this.text = text;
        }

        @Override
        public void run() {
            try {
                if (what == PICK) {
                    host.doPick(kind, text);
                } else if (what == BARS) {
                    host.doBars();
                } else if (what == POINTER) {
                    host.doPointer(kind);
                } else if (what == COPY) {
                    ClipboardManager clipboard = (ClipboardManager) host.getSystemService(Context.CLIPBOARD_SERVICE);
                    clipboard.setPrimaryClip(ClipData.newPlainText("InkPDF", text == null ? "" : text));
                } else {
                    Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(text));
                    intent.addCategory(Intent.CATEGORY_BROWSABLE);
                    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                    host.startActivity(intent);
                }
            } catch (Exception e) {
                host.lastError = String.valueOf(e);
                Log.e("inkpdf", "could not do it (" + what + "): " + e);
                if (what == URL) {
                    // (Said on the screen too: a link that does nothing is the worst of it.)
                    Toast.makeText(host, "Could not open the link: " + e.getMessage(), Toast.LENGTH_LONG).show();
                }
            }
        }
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK) {
            return;
        }
        if (result == RESULT_OK && data != null && data.getData() != null) {
            Uri uri = data.getData();
            keep(uri, data.getFlags());
            picked = pendingKind + "\t" + uri;
        } else {
            picked = pendingKind + "\t";
        }
    }

    public String takePicked() {
        String answer = picked;
        picked = null;
        return answer;
    }

    public String takeLaunch() {
        String address = launched;
        launched = null;
        return address;
    }

    public String lastError() {
        return lastError;
    }

    /** The bytes at an address, or null (and lastError says why). */
    public byte[] read(String address) {
        try (InputStream in = getContentResolver().openInputStream(Uri.parse(address))) {
            if (in == null) {
                lastError = "the system gave no file";
                return null;
            }
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buffer = new byte[64 * 1024];
            int n;
            while ((n = in.read(buffer)) > 0) {
                out.write(buffer, 0, n);
            }
            return out.toByteArray();
        } catch (Exception e) {
            lastError = String.valueOf(e);
            return null;
        }
    }

    /** Replace what is at an address with the bytes: null if it worked, else why not. */
    public String write(String address, byte[] data) {
        try (OutputStream out = getContentResolver().openOutputStream(Uri.parse(address), "wt")) {
            if (out == null) {
                return "the system gave no file to write";
            }
            out.write(data);
            out.flush();
            return null;
        } catch (Exception e) {
            return String.valueOf(e);
        }
    }

    public void copy(String text) {
        runOnUiThread(new Task(this, Task.COPY, 0, text));
    }

    /** The text on the clipboard, or null. (An app in the background is not allowed to read it.) */
    public String paste() {
        try {
            ClipboardManager clipboard = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            ClipData clip = clipboard.getPrimaryClip();
            if (clip == null || clip.getItemCount() == 0) {
                return null;
            }
            CharSequence text = clip.getItemAt(0).coerceToText(this);
            return text == null ? null : text.toString();
        } catch (Exception e) {
            lastError = String.valueOf(e);
            return null;
        }
    }

    public void openUrl(String address) {
        runOnUiThread(new Task(this, Task.URL, 0, address));
    }
}
