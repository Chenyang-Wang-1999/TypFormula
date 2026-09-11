"""The engine's own boxes for a list of formulas, from the real layout adapter.

Evidence for the measurements in `docs/kind-inventory.md` and for the spacing
question behind `Kind::Number`: the adapter compiles a document and reports the
width/height/baseline of the fragment it was asked to map.

    cargo build --offline --locked --release --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
    python tools/engine_boxes.py                 # the default formula list
    python tools/engine_boxes.py "12" "1 2"      # your own bodies
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ADAPTER = ROOT / "target/adapter/release/visual-typst-layout.exe"

# Every entry is a math body, spliced into `$…$` so the adapter maps that range.
# The list covers what the inventory compares: a base, the marks that decorate it,
# whether a space between digit runs matters, and the constructs that are still Raw.
DEFAULT_BODIES = [
    "x",
    "hat(x)", "vec(x)", "overline(x)", "underline(x)", "cancel(x)", "tilde(x)", "dot(x)",
    "12", "1 2", "1  2", "1.5", "1 . 5", "1.2.3", ".5", "1.",
    "x 1", "1 x", "x1",
    "overbrace(x)", "x'", "x'''''",
]


def ask(body, size=24.0):
    prefix = f"#set text(size: {size}pt)\n$"
    source = prefix + body + "$"
    start = len(prefix)
    request = {"path": "main.typ", "source": source,
               "raw": [{"id": "0", "start": start, "end": start + len(body)}]}
    child = subprocess.run([str(ADAPTER), "--server"], input=(json.dumps(request) + "\n").encode(),
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=300)
    first = child.stdout.split(b"\n")[0]
    if not first:
        raise SystemExit("adapter died: " + child.stderr.read().decode("utf-8", "replace"))
    reply = json.loads(first)
    if "error" in reply:
        return {"error": reply["error"]}
    item = reply["items"][0]
    return {key: round(item[key], 4) for key in ("width", "height", "baseline")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bodies", nargs="*", help="math bodies (default: the inventory list)")
    parser.add_argument("--size", type=float, default=24.0, help="font size in pt")
    arguments = parser.parse_args()
    # A Windows console defaults to the local code page, which cannot print the
    # operators and marks these bodies contain.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not ADAPTER.is_file():
        raise SystemExit(f"缺少适配器：{ADAPTER}\n先运行 cargo build --offline --locked --release "
                         f"--manifest-path native-adapter/Cargo.toml --target-dir target/adapter")
    for body in (arguments.bodies or DEFAULT_BODIES):
        print(f"{body:14s} {json.dumps(ask(body, arguments.size), ensure_ascii=False)}")


if __name__ == "__main__":
    main()
