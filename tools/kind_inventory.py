"""What each `Kind` actually puts on the wire, taken from the real backend.

Evidence for `docs/kind-inventory.md`: it drives the release backend over the
desktop protocol and prints, per `Kind`, the node the frontend would receive.

    cargo build --offline --locked --release --bin visual-typst --target-dir target/server
    python tools/kind_inventory.py            # the tables, as text
    python tools/kind_inventory.py --json out.json

No Qt and no window: the protocol is one JSON request per line on stdin, one
JSON reply per line on stdout.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BACKEND = ROOT / "target/server/release/visual-typst.exe"

# One source per `Kind`, plus the actions that reach the two states no source can
# express. `input` after the formula is active is how a command draft is typed.
CASES = [
    ("Char", "$x$", []),
    ("Symbol", "$alpha$", []),
    ("Number", "$12.5$", []),
    ("Raw", "$arrow.r$", []),
    ("Unknown", "$x$", [("input", {"text": "\\"})]),
    ("Text", '$"txt"$', []),
    ("MacroCall", "#let twice(a) = $ #a + 1 $\n$ twice(x) $", []),
    ("Fraction", "$frac(a, b)$", []),
    ("Sqrt", "$sqrt(x)$", []),
    ("Root", "$root(3, x)$", []),
    ("Scripts", "$x^2$", []),
    ("Fenced", "$(a)$", []),
    ("Table", "$mat(1, 2; 3, 4)$", []),
    ("Multiline", "$a &= 1 \\ b &= 2$", []),
    ("Accent", "$hat(x)$", []),
    ("Line", "$overline(x)$", []),
    # The two kinds that exist only inside a macro template: neither is reachable
    # from source, so what this shows is their absence.
    ("TemplateCall/Parameter", "#let inner(x) = $ #x $\n#let outer(a) = $ frac(inner(#a), 2) $\n$ outer(y) $", []),
    # Constructs the vocabulary alignment is about, none of which is its own Kind yet.
    ("a/b", "$a/b$", []),
    ("cancel", "$cancel(x)$", []),
    ("vec", "$vec(x)$", []),
    ("primes", "$x'$", []),
]

# The `View` fields the frontend can read (`src/view.rs`).
WIRE = ["kind", "role", "text", "display_glyph", "columns", "attachment", "edit",
        "definitions", "origin", "source_range", "active", "selected"]

# Views that belong to a node kind (`slots::Decl::view`), for the summary.
VIEW_KINDS = ["char", "symbol", "number", "raw", "unknown", "text", "macro", "macro-collapsed",
              "macro-argument", "template-call", "parameter", "fraction", "sqrt", "root",
              "script", "delim", "grid", "aligned", "decoration", "line", "absent"]


def call(child, payload):
    child.stdin.write((json.dumps(payload, ensure_ascii=False) + "\n").encode())
    child.stdin.flush()
    line = child.stdout.readline()
    if not line:
        raise SystemExit("backend died: " + child.stderr.read().decode("utf-8", "replace"))
    return json.loads(line)


def probe(source, actions):
    child = subprocess.Popen([str(BACKEND), "--desktop-core"], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        call(child, {"action": "set_source", "source": source})
        # Take the *last* equation. Using the first `$` would activate the one
        # inside a definition and quietly report the definition body instead.
        equations = call(child, {"action": "state"})["result"]["equations"]
        if not equations:
            return {"error": "文档里没有公式"}
        reply = call(child, {"action": "activate_formula", "start": equations[-1]["start"]})
        if "error" in reply:
            return reply
        for action, arguments in actions:
            call(child, {"action": action, **arguments})
        return call(child, {"action": "state"})
    finally:
        child.stdin.close()
        child.wait(timeout=30)


def walk(view, depth, lines):
    role = ("@" + view["role"]) if view.get("role") else ""
    lines.append("  " * depth + view["kind"] + role)
    for child in view.get("children") or []:
        walk(child, depth + 1, lines)


def node_fields(view):
    """The node's own fields, without recursing into children."""
    present = {}
    for field in WIRE:
        value = view.get(field)
        if value not in (None, "", 0, False):
            present[field] = value
    children = view.get("children") or []
    present["children"] = len(children)
    present["child_roles"] = [child.get("role") for child in children]
    present["child_kinds"] = [child["kind"] for child in children]
    return present


def nodes_of(view, kind, out):
    if view.get("kind") == kind:
        out.append(node_fields(view))
    for child in view.get("children") or []:
        nodes_of(child, kind, out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", metavar="PATH", help="also write the raw dump")
    arguments = parser.parse_args()
    # A Windows console defaults to the local code page, which cannot print the
    # glyphs a `Symbol` node carries (`𝛼`). The tables are worthless mangled.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not BACKEND.is_file():
        raise SystemExit(f"缺少后端：{BACKEND}\n先运行 cargo build --offline --locked --release "
                         f"--bin visual-typst --target-dir target/server")
    dump = {}
    for label, source, actions in CASES:
        reply = probe(source, actions)
        if "result" not in reply:
            print(f"== {label} | {source!r}\n   {reply}")
            continue
        response = reply["result"]
        tree = []
        walk(response["view"], 0, tree)
        found = {}
        for kind in VIEW_KINDS:
            nodes = []
            nodes_of(response["view"], kind, nodes)
            if nodes:
                found[kind] = nodes
        dump[label] = {"source": source, "actions": [a for a, _ in actions],
                       "document": response.get("source"), "view": tree, "nodes": found}
        print(f"== {label} | {source!r}")
        for line in tree:
            print("   " + line)
        for kind, nodes in found.items():
            for node in nodes:
                fields = " ".join(f"{k}={v!r}" for k, v in node.items() if k not in
                                  ("kind", "children", "child_roles", "child_kinds"))
                print(f"   [{kind}] children={node['children']} roles={node['child_roles']} {fields}")
        if label == "TemplateCall/Parameter":
            text = json.dumps(response["view"], ensure_ascii=False)
            for name in ("parameter", "template-call"):
                marker = '"kind": "' + name + '"'
                print(f"   {name} on the wire: {marker in text}")
        print()
    if arguments.json:
        Path(arguments.json).write_text(json.dumps(dump, ensure_ascii=False, indent=1), encoding="utf-8")
        print(f"raw dump -> {arguments.json}", file=sys.stderr)


if __name__ == "__main__":
    main()
