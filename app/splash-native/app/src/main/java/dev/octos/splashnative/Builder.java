package dev.octos.splashnative;

import android.content.Context;
import android.graphics.Color;
import android.graphics.drawable.GradientDrawable;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.LinearLayout;
import android.widget.TextView;

/**
 * Node tree → native Android views.
 *
 * Framework widgets only ({@code android.widget.*}) — no Material dependency, so the
 * experiment answers "does a Splash card render natively" without also answering "does
 * it look like Material". Everything visual comes from the card's own attributes, which
 * is what makes this a fair test of the DSL rather than of a design system.
 *
 * Java owns every View created here. Rust never sees one.
 */
final class Builder {
    private final Context ctx;

    Builder(Context ctx) { this.ctx = ctx; }

    private int dp(float v) {
        return Math.round(TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP, v, ctx.getResources().getDisplayMetrics()));
    }

    /** ARGB comes across as a double, because the wire format carries numbers as f64. */
    private static int argb(double v) { return (int) (long) v; }

    /**
     * A Material type token → (sp, bold).
     *
     * The lowering names a ROLE, never a size — that is the whole point of roles, and it
     * is what lets the same plan look native on each backend. Ignoring the token was why
     * an early build rendered every line at the same default size and read as flat: the
     * card was saying "displayMedium" and the builder was hearing nothing.
     */
    private static float[] type(String variant) {
        switch (variant == null ? "" : variant) {
            case "displayLarge":  return new float[]{57, 0};
            case "displayMedium": return new float[]{45, 0};
            case "displaySmall":  return new float[]{36, 0};
            case "headlineLarge": return new float[]{32, 0};
            case "headlineMedium":return new float[]{28, 0};
            case "headlineSmall": return new float[]{24, 0};
            case "titleLarge":    return new float[]{22, 0};
            case "titleMedium":   return new float[]{16, 1};
            case "titleSmall":    return new float[]{14, 1};
            case "bodyLarge":     return new float[]{16, 0};
            case "bodyMedium":    return new float[]{14, 0};
            case "bodySmall":     return new float[]{12, 0};
            case "labelLarge":    return new float[]{14, 1};
            case "labelMedium":   return new float[]{12, 1};
            case "labelSmall":    return new float[]{11, 1};
            default:              return new float[]{14, 0};
        }
    }

    View build(Node n) {
        if (n == null) return null;
        switch (n.kind) {
            case "col":
            case "row": {
                LinearLayout l = new LinearLayout(ctx);
                boolean col = "col".equals(n.kind);
                l.setOrientation(col ? LinearLayout.VERTICAL : LinearLayout.HORIZONTAL);
                if (!col) l.setGravity(Gravity.CENTER_VERTICAL);
                if (n.has("bg")) l.setBackgroundColor(argb(n.f("bg", 0)));
                int p = dp(n.f("pad", 0));
                if (p > 0) l.setPadding(p, p, p, p);
                int gap = dp(n.f("spacing", n.f("gap", 0)));
                for (int i = 0; i < n.children.size(); i++) {
                    View cv = build(n.children.get(i));
                    if (cv == null) continue;
                    LinearLayout.LayoutParams lp = childParams(n.children.get(i), col);
                    if (gap > 0 && i > 0) {
                        if (col) lp.topMargin = gap; else lp.leftMargin = gap;
                    }
                    l.addView(cv, lp);
                }
                return l;
            }
            case "card": {
                // A card is a padded, rounded, coloured column. Deliberately NOT a
                // FrameLayout wrapper: several direct children of one would stack on top
                // of each other, which reads as a data bug and is a container bug.
                LinearLayout l = new LinearLayout(ctx);
                l.setOrientation(LinearLayout.VERTICAL);
                GradientDrawable bg = new GradientDrawable();
                bg.setColor(n.has("bg") ? argb(n.f("bg", 0)) : 0x14FFFFFF);
                bg.setCornerRadius(dp(n.f("radius", 14)));
                l.setBackground(bg);
                int p = dp(n.f("pad", 12));
                l.setPadding(p, p, p, p);
                int gap = dp(n.f("spacing", n.f("gap", 6)));
                for (int i = 0; i < n.children.size(); i++) {
                    View cv = build(n.children.get(i));
                    if (cv == null) continue;
                    LinearLayout.LayoutParams lp = childParams(n.children.get(i), true);
                    if (gap > 0 && i > 0) lp.topMargin = gap;
                    l.addView(cv, lp);
                }
                return l;
            }
            case "text": {
                TextView t = new TextView(ctx);
                t.setText(n.s("text", ""));
                float[] ty = type(n.s("variant"));
                // An explicit size wins; otherwise the role decides. A role-only card is
                // the normal case and must not fall back to one flat size.
                t.setTextSize(TypedValue.COMPLEX_UNIT_SP, n.has("size") ? n.f("size", 14) : ty[0]);
                boolean bold = ty[1] > 0 || n.f("weight", 4) >= 6;
                if (bold) t.setTypeface(t.getTypeface(), android.graphics.Typeface.BOLD);
                // Captions and labels recede; everything else is primary. Colour is the
                // theme's, not the card's, so the card stays legible on any background.
                int fg = Color.WHITE;
                String v = n.s("variant", "");
                if (v.startsWith("label") || "bodySmall".equals(v)) fg = 0x99FFFFFF;
                else if ("bodyMedium".equals(v)) fg = 0xCCFFFFFF;
                t.setTextColor(n.has("color") ? argb(n.f("color", 0)) : fg);
                if (ty[0] >= 36) t.setLetterSpacing(-0.02f);
                return t;
            }
            case "weathericon": {
                // The condition glyph. A drawn icon rather than the "[weathericon]"
                // placeholder an unknown kind gets — a weather card whose weather is a
                // bracketed word is not a weather card.
                WeatherIconView w = new WeatherIconView(ctx, (int) n.f("cond", n.f("bind_cond", 1)));
                return w;
            }
            case "tempbar": {
                // The forecast range bar. A gradient keyed to POSITION IN THE WEEK, not
                // to absolute degrees: a week spanning 26-37 keyed absolutely sits
                // entirely in the warm half and every bar draws the same colour.
                View v = new View(ctx);
                GradientDrawable g = new GradientDrawable(
                    GradientDrawable.Orientation.LEFT_RIGHT,
                    new int[]{ ramp(n.f("lo", 0), n.f("wmin", 0), n.f("wmax", 1)),
                               ramp(n.f("hi", 1), n.f("wmin", 0), n.f("wmax", 1)) });
                g.setCornerRadius(dp(3));
                v.setBackground(g);
                return v;
            }
            case "spacer": {
                View v = new View(ctx);
                v.setMinimumHeight(dp(n.f("h", 8)));
                v.setMinimumWidth(dp(n.f("w", 0)));
                return v;
            }
            case "divider": {
                View v = new View(ctx);
                v.setMinimumHeight(dp(1));
                v.setBackgroundColor(n.has("color") ? argb(n.f("color", 0)) : 0x1AFFFFFF);
                return v;
            }
            case "box": {
                View v = new View(ctx);
                GradientDrawable bg = new GradientDrawable();
                bg.setColor(n.has("bg") ? argb(n.f("bg", 0)) : Color.TRANSPARENT);
                bg.setCornerRadius(dp(n.f("radius", 0)));
                v.setBackground(bg);
                return v;
            }
            default: {
                // An unknown kind is NAMED on screen, never dropped. A silently missing
                // section looks like a complete card that is quietly wrong.
                TextView t = new TextView(ctx);
                t.setText("[" + n.kind + "]");
                t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 11);
                t.setTextColor(0xFFFF9F0A);
                return t;
            }
        }
    }

    /** Cool→warm by position in the week. Same nine-stop palette as the makepad TempBar. */
    private static int ramp(double t, double wmin, double wmax) {
        double span = Math.max(0.001, wmax - wmin);
        double p = Math.max(0, Math.min(1, (t - wmin) / span));
        int[] stops = {0xFF1E5CFF, 0xFF00A3FF, 0xFF00D9C0, 0xFF3FBF52, 0xFFC6E016,
                       0xFFFFC400, 0xFFFF8A00, 0xFFFF4B10, 0xFFE01B1B};
        double x = p * (stops.length - 1);
        int i = Math.min((int) Math.floor(x), stops.length - 2);
        double f = x - i;
        return lerp(stops[i], stops[i + 1], f);
    }

    private static int lerp(int a, int b, double f) {
        int ar = (a >> 16) & 0xFF, ag = (a >> 8) & 0xFF, ab = a & 0xFF;
        int br = (b >> 16) & 0xFF, bg = (b >> 8) & 0xFF, bb = b & 0xFF;
        return 0xFF000000
            | ((int) Math.round(ar + (br - ar) * f) << 16)
            | ((int) Math.round(ag + (bg - ag) * f) << 8)
            | (int) Math.round(ab + (bb - ab) * f);
    }

    /** Explicit w/h win; otherwise a child fills the cross axis and wraps the main one. */
    private LinearLayout.LayoutParams childParams(Node c, boolean parentIsColumn) {
        int w = c.has("w") ? dp(c.f("w", 0))
            : (parentIsColumn ? ViewGroup.LayoutParams.MATCH_PARENT
                              : ViewGroup.LayoutParams.WRAP_CONTENT);
        int h = c.has("h") ? dp(c.f("h", 0)) : ViewGroup.LayoutParams.WRAP_CONTENT;
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(w, h);
        if (!parentIsColumn && !c.has("w") && c.f("grow", 0) > 0) {
            lp.width = 0;
            lp.weight = (float) c.f("grow", 1);
        }
        return lp;
    }
}
