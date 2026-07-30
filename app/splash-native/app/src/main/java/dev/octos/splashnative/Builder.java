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
                t.setTextSize(TypedValue.COMPLEX_UNIT_SP, n.f("size", 14));
                t.setTextColor(n.has("color") ? argb(n.f("color", 0)) : Color.WHITE);
                // The DSL's weight is a 1..9 scale; anything at 6 or above reads as bold.
                if (n.f("weight", 4) >= 6) t.setTypeface(t.getTypeface(), android.graphics.Typeface.BOLD);
                return t;
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
