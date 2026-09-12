"""What each `Kind` actually puts on the wire, taken from the real backend.

Evidence for `docs/kind-inventory.md`: it drives the release backend over the
desktop protocol and prints, per `Kind`, the node the frontend would receive.

    cargo build --offline --locked --release --bin typformula --target-dir target/server
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
BACKEND = ROOT / "target/server/release/typformula.exe"

# A macro chain whose projection doubles per layer, far past `PROJECTION_LIMIT`
# (4096): layer15 expands to 2**15 items, so the kernel keeps the call as the
# `raw_macro` arrangement instead of instantiating it.
DEEP_CHAIN = "#let layer0(x) = $#x$" + "".join(
    f"\n#let layer{i}(x) = $layer{i-1}(#x) + layer{i-1}(#x)$" for i in range(1, 16)
) + "\n$layer15(a)$"

# One source per `Kind`, plus the actions that reach the two states no source can
# express. `input` after the formula is active is how a command draft is typed.
CASES = [
    ("Char", "$x$", []),
    ("Symbol", "$alpha$", []),
    ("Number", "$12.5$", []),
    ("Raw", "$arrow.r$", []),
    ("Unknown", "$x$", [("input", {"text": "\\"})]),
    # A draft with a name in it, which is what draws `draft-text`; the bare backslash
    # above only reaches the placeholder and the caret.
    ("Unknown/typing", "$x$", [("input", {"text": "\\f"})]),
    # An empty cell. Note the trailing comma in `frac(a, )` makes *no* argument, so a
    # table spelled with an explicit gap is what actually reaches this: the padded cell
    # of a short row is an empty cell too.
    ("empty-cell", "$mat(, ; , )$", []),
    ("Text", '$"txt"$', []),
    ("MacroCall", "#let twice(a) = $ #a + 1 $\n$ twice(x) $", []),
    # A call the kernel declines to expand. Note what does *not* reach here: a
    # macro whose parameter sits in a Raw is stored as plain `Raw`, because the
    # parser never builds a `MacroCall` for a name that cannot expand. This one
    # is a real `MacroCall` -- the name and its arguments are known -- whose
    # projection doubles per layer and passes `PROJECTION_LIMIT`.
    ("RawMacro", DEEP_CHAIN, []),
    # The ordinary case, which is most of them: a call whose name the command file does
    # not know. Its arguments are positional, so the node keeps them, and the call's own
    # source is what gets rendered -- the caret's position picks which of the two
    # drawings is used.
    ("RawMacro/unknown name", "$bb(A)$", []),
    # ... unless an argument is not positional, in which case no cell list can spell the
    # call back and it stays `Raw`.
    ("Raw/named argument", "$lr(x, size: #100%)$", []),
    # Binding the argument turns the stored raw_macro template into nested style
    # Views, while retaining the argument's cursor identities.
    ("Style/nested in a macro",
     "#let mathbf(x) = $bold(upright(#x))$\n$ mathbf(a) $", []),
    ("Fraction", "$frac(a, b)$", []),
    ("Sqrt", "$sqrt(x)$", []),
    ("Root", "$root(3, x)$", []),
    ("Scripts", "$x^2$", []),
    ("Fenced", "$(a)$", []),
    ("Table", "$mat(1, 2; 3, 4)$", []),
    # The same `grid` shape from two other names: a table's rows and its delimiters
    # belong to the command, so `vec` is one argument per row inside parentheses while
    # `cases` is the same rows inside a single left brace. Both write back as
    # themselves, not as `mat(…; …)`.
    ("Table/vec", "$vec(1, 2, 3)$", []),
    ("Table/cases", "$cases(1, 2)$", []),
    ("Multiline", "$a &= 1 \\ b &= 2$", []),
    # A font variant. The kernel cannot produce the substituted glyphs (the table lives
    # in a crate it cannot reach), so the node carries the **call's spelling** — which is
    # what the engine is asked for, both for the image and for the glyphs.
    ("Accent", "$hat(x)$", []),
    # A font variant. The kernel cannot produce the substituted glyphs (the table lives
    # in a crate it cannot reach), so the node carries the **call's spelling** — which is
    # what the engine is asked for, both for the image and for the glyphs.
    ("Style", "$bold(A)$", []),
    ("Style/nested", "$bold(upright(a))$", []),
    # A variant whose body has no glyph run: a fraction, an accent, a picture. It is drawn
    # as the call instead (see `has_glyph_run`), which is a path that already works rather
    # than a variant that would have to report "no glyphs" every time it is drawn.
    ("Style/structured body", "$bold(frac(a, b))$", []),
    ("Line", "$overline(x)$", []),
    # The two kinds that exist only inside a macro template: neither is reachable
    # from source, so what this shows is their absence.
    ("TemplateCall/Parameter", "#let inner(x) = $ #x $\n#let outer(a) = $ frac(inner(#a), 2) $\n$ outer(y) $", []),
    # Source text the editor keeps opaque, and the constructs the vocabulary
    # alignment is about.
    ("a/b", "$a/b$", []),
    ("cancel", "$cancel(x)$", []),
    ("vec", "$vec(x)$", []),
    ("primes", "$x'$", []),
]

# Core View fields and host annotation fields the frontend can read.
WIRE = ["kind", "role", "text", "display_glyph", "columns", "attachment", "edit",
        "definitions", "origin", "source_range", "active", "selected",
        "marker", "style_name", "border", "is_mat", "row_lengths", "source_text",
        "render_id", "render_request"]

# Views that belong to a node kind, for the summary. These are the *wire* names
# `view_atom` emits, which is not always the shape name a `Kind` declares
# (`slots::Decl::view`): `sqrt`, a delimiter pair, an accent and a rule all
# travel as `decorated`, and `grid`/`aligned`/`script` travel as
# `table`/`multiline`/`scripts`.
VIEW_KINDS = ["char", "symbol", "number", "raw", "unknown", "text", "macro", "raw_macro",
              "macro-argument", "template-call", "parameter", "fraction", "decorated",
              "root", "scripts", "table", "multiline", "style", "absent"]


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


def emitted_kinds(view, out):
    """Every wire kind this view tree actually uses."""
    out.add(view["kind"])
    for child in view.get("children") or []:
        emitted_kinds(child, out)


# Nodes the frontend can also build for itself: delimiter symbols and missing-child
# placeholders. The backend may emit these too; they need no separate probe case.
FRONTEND_MADE = {"symbol", "absent"}

# There used to be a second exception list here: `parameter` and `template-call`, the two
# nodes of a macro template's internal tree, which `bind_template_inner` replaced before
# the view reached the wire. Both are gone -- from that list, from `ARRANGEMENTS`, and
# from the display tree altogether: a template is stored as a display tree whose holes
# and edges are *variants* of the stored type (`crates/core/src/view.rs`,
# `ViewTemplate`), so there is no node to send and therefore nothing for the frontend to
# draw. The comparison below is now exact: the frontend draws the names the backend
# really emits, allowing also the nodes it can synthesize itself.


def frontend_arrangements():
    """`Typesetter.ARRANGEMENTS`, read out of `mathview.py` without importing Qt."""
    source = (ROOT / "desktop/mathview.py").read_text(encoding="utf-8")
    at = source.index("ARRANGEMENTS = frozenset({")
    body = source[at:source.index("})", at)]
    return {name.strip().strip('"') for name in body[body.index("{") + 1:].split(",") if name.strip()}


def audit_arrangements(emitted):
    """Compare what the frontend can draw against what the backend really emits.

    The direction that matters is the second one: a wire kind with no arrangement is
    caught at runtime (`note_unknown` says so once), but an *arrangement nothing can
    emit* is silent, and it is how nine dead name arms were found in `mathview.py`
    before. That is why this is a check and not a comment.
    """
    declared = frontend_arrangements()
    missing = sorted(emitted - declared)
    extra = sorted(declared - emitted - FRONTEND_MADE)
    return declared, missing, extra


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", metavar="PATH", help="also write the raw dump")
    arguments = parser.parse_args()
    # A Windows console defaults to the local code page, which cannot print the
    # glyphs a `Symbol` node carries (`𝛼`). The tables are worthless mangled.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not BACKEND.is_file():
        raise SystemExit(f"缺少后端：{BACKEND}\n先运行 cargo build --offline --locked --release "
                         f"--bin typformula --target-dir target/server")
    dump = {}
    emitted = set()
    for label, source, actions in CASES:
        reply = probe(source, actions)
        if "result" not in reply:
            print(f"== {label} | {source!r}\n   {reply}")
            continue
        response = reply["result"]
        emitted_kinds(response["view"], emitted)
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
    declared, missing, extra = audit_arrangements(emitted)
    print("== 排布名双向对照 ==")
    print(f"   后端在这 {len(CASES)} 个用例里真正发出的线名：{len(emitted)} 个")
    print(f"   前端 ARRANGEMENTS 声明：{len(declared)} 个"
          f"（其中 {sorted(FRONTEND_MADE)} 也可由前端合成）")
    if missing:
        print(f"   ✗ 前端没有画法的线名：{missing}——会在运行时经 note_unknown 报告")
    if extra:
        print(f"   ✗ 后端发不出来的排布名：{extra}——这类分支永远不会执行")
    if not missing and not extra:
        print("   ✓ 两个方向都对齐：每个线名都有画法，每个画法都有线名")
    print()
    if arguments.json:
        Path(arguments.json).write_text(json.dumps(dump, ensure_ascii=False, indent=1), encoding="utf-8")
        print(f"raw dump -> {arguments.json}", file=sys.stderr)
    return 1 if (missing or extra) else 0


if __name__ == "__main__":
    sys.exit(main())
