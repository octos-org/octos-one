package dev.octos.splashnative;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.view.View;

/**
 * The condition glyph, drawn.
 *
 * A CPU-drawn peer of makepad's shader-animated `WeatherIcon`. Same eight condition
 * indices, so a card asking for condition 3 gets rain on both backends — that agreement
 * is what makes the index a shared contract rather than a makepad detail.
 *
 * Static rather than animated: makepad's rays rotate and its rain falls because a
 * fragment shader runs every frame anyway. Reproducing that here would mean an
 * invalidate loop for decoration, which is a poor trade on a scrolling card. The
 * degradation is deliberate and worth naming.
 */
final class WeatherIconView extends View {
    private final Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path path = new Path();
    private final int cond;

    WeatherIconView(Context c, int cond) {
        super(c);
        this.cond = cond;
    }

    @Override protected void onMeasure(int w, int h) {
        // Square, and sized by the layout params the card set.
        int d = Math.min(MeasureSpec.getSize(w), MeasureSpec.getSize(h));
        if (d <= 0) d = Math.round(28 * getResources().getDisplayMetrics().density);
        setMeasuredDimension(d, d);
    }

    @Override protected void onDraw(Canvas c) {
        float w = getWidth(), h = getHeight();
        if (w <= 0 || h <= 0) return;
        float s = Math.min(w, h);
        switch (cond) {
            case 0: sun(c, w / 2, h / 2, s * 0.30f, true); break;             // clear
            case 1: sun(c, w * 0.36f, h * 0.36f, s * 0.20f, true);
                    cloud(c, w * 0.56f, h * 0.60f, s * 0.34f, 0xFFC9D3E3); break;  // partly
            case 2: cloud(c, w * 0.50f, h * 0.54f, s * 0.40f, 0xFFB6C0D2); break;  // cloudy
            case 3: cloud(c, w * 0.50f, h * 0.44f, s * 0.36f, 0xFFA8B4C8);
                    drops(c, w, h, s, 0xFF5BA4F5); break;                    // rain
            case 4: cloud(c, w * 0.50f, h * 0.44f, s * 0.36f, 0xFF8E99AC);
                    bolt(c, w, h, s); break;                                  // storm
            case 5: cloud(c, w * 0.50f, h * 0.44f, s * 0.36f, 0xFFC9D3E3);
                    drops(c, w, h, s, 0xFFE8F1FF); break;                    // snow
            case 6: wind(c, w, h, s); break;                                  // wind
            case 7: fog(c, w, h, s); break;                                   // fog
            default: cloud(c, w * 0.5f, h * 0.54f, s * 0.40f, 0xFFB6C0D2);
        }
    }

    private void sun(Canvas c, float cx, float cy, float r, boolean rays) {
        p.setStyle(Paint.Style.FILL);
        if (rays) {
            p.setColor(0xFFFFC94D);
            p.setStrokeWidth(r * 0.22f);
            p.setStrokeCap(Paint.Cap.ROUND);
            for (int i = 0; i < 8; i++) {
                double a = Math.PI * i / 4.0;
                float x0 = cx + (float) Math.cos(a) * r * 1.45f;
                float y0 = cy + (float) Math.sin(a) * r * 1.45f;
                float x1 = cx + (float) Math.cos(a) * r * 1.95f;
                float y1 = cy + (float) Math.sin(a) * r * 1.95f;
                c.drawLine(x0, y0, x1, y1, p);
            }
        }
        p.setColor(0xFFFFB300);
        c.drawCircle(cx, cy, r, p);
        p.setColor(0xFFFFD466);
        c.drawCircle(cx, cy, r * 0.74f, p);
    }

    private void cloud(Canvas c, float cx, float cy, float r, int color) {
        p.setStyle(Paint.Style.FILL);
        p.setColor(color);
        c.drawCircle(cx - r * 0.55f, cy + r * 0.12f, r * 0.48f, p);
        c.drawCircle(cx + r * 0.45f, cy + r * 0.16f, r * 0.40f, p);
        c.drawCircle(cx - r * 0.05f, cy - r * 0.22f, r * 0.58f, p);
        c.drawRoundRect(cx - r * 0.95f, cy + r * 0.02f, cx + r * 0.90f, cy + r * 0.62f,
            r * 0.30f, r * 0.30f, p);
    }

    private void drops(Canvas c, float w, float h, float s, int color) {
        p.setColor(color);
        p.setStrokeWidth(s * 0.075f);
        p.setStrokeCap(Paint.Cap.ROUND);
        for (int i = 0; i < 3; i++) {
            float x = w * (0.34f + i * 0.16f);
            c.drawLine(x, h * 0.68f, x - s * 0.05f, h * 0.86f, p);
        }
    }

    private void bolt(Canvas c, float w, float h, float s) {
        p.setStyle(Paint.Style.FILL);
        p.setColor(0xFFFFC400);
        path.reset();
        path.moveTo(w * 0.52f, h * 0.60f);
        path.lineTo(w * 0.40f, h * 0.86f);
        path.lineTo(w * 0.50f, h * 0.84f);
        path.lineTo(w * 0.44f, h * 1.00f);
        path.lineTo(w * 0.64f, h * 0.76f);
        path.lineTo(w * 0.53f, h * 0.78f);
        path.close();
        c.drawPath(path, p);
    }

    private void wind(Canvas c, float w, float h, float s) {
        p.setStyle(Paint.Style.STROKE);
        p.setColor(0xFFB6C0D2);
        p.setStrokeWidth(s * 0.09f);
        p.setStrokeCap(Paint.Cap.ROUND);
        for (int i = 0; i < 3; i++) {
            float y = h * (0.34f + i * 0.17f);
            c.drawLine(w * 0.18f, y, w * (0.62f + i * 0.08f), y, p);
        }
        p.setStyle(Paint.Style.FILL);
    }

    private void fog(Canvas c, float w, float h, float s) {
        cloud(c, w * 0.50f, h * 0.40f, s * 0.32f, 0xFFB6C0D2);
        p.setStyle(Paint.Style.STROKE);
        p.setColor(0x99C9D3E3);
        p.setStrokeWidth(s * 0.08f);
        p.setStrokeCap(Paint.Cap.ROUND);
        for (int i = 0; i < 2; i++) {
            float y = h * (0.72f + i * 0.14f);
            c.drawLine(w * 0.20f, y, w * 0.80f, y, p);
        }
        p.setStyle(Paint.Style.FILL);
    }
}
