package dev.octos.splashnative;

import android.app.Activity;
import android.graphics.Color;
import android.os.Bundle;
import android.util.Log;
import android.util.TypedValue;
import android.view.View;
import android.view.ViewGroup;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;

/**
 * Renders a card octos-one's LLM produced, with no makepad anywhere.
 *
 * The card text is read from octos-one's own saved cards when they are reachable, and
 * from a bundled copy otherwise. Reading the REAL file is the point: a transcription
 * would quietly drift from what the model actually emits, and this experiment exists to
 * test the real output.
 */
public class MainActivity extends Activity {
    static final String TAG = "SplashNative";

    /**
     * Where to look for a card octos-one's LLM generated, in order.
     *
     * The app's own files dir is private to it, so a second app cannot read it — the
     * earlier runs all fell back to the bundled copy and said so on screen. A handoff
     * directory is the honest fix for an experiment: octos-one's saved card is COPIED
     * there verbatim, so what renders is still the model's own bytes rather than a
     * transcription. A real integration would put the renderer inside octos-one instead
     * of alongside it.
     */
    static final String[] CARD_DIRS = {
        "/storage/emulated/0/Android/media/dev.makepad.octos_app/cards/",
        "/data/local/tmp/octos-cards/",
        "/data/data/dev.makepad.octos_app/files/a2app_cards/",
    };

    LinearLayout root;
    ScrollView scroll;

    @Override protected void onCreate(Bundle b) {
        super.onCreate(b);
        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(0xFF0B0B0D);
        scroll = new ScrollView(this);
        scroll.addView(root, new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        setContentView(scroll);

        // Evaluating a card performs live HTTP, which cannot run on the UI thread.
        note("Evaluating card…");
        new Thread(this::render, "splash-eval").start();
    }

    void render() {
        String name = getIntent() != null && getIntent().getStringExtra("card") != null
            ? getIntent().getStringExtra("card") : "weather-app";
        String src = readCard(name);
        final String origin = src.isEmpty() ? "no card found" : cardOrigin;

        if (src.isEmpty()) {
            runOnUiThread(() -> {
                root.removeAllViews();
                note("No card to render");
                note("Looked in " + String.join(", ", CARD_DIRS) + " and in assets/");
                note("Generate one in octos-one first, then relaunch.");
            });
            return;
        }

        ByteBuffer bb = Native.renderCard(src);
        final String diag = Native.diag();
        final String caps = Native.capabilities();
        final String stats = Native.fetchStats();

        runOnUiThread(() -> {
            root.removeAllViews();
            if (bb == null) {
                note("render returned nothing — that is a bug, not an empty card");
                return;
            }
            try {
                Node n = Node.decode(bb);
                View v = new Builder(this).build(n);
                if (v != null) {
                    root.addView(v, new LinearLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.WRAP_CONTENT));
                }
            } catch (Throwable t) {
                Log.e(TAG, "build failed", t);
                note("build failed: " + t);
            }
            // Provenance LAST: it is evidence for a reviewer, not chrome for a user. A
            // card whose first three lines are diagnostics reads as a debug screen.
            note("");
            note("card: " + name + " — " + origin);
            note("VM: ymote/Splash (splash-core) · render: android.widget.* · makepad: none");
            note("fetch: " + stats);
            if (diag != null && !diag.isEmpty()) note("diag: " + diag);
            note("sys.*: " + caps);
        });
    }

    String cardOrigin = "";

    /** octos-one's saved card if readable, else the bundled copy. */
    String readCard(String name) {
        for (String dir : CARD_DIRS) {
            File f = new File(dir + name + ".splash");
            if (!f.canRead()) continue;
            try (InputStream in = new FileInputStream(f)) {
                cardOrigin = "octos-one's LLM output (" + dir + ")";
                return slurp(in);
            } catch (Throwable t) {
                Log.w(TAG, "cannot read " + f, t);
            }
        }
        try (InputStream in = getAssets().open("cards/" + name + ".splash")) {
            cardOrigin = "bundled copy (octos-one's card dir not readable)";
            return slurp(in);
        } catch (Throwable t) {
            Log.w(TAG, "no bundled card either", t);
            return "";
        }
    }

    static String slurp(InputStream in) throws Exception {
        byte[] buf = new byte[Math.max(in.available(), 1024)];
        int n = 0, r;
        while ((r = in.read(buf, n, buf.length - n)) > 0) {
            n += r;
            if (n == buf.length) {
                byte[] bigger = new byte[buf.length * 2];
                System.arraycopy(buf, 0, bigger, 0, n);
                buf = bigger;
            }
        }
        return new String(buf, 0, n, StandardCharsets.UTF_8);
    }

    void note(String s) {
        TextView t = new TextView(this);
        t.setText(s);
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 10);
        t.setTextColor(0x99FFFFFF);
        t.setBackgroundColor(Color.TRANSPARENT);
        int p = Math.round(6 * getResources().getDisplayMetrics().density);
        t.setPadding(p, p / 2, p, p / 2);
        root.addView(t);
    }
}
