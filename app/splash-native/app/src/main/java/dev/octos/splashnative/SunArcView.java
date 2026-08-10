package dev.octos.splashnative;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.view.View;

/**
 * The sun's daily path — a hairline arc from sunrise to sunset with the sun riding it at
 * the current time.
 *
 * `progress` is the fraction of daylight elapsed: 0 at sunrise, 1 at sunset. Outside that
 * range it is night, and the sun parks at the nearer horizon and dims rather than
 * disappearing — an empty box reads as a failed fetch.
 *
 * The arc is the top of a large circle whose chord IS the horizon line, so the curve meets
 * the horizon exactly at both ends instead of floating above them.
 */
final class SunArcView extends View {
    private final Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF oval = new RectF();
    private final double progress;

    SunArcView(Context c, double progress) { super(c); this.progress = progress; }

    @Override protected void onMeasure(int w, int h) {
        int width = MeasureSpec.getSize(w);
        int height = MeasureSpec.getSize(h);
        if (height <= 0) height = Math.round(76 * getResources().getDisplayMetrics().density);
        setMeasuredDimension(width, height);
    }

    @Override protected void onDraw(Canvas c) {
        float w = getWidth(), h = getHeight();
        if (w <= 0 || h <= 0) return;
        float pad = w * 0.06f;
        float half = (w - pad * 2) / 2f;
        float rise = h * 0.56f;
        float hy = h * 0.80f;
        // R = (half² + rise²) / (2·rise): the circle through both horizon ends whose top
        // sits `rise` above them.
        float rr = (half * half + rise * rise) / (2 * rise);
        float ccx = w / 2f, ccy = hy - rise + rr;

        p.setStyle(Paint.Style.STROKE);
        p.setStrokeWidth(Math.max(1.5f, h * 0.018f));
        p.setColor(0x4DFFFFFF);
        double th = Math.toDegrees(Math.asin(Math.min(1, half / rr)));
        oval.set(ccx - rr, ccy - rr, ccx + rr, ccy + rr);
        c.drawArc(oval, (float) (270 - th), (float) (2 * th), false, p);

        p.setStrokeWidth(Math.max(1f, h * 0.012f));
        p.setColor(0x26FFFFFF);
        c.drawLine(pad, hy, w - pad, hy, p);

        double t = Math.max(0, Math.min(1, progress));
        double ph = Math.toRadians(-th + 2 * th * t);
        float sx = (float) (ccx + rr * Math.sin(ph));
        float sy = (float) (ccy - rr * Math.cos(ph));

        boolean up = progress >= 0 && progress <= 1;
        // Low sun goes orange, the way real low-angle light does.
        double low = 1 - Math.min(1, Math.min(t, 1 - t) / 0.35);
        int disc = up
            ? blend(0xFFFFD75D, 0xFFFF9138, low)
            : 0x66FFB300;
        p.setStyle(Paint.Style.FILL);
        p.setColor(disc);
        c.drawCircle(sx, sy, h * 0.075f, p);
        p.setColor((up ? 0x33 : 0x11) << 24 | (disc & 0xFFFFFF));
        c.drawCircle(sx, sy, h * 0.14f, p);
    }

    private static int blend(int a, int b, double f) {
        f = Math.max(0, Math.min(1, f));
        int ar = (a >> 16) & 0xFF, ag = (a >> 8) & 0xFF, ab = a & 0xFF;
        int br = (b >> 16) & 0xFF, bg = (b >> 8) & 0xFF, bb = b & 0xFF;
        return 0xFF000000
            | ((int) Math.round(ar + (br - ar) * f) << 16)
            | ((int) Math.round(ag + (bg - ag) * f) << 8)
            | (int) Math.round(ab + (bb - ab) * f);
    }
}
