"""Turn raw hdc device captures into AppGallery-ready PC/2in1 store assets.

Input : raw/*.jpeg  (3120x2080 full-display captures, app window = [0,0][3120,1955],
                     system decor bar = top 70px, system dock = bottom 125px)
Output: packaging/appgallery/screenshots/*.png   1920x1080 16:9  (primary style)
        packaging/appgallery/screenshots/_alt-padded/*.png       (alternative style)
        packaging/appgallery/icon/*.png          1024 + 216 square icon
        packaging/appgallery/_preview/*.jpeg     contact sheets for eyeballing
"""
import os
import sys
from PIL import Image, ImageStat

HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "raw")
REPO = r"C:\workspace\RapidRAW-hmos"
OUT = os.path.join(REPO, "packaging", "appgallery")

WIN = (0, 0, 3120, 1955)          # app window incl. decor bar
DECOR = 70                        # system decor bar height
WEB = (WIN[0], WIN[1] + DECOR, WIN[2], WIN[3])   # webview area
TARGET = (1920, 1080)
DROP_BOTTOM = 130                 # px of webview dropped to reach 16:9

# raw file -> (order, stable name)
SHOTS = [
    ("s2_editor", "01-editor-adjust"),
    ("lib_dark", "02-library"),
    ("s4_masks", "03-masks"),
    ("s5_ai", "04-ai-retouch"),
    ("s6_presets", "05-presets"),
]


def edge_uniformity(im):
    """Return (max delta) between the 4 corner blocks — small => invisible padding."""
    w, h = im.size
    blocks = [(0, 0, 40, 40), (w - 40, 0, w, 40), (0, h - 40, 40, h), (w - 40, h - 40, w, h)]
    means = [tuple(round(v) for v in ImageStat.Stat(im.crop(b)).mean) for b in blocks]
    delta = max(max(abs(m[i] - means[0][i]) for i in range(3)) for m in means)
    return delta, means


def fit_pad(im):
    """Scale to fit 1080 tall, pad sides with the corner colour (invisible on a
    uniform-background app frame: measured corner delta is ~1/255)."""
    w, h = im.size
    scale = TARGET[1] / h
    new = im.resize((round(w * scale), TARGET[1]), Image.LANCZOS)
    delta, means = edge_uniformity(im)
    fill = means[0]
    canvas = Image.new("RGB", TARGET, fill)
    x = (TARGET[0] - new.size[0]) // 2
    canvas.paste(new, (x, 0))
    return canvas, fill, delta, means


def crop_169(im):
    """Keep full width, drop DROP_BOTTOM px from the bottom to reach 16:9."""
    w, _ = im.size
    keep = round(w * TARGET[1] / TARGET[0])
    return im.crop((0, 0, w, keep)).resize(TARGET, Image.LANCZOS)


def contact_sheet(paths, out, cols=3, cell=(640, 360), pad=10, bg=(32, 32, 32)):
    rows = (len(paths) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * cell[0] + pad * (cols + 1), rows * cell[1] + pad * (rows + 1)), bg)
    for i, p in enumerate(paths):
        im = Image.open(p).convert("RGB").resize(cell, Image.LANCZOS)
        cx = pad + (i % cols) * (cell[0] + pad)
        cy = pad + (i // cols) * (cell[1] + pad)
        sheet.paste(im, (cx, cy))
    sheet.save(out, quality=92)
    return out


def main():
    shots_dir = os.path.join(OUT, "screenshots")
    alt_dir = os.path.join(shots_dir, "_alt-padded")
    icon_dir = os.path.join(OUT, "icon")
    prev_dir = os.path.join(OUT, "_preview")
    for d in (shots_dir, alt_dir, icon_dir, prev_dir):
        os.makedirs(d, exist_ok=True)

    primary, alt = [], []
    for raw_name, out_name in SHOTS:
        src = os.path.join(RAW, raw_name + ".jpeg")
        if not os.path.exists(src):
            print(f"MISSING {src}")
            continue
        im = Image.open(src).convert("RGB").crop(WEB)

        # primary: nothing is cropped away; the 66px side pads match the frame colour
        padded, fill, delta, means = fit_pad(im)
        p1 = os.path.join(shots_dir, out_name + ".png")
        padded.save(p1, optimize=True)

        # alternative: full-bleed 16:9 (drops the bottom 130px of the app frame)
        cropped = crop_169(im)
        p2 = os.path.join(alt_dir, out_name + ".png")
        cropped.save(p2, optimize=True)

        kb1 = os.path.getsize(p1) // 1024
        kb2 = os.path.getsize(p2) // 1024
        print(f"{raw_name:12} -> {out_name:18} primary(fit+pad)={kb1:5}KB  alt(crop)={kb2:5}KB "
              f"pad_fill={fill} corner_delta={delta}")
        primary.append(p1)
        alt.append(p2)

    # icons straight from the in-package foreground art (identical to LightRaw.png)
    icon_src = os.path.join(REPO, "LightRaw.png")
    ic = Image.open(icon_src).convert("RGB")
    ic.resize((1024, 1024), Image.LANCZOS).save(os.path.join(icon_dir, "icon-1024.png"), optimize=True)
    ic.resize((216, 216), Image.LANCZOS).save(os.path.join(icon_dir, "icon-216.png"), optimize=True)
    print("icon source:", icon_src, ic.size, "-> icon-1024.png (%.0fKB), icon-216.png (%.0fKB)" % (
        os.path.getsize(os.path.join(icon_dir, "icon-1024.png")) / 1024,
        os.path.getsize(os.path.join(icon_dir, "icon-216.png")) / 1024))

    print("contact sheet:", contact_sheet(primary, os.path.join(prev_dir, "screenshots-primary.jpeg")))
    print("contact sheet:", contact_sheet(alt, os.path.join(prev_dir, "screenshots-padded-alt.jpeg")))
    print("icon (1024 -> ascii, 40 cols):")


if __name__ == "__main__":
    main()
