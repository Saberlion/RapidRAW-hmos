"""Crude ASCII view of a screenshot so the layout can be reasoned about without vision.

Prints a luminance ramp plus colour hints:
  '.' near-black  ':' dark  'o' mid  'O' bright  '#' near-white
  'R'/'G'/'B' strongly saturated channel dominance
  'y'/'c'/'m' mixed saturated (yellow/cyan/magenta-ish)
"""
import sys
from PIL import Image

RAMP = " .:-=+*#%@"


def main():
    path = sys.argv[1]
    cols = int(sys.argv[2]) if len(sys.argv) > 2 else 78
    im = Image.open(path).convert("RGB")
    if len(sys.argv) >= 7:
        im = im.crop(tuple(int(v) for v in sys.argv[3:7]))
    w, h = im.size
    rows = max(1, int(cols * h / w * 0.5))
    small = im.resize((cols, rows), Image.BOX)
    px = small.load()
    print(f"{path}  {w}x{h} -> {cols}x{rows}")
    for y in range(rows):
        line = []
        for x in range(cols):
            r, g, b = px[x, y]
            mx, mn = max(r, g, b), min(r, g, b)
            sat = mx - mn
            lum = (r * 299 + g * 587 + b * 114) // 1000
            if sat > 60 and mx > 60:
                if mx == r and g >= b:
                    ch = "R" if r - max(g, b) > 40 else "y"
                elif mx == g and b >= r:
                    ch = "c" if g - min(r, b) > 40 else "g"
                elif mx == b:
                    ch = "B"
                else:
                    ch = "m"
            else:
                ch = RAMP[min(len(RAMP) - 1, lum * len(RAMP) // 256)]
            line.append(ch)
        print("".join(line))


if __name__ == "__main__":
    main()
