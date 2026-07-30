package dev.octos.splashnative;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.view.View;

/**
 * The lit fraction of the moon, drawn.
 *
 * `phase` is position in the synodic cycle: 0 new, 0.25 first quarter, 0.5 full.
 *
 * The terminator is the projection of the great circle dividing the sphere's lit half,
 * so seen face-on it is a HALF-ELLIPSE whose width tracks cos(2π·phase). That is why a
 * crescent's inner edge curves while its outer edge is a true circular limb. Two
 * overlapping circles — the tempting shortcut — give a lens-shaped crescent that is wrong
 * at every phase except the quarters.
 *
 * A dark limb stays faintly visible rather than becoming a hole, so the disc still reads
 * as a sphere at new moon instead of vanishing into the card.
 */
final class MoonPhaseView extends View {
    private final Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path lit = new Path();
    private final RectF oval = new RectF();
    private final double phase;

    MoonPhaseView(Context c, double phase) {
        super(c);
        this.phase = ((phase % 1.0) + 1.0) % 1.0;
    }

    @Override protected void onMeasure(int w, int h) {
        int d = Math.min(MeasureSpec.getSize(w), MeasureSpec.getSize(h));
        if (d <= 0) d = Math.round(56 * getResources().getDisplayMetrics().density);
        setMeasuredDimension(d, d);
    }

    @Override protected void onDraw(Canvas c) {
        float w = getWidth(), h = getHeight();
        if (w <= 0 || h <= 0) return;
        float r = Math.min(w, h) * 0.46f;
        float cx = w / 2f, cy = h / 2f;

        // The unlit sphere: dark, but never absent.
        p.setStyle(Paint.Style.FILL);
        p.setColor(0xFF25293A);
        c.drawCircle(cx, cy, r, p);

        // k = cos(2π·phase): +1 at new, 0 at the quarters, -1 at full. Its magnitude is
        // the terminator ellipse's half-width; its sign says which side is lit.
        double k = Math.cos(2 * Math.PI * phase);
        boolean waxing = phase < 0.5;

        lit.reset();
        if (Math.abs(k) < 0.999) {
            // Half a disc, plus a half-ellipse whose width is |k|·r. Union when the
            // terminator bulges outward (gibbous), difference when it cuts in (crescent).
            float ex = (float) (r * Math.abs(k));
            oval.set(cx - ex, cy - r, cx + ex, cy + r);
            if (waxing) {
                lit.addArc(cx - r, cy - r, cx + r, cy + r, -90, 180);   // right limb
                lit.arcTo(oval, 90, k >= 0 ? -180 : 180);
            } else {
                lit.addArc(cx - r, cy - r, cx + r, cy + r, 90, 180);    // left limb
                lit.arcTo(oval, -90, k >= 0 ? 180 : -180);
            }
            lit.close();
        } else if (k < 0) {
            lit.addCircle(cx, cy, r, Path.Direction.CW);                 // full
        }

        p.setColor(0xFFF5F3E8);
        c.drawPath(lit, p);

        // Maria: broad, very low-contrast darkenings. Without them a large disc reads as
        // a flat token; with more contrast it reads as a cartoon.
        p.setColor(0x1A000000);
        c.save();
        c.clipPath(lit);
        c.drawCircle(cx - r * 0.22f, cy + r * 0.20f, r * 0.30f, p);
        c.drawCircle(cx + r * 0.26f, cy + r * 0.08f, r * 0.22f, p);
        c.drawCircle(cx + r * 0.06f, cy - r * 0.34f, r * 0.18f, p);
        c.restore();
    }
}
