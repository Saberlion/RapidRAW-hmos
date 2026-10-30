"""Device driver for RapidRAW/LightRaw store-screenshot capture (OHOS via hdc).

Uses the accessibility tree from `uitest dumpLayout`, which DOES expose ArkWeb
page content as text+bounds, so screens can be identified and clicked without
seeing the image.

Usage:
  python ui.py dump                 # print accessibility tree (text nodes)
  python ui.py find <substring>     # print matching nodes + center coords
  python ui.py click <substring>    # click center of first matching node
  python ui.py click-xy <x> <y>     # raw click
  python ui.py key <keycode>        # uitest uiInput keyEvent <code>
  python ui.py shot <name>          # snapshot_display + recv to raw/<name>.jpeg
"""
import json
import os
import re
import subprocess
import sys

BUNDLE = "io.github.CyberTimon.RapidRAW"
RAW = os.path.join(os.path.dirname(os.path.abspath(__file__)), "raw")
DEV_LAYOUT = "/data/local/tmp/layout.json"


def sh(cmd: str, timeout: int = 60) -> str:
    p = subprocess.run(["hdc", "shell", cmd], capture_output=True, text=True,
                       encoding="utf-8", errors="replace", timeout=timeout)
    return (p.stdout or "") + (p.stderr or "")


def dump_nodes(all_windows: bool = False):
    flag = "-i " if all_windows else ""
    sh(f"uitest dumpLayout {flag}-p {DEV_LAYOUT}")
    local = os.path.join(RAW, "layout.json")
    subprocess.run(["hdc", "file", "recv", DEV_LAYOUT, local], capture_output=True, timeout=60)
    with open(local, encoding="utf-8") as f:
        root = json.load(f)
    out = []

    def walk(n, depth):
        if not isinstance(n, dict):
            return
        a = n.get("attributes") or {}
        out.append({
            "depth": depth,
            "type": a.get("type", ""),
            "text": (a.get("text") or "").strip(),
            "desc": (a.get("description") or "").strip(),
            "bounds": a.get("bounds", ""),
            "clickable": a.get("clickable") == "true",
            "checkable": a.get("checkable") == "true",
            "checked": a.get("checked") == "true",
            "bundle": a.get("bundleName", ""),
            "win": a.get("hostWindowId", ""),
        })
        for c in n.get("children") or []:
            walk(c, depth + 1)

    for r in (root if isinstance(root, list) else [root]):
        walk(r, 0)
    return out


def center(bounds: str):
    m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", bounds or "")
    if not m:
        return None
    x1, y1, x2, y2 = (int(v) for v in m.groups())
    return (x1 + x2) // 2, (y1 + y2) // 2


def main():
    args = sys.argv[1:]
    os.makedirs(RAW, exist_ok=True)
    if not args:
        print(__doc__)
        return
    cmd, rest = args[0], args[1:]

    if cmd == "dump":
        nodes = dump_nodes()
        for n in nodes:
            if n["text"] or n["clickable"]:
                print(f"{'  ' * min(n['depth'], 8)}[{n['type']}] {n['text'][:90]}"
                      f" @{n['bounds']}" + ("  <clickable>" if n["clickable"] else ""))
        print(f"-- {len(nodes)} nodes", file=sys.stderr)
    if cmd == "dumpall":
        nodes = dump_nodes(all_windows=True)
        for n in nodes:
            if n["text"] or n["clickable"]:
                print(f"[{n['bundle'] or '-'}] [{n['type']}] {n['text'][:70]} @{n['bounds']}"
                      + ("  <clickable>" if n["clickable"] else ""))
        print(f"-- {len(nodes)} nodes", file=sys.stderr)
    elif cmd == "treefile":
        with open(os.path.join(RAW, "layout.json"), encoding="utf-8") as f:
            root = json.load(f)
        rows = []

        def walk(n, dep=0):
            if not isinstance(n, dict) or dep > 40:
                return
            a = n.get("attributes") or {}
            rows.append(f"{'  ' * dep}{a.get('type','')} {a.get('bounds','')} "
                        f"clk={a.get('clickable')} id={(a.get('id') or '')[:30]} "
                        f"desc={(a.get('description') or '')[:40]!r} "
                        f"txt={(a.get('text') or '')[:60]!r}")
            for c in (n.get("children") or []):
                walk(c, dep + 1)

        walk(root)
        out = os.path.join(RAW, "tree.txt")
        with open(out, "w", encoding="utf-8") as f:
            f.write("\n".join(rows))
        print(f"{out}  ({len(rows)} lines)")
    elif cmd == "tree":
        with open(os.path.join(RAW, "layout.json"), encoding="utf-8") as f:
            root = json.load(f)
        lim = int(rest[0]) if rest else 45

        def walk(n, dep=0):
            if not isinstance(n, dict) or dep > 12:
                return
            a = n.get("attributes") or {}
            print("  " * dep + f"{a.get('type','')} {a.get('bounds','')} "
                  f"bundle={a.get('bundleName','')} id={(a.get('id') or '')[:28]} "
                  f"txt={(a.get('text') or '')[:40]!r}")
            for c in (n.get("children") or []):
                walk(c, dep + 1)

        walk(root)
    elif cmd == "images":
        for n in dump_nodes():
            t = n["type"].lower()
            if any(k in t for k in ("image", "img", "canvas")):
                print(f"[{n['type']}] {n['bounds']} txt={n['text'][:60]!r}")
    elif cmd == "desc":
        with open(os.path.join(RAW, "layout.json"), encoding="utf-8") as f:
            root = json.load(f)
        pat = rest[0].lower() if rest else ""

        def walk(n):
            if not isinstance(n, dict):
                return
            a = n.get("attributes") or {}
            t, x, ds = a.get("type", ""), a.get("text", "") or "", a.get("description", "") or ""
            if (t in ("Button", "popUpButton", "Toggle", "Radio", "Checkbox") or x or ds) \
                    and pat in (x + ds).lower():
                print(f"{t:14} {a.get('bounds',''):26} clk={a.get('clickable')} "
                      f"desc={ds[:40]!r} txt={x[:40]!r}")
            for c in (n.get("children") or []):
                walk(c)

        walk(root)
    elif cmd == "find":
        pat = rest[0].lower()
        for n in dump_nodes():
            if pat in n["text"].lower():
                print(f"{n['bounds']} c={center(n['bounds'])} click={n['clickable']} :: {n['text'][:100]}")
    elif cmd == "click":
        pat = rest[0].lower()
        nodes = dump_nodes()

        def rank(n):
            t = n["text"].lower()
            if t == pat:
                return (0 if n["clickable"] else 1)
            if t.startswith(pat):
                return (2 if n["clickable"] else 3)
            return (4 if n["clickable"] else 5)

        hits = sorted([n for n in nodes if pat in n["text"].lower()], key=rank)
        if not hits:
            print(f"NO MATCH for {pat!r}")
            sys.exit(2)
        n = hits[0]
        cx, cy = center(n["bounds"])
        print(f"click {cx},{cy} :: [{n['type']}] {n['text'][:80]} clickable={n['clickable']}")
        sh(f"uitest uiInput click {cx} {cy}")
    elif cmd == "click-xy":
        x, y = rest[0], rest[1]
        print(sh(f"uitest uiInput click {x} {y}"))
    elif cmd == "dclick-xy":
        x, y = rest[0], rest[1]
        print(sh(f"uitest uiInput doubleClick {x} {y}"))
    elif cmd == "key":
        print(sh(f"uitest uiInput keyEvent {rest[0]}"))
    elif cmd == "shot":
        name = rest[0]
        dev = f"/data/local/tmp/{name}.jpeg"
        print(sh(f"snapshot_display -f {dev}", timeout=120).strip().splitlines()[-1])
        dst = os.path.join(RAW, f"{name}.jpeg")
        subprocess.run(["hdc", "file", "recv", dev, dst], capture_output=True, timeout=120)
        size = os.path.getsize(dst)
        print(f"{dst}  {size // 1024}KB")
    else:
        print(__doc__)


if __name__ == "__main__":
    main()
