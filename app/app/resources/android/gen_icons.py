#!/usr/bin/env python3
"""Generate octos-one launcher icons (mipmap-*/ic_launcher.png).

Mark: rounded-square deep-teal tile, cream ring, 8 orbiting dots (octos).
Colors come from the app theme in app/src/main.rs (ai_panel / ai_cream / ai_cyan).
"""
import math
import os
from PIL import Image, ImageDraw

SIZES = {
    "mipmap-mdpi": 48,
    "mipmap-hdpi": 72,
    "mipmap-xhdpi": 96,
    "mipmap-xxhdpi": 144,
    "mipmap-xxxhdpi": 192,
}

BG = (10, 58, 48, 255)        # ai_panel  #0A3A30
RING = (243, 227, 199, 255)   # ai_cream  #F3E3C7
DOT = (114, 228, 255, 255)    # ai_cyan   #72E4FF

OUT = os.path.join(os.path.dirname(__file__))

def draw_icon(px: int) -> Image.Image:
    ss = 4  # supersample for smooth edges
    S = px * ss
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    r = int(S * 0.22)  # corner radius
    d.rounded_rectangle([0, 0, S - 1, S - 1], radius=r, fill=BG)
    cx = cy = S / 2
    ring_r = S * 0.27
    ring_w = max(2, int(S * 0.055))
    d.ellipse([cx - ring_r, cy - ring_r, cx + ring_r, cy + ring_r],
              outline=RING, width=ring_w)
    dot_r = S * 0.045
    orbit = S * 0.40
    for i in range(8):
        a = math.radians(i * 45 - 90)
        x = cx + orbit * math.cos(a)
        y = cy + orbit * math.sin(a)
        d.ellipse([x - dot_r, y - dot_r, x + dot_r, y + dot_r], fill=DOT)
    return img.resize((px, px), Image.LANCZOS)

def main():
    base = os.path.join(os.path.dirname(os.path.abspath(__file__)), "res")
    for folder, px in SIZES.items():
        d = os.path.join(base, folder)
        os.makedirs(d, exist_ok=True)
        draw_icon(px).save(os.path.join(d, "ic_launcher.png"))
        print(f"{folder}/ic_launcher.png {px}px")

if __name__ == "__main__":
    main()
