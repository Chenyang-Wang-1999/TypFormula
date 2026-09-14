"""Check the document protocol schema and its fixtures.

Reads `protocol/schema.json` and every `protocol/fixtures/*.jsonl` and answers three
questions without needing any implementation:

1. is the schema self-consistent (every type expression resolves, every verb and event
   has a shape, the enums and the forbidden list are usable);
2. is every fixture step legal (verbs, fields, required fields, enums, ids, revisions,
   `?` wildcards, `paths`, `$` references, dropped events);
3. does the fixture agree with the capability table it declares (every view kind, role
   and marker asserted somewhere is in the `hello` reply) and with the new protocol
   (no name from the old one leaked in: session actions, `/api/*` routes, frontend-side
   cache keys).

Exit code 0 when everything holds, 1 otherwise. Usage: python tools/check_protocol.py
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCHEMA = ROOT / "protocol" / "schema.json"
FIXTURES = ROOT / "protocol" / "fixtures"
PRIMITIVES = {"int", "number", "string", "bool", "range"}
PATH_PART = re.compile(r"[^.\[\]]+|\[\d+\]")
STEP_KEYS = ("step", "note", "send", "expect", "paths", "recv", "expect_drop", "malformed")

errors: list[str] = []
warnings: list[str] = []


def fail(where: str, message: str) -> None:
    errors.append(f"{where}: {message}")


# --- type expressions -------------------------------------------------------------
def parse_type(expr: str):
    """`int` | `int?` | `cursor|null` | `[range]` | `{image}` -> (base, optional, nullable)."""
    e, optional, nullable = expr.strip(), False, False
    while True:
        if e.endswith("?"):
            optional, e = True, e[:-1]
        elif e.endswith("|null"):
            nullable, e = True, e[:-5]
        else:
            break
    if e.startswith("[") and e.endswith("]"):
        return ("array", parse_type(e[1:-1])), optional, nullable
    if e.startswith("{") and e.endswith("}"):
        return ("map", parse_type(e[1:-1])), optional, nullable
    if e in PRIMITIVES:
        return ("prim", e), optional, nullable
    return ("ref", e), optional, nullable


def type_of(spec_node):
    """A verb's `reply` is either a type expression or an inline object of them."""
    if isinstance(spec_node, str):
        return parse_type(spec_node)
    return ("object", {key: parse_type(value) for key, value in spec_node.items()}), False, False


class Schema:
    def __init__(self, raw: dict):
        self.raw = raw
        # The capability table is the `hello` reply, so it is addressable as a type too.
        self.types = {**raw["types"], "capabilities": raw["capabilities"]}
        self.verbs = raw["verbs"]
        self.events = raw["events"]
        self.enums = raw["enums"]
        self.forbidden = raw["forbidden"]
        self.capabilities = raw["capabilities"]
        self._fields: dict[str, dict] = {}

    def fields_of(self, name: str) -> dict:
        if name in self._fields:
            return self._fields[name]
        node = self.types[name]
        self._fields[name] = {"*alias*": parse_type(node)} if isinstance(node, str) else {
            key: parse_type(value) for key, value in node.items()
        }
        return self._fields[name]

    def resolve(self, node):
        base, optional, nullable = node
        if base[0] == "ref":
            alias = self.fields_of(base[1])
            if "*alias*" in alias:
                inner = self.resolve(alias["*alias*"])
                return inner[0], optional or inner[1], nullable or inner[2]
            return ("object", alias), optional, nullable
        return node

    def children(self, node):
        base = self.resolve(node)[0]
        return base[1] if base[0] == "object" else None

    def path_type(self, node, path: str, where: str):
        current = node
        for part in PATH_PART.findall(path):
            if part.startswith("["):
                base = self.resolve(current)[0]
                if base[0] != "array":
                    fail(where, f"path {path!r}: {part} applied to a non-array")
                    return None
                current = base[1]
                continue
            fields = self.children(current)
            if fields is None:
                fail(where, f"path {path!r}: {part} applied to a non-object")
                return None
            if part not in fields:
                fail(where, f"path {path!r}: no field {part!r} here (have {sorted(fields)})")
                return None
            current = fields[part]
        return current


def check_value(schema: Schema, node, value, where: str, refs_ok: bool = False) -> None:
    """Subset match: only what the fixture writes is checked. `"?"` means any value."""
    if value == "?":
        return
    if refs_ok and isinstance(value, str) and value.startswith("$result"):
        return  # a path into the previous reply; the caller has already checked it
    base, _, nullable = schema.resolve(node)
    kind = base[0]
    if value is None:
        if not nullable:
            fail(where, "null where a value is required")
        return
    if kind == "prim":
        name = base[1]
        if name == "range":
            if not (isinstance(value, list) and len(value) == 2 and all(isinstance(v, int) for v in value)):
                fail(where, f"expected [int, int], got {value!r}")
        elif name == "int":
            if isinstance(value, bool) or not isinstance(value, int):
                fail(where, f"expected int, got {value!r}")
        elif name == "number":
            if isinstance(value, bool) or not isinstance(value, (int, float)):
                fail(where, f"expected number, got {value!r}")
        elif name == "string":
            if not isinstance(value, str):
                fail(where, f"expected string, got {value!r}")
        elif not isinstance(value, bool):
            fail(where, f"expected bool, got {value!r}")
        return
    if kind == "array":
        if not isinstance(value, list):
            fail(where, f"expected an array, got {type(value).__name__}")
            return
        for index, item in enumerate(value):
            check_value(schema, base[1], item, f"{where}[{index}]", refs_ok)
        return
    if kind == "map":
        if not isinstance(value, dict):
            fail(where, f"expected a map, got {type(value).__name__}")
            return
        for key, item in value.items():
            check_value(schema, base[1], item, f"{where}.{key}", refs_ok)
        return
    fields = base[1]
    if not isinstance(value, dict):
        fail(where, f"expected an object, got {type(value).__name__}")
        return
    for key, item in value.items():
        if key not in fields:
            fail(where, f"unknown field {key!r}")
            continue
        check_value(schema, fields[key], item, f"{where}.{key}", refs_ok)


def check_required(schema: Schema, node, value: dict, where: str) -> None:
    for name, field in (schema.children(node) or {}).items():
        optional = field[1] or field[2]
        if not optional and name not in value:
            fail(where, f"missing required field {name!r}")


def refs_in(value, out: list[str]) -> None:
    """A reference is a string starting with `$result` (Typst sources start with `$` too)."""
    if isinstance(value, str) and value.startswith("$result"):
        out.append(value[1:])
    elif isinstance(value, list):
        for item in value:
            refs_in(item, out)
    elif isinstance(value, dict):
        for item in value.values():
            refs_in(item, out)


def ids_in(value, out: set[str]) -> None:
    """Id literals: values under `*_id` keys, and the keys of id maps."""
    if isinstance(value, dict):
        for name, item in value.items():
            if name.endswith("_id") and isinstance(item, str) and item != "?":
                out.add(item)
            if name in ("images", "glyphs", "placements") and isinstance(item, dict):
                out.update(item)
            ids_in(item, out)
    elif isinstance(value, list):
        for item in value:
            ids_in(item, out)


def keys_in(value, out: list[str]) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            out.append(key)
            keys_in(item, out)
    elif isinstance(value, list):
        for item in value:
            keys_in(item, out)


def strings_in(value, out: list[str]) -> None:
    if isinstance(value, str):
        out.append(value)
    elif isinstance(value, list):
        for item in value:
            strings_in(item, out)
    elif isinstance(value, dict):
        for item in value.values():
            strings_in(item, out)


def check_capability(spot: str, path: str, value, capabilities: dict) -> None:
    """Every name a fixture asserts must be declared by the capability table (R5).

    Only `view` nodes carry the drawing vocabulary; `styles[].kind` is a different
    enumeration (let/comment/heading) and is not the frontend's drawing whitelist.
    """
    if not isinstance(value, str) or value == "?" or not capabilities or ".view" not in path:
        return
    field = path.rsplit(".", 1)[-1]
    declared = {"kind": capabilities.get("view_kinds"), "role": capabilities.get("roles")}.get(field)
    if declared and value not in declared:
        fail(spot, f"{path} = {value!r} is not in the capability table")
    if field == "marker":
        allowed = {m for group in (capabilities.get("markers") or {}).values() for m in group}
        if allowed and value not in allowed:
            fail(spot, f"{path} = {value!r} is not in the declared markers")


def check_enums(schema: Schema, verb: str, request: dict, spot: str) -> None:
    enums = schema.enums
    if verb == "edit":
        intent = request.get("intent") or {}
        op = intent.get("op")
        allowed = enums["edit.document_ops"] + enums["edit.formula_ops"]
        if op not in allowed:
            fail(spot, f"intent.op {op!r} is not one of {allowed}")
        if op in ("grow_grid", "shrink_grid") and intent.get("axis") not in enums["edit.axes"]:
            fail(spot, f"{op} needs axis in {enums['edit.axes']}, got {intent.get('axis')!r}")
        if op in enums["edit.formula_ops"] and "session" not in request:
            fail(spot, f"{op} is a formula op and needs a session")
    if verb == "navigate":
        op = request.get("op") or {}
        if op.get("kind") not in enums["nav.kinds"]:
            fail(spot, f"nav kind {op.get('kind')!r} is not one of {enums['nav.kinds']}")
        if op.get("kind") == "move" and op.get("direction") not in enums["nav.directions"]:
            fail(spot, f"move needs direction in {enums['nav.directions']}, got {op.get('direction')!r}")
    if verb == "export" and request.get("kind") not in enums["export.kinds"]:
        fail(spot, f"export kind {request.get('kind')!r} is not one of {enums['export.kinds']}")
    if verb == "lsp" and request.get("method") not in enums["lsp.methods"]:
        fail(spot, f"lsp method {request.get('method')!r} is not one of {enums['lsp.methods']}")
    if verb == "packages" and request.get("action") not in enums["packages.actions"]:
        fail(spot, f"packages action {request.get('action')!r} is not one of {enums['packages.actions']}")


def load_fixtures() -> dict[str, list[dict]]:
    files = {}
    for path in sorted(FIXTURES.glob("*.jsonl")):
        steps = []
        for number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
            if not line.strip():
                continue
            try:
                steps.append(json.loads(line))
            except json.JSONDecodeError as error:
                fail(f"{path.name}:{number}", f"not JSON: {error}")
        files[path.name] = steps
    if not files:
        fail("protocol/fixtures", "no fixture files")
    return files


def check_fixture(schema: Schema, name: str, steps: list[dict], capabilities: dict) -> None:
    revision, last_id, last_verb, last_send = 0, 0, None, None
    current_reply = None
    seen_ids: set[str] = set()
    for index, step in enumerate(steps, 1):
        spot = f"{name}:step {index}"
        if step.get("step") != index:
            fail(spot, f"step number is {step.get('step')!r}, expected {index}")
        for key in step:
            if key not in STEP_KEYS:
                fail(spot, f"unknown fixture key {key!r}")
        if "send" in step:
            request = step["send"]
            verb = request.get("verb")
            if verb not in schema.verbs:
                fail(spot, f"unknown verb {verb!r}")
                continue
            if not isinstance(request.get("id"), int):
                fail(spot, "request has no int id")
            elif request["id"] <= last_id:
                fail(spot, f"request id {request['id']} is not greater than {last_id}")
            else:
                last_id = request["id"]
            spec = schema.verbs[verb]
            request_type = (("object", {"id": parse_type("int"), "verb": parse_type("string"),
                                        **{k: parse_type(v) for k, v in spec["request"].items()}}), False, False)
            check_value(schema, request_type, request, spot, refs_ok=True)
            check_required(schema, request_type, request, spot)
            refs: list[str] = []
            refs_in(request, refs)
            for path in refs:
                if current_reply is None:
                    fail(spot, f"reference ${path} but there is no previous reply")
                else:
                    schema.path_type(current_reply, path, spot)
            if not step.get("malformed"):
                check_enums(schema, verb, request, spot)
            last_send, last_verb = request, verb
            current_reply = (("object", {"id": parse_type("int"), "result": type_of(spec["reply"])}), False, False)
            if verb == "hello" and request.get("protocol") == schema.raw["protocol"]:
                result = (step.get("expect") or {}).get("result")
                if isinstance(result, dict):
                    capabilities.update({k: v for k, v in result.items() if v != "?"})
        if "expect" in step:
            if last_send is None:
                fail(spot, "expect with no preceding send")
                continue
            expect = step["expect"]
            if expect.get("id") != last_send["id"]:
                fail(spot, f"expect.id {expect.get('id')!r} does not match request id {last_send['id']!r}")
            reply_type = current_reply
            if "error" in expect:
                reply_type = (("object", {"id": parse_type("int"), "error": parse_type("error")}), False, False)
                if expect["error"].get("verb") != last_verb:
                    fail(spot, f"error.verb {expect['error'].get('verb')!r} is not the failing verb {last_verb!r}")
            check_value(schema, reply_type, expect, spot)
            result = expect.get("result")
            if isinstance(result, dict) and isinstance(result.get("revision"), int):
                revision = result["revision"]
        if "paths" in step:
            if current_reply is None:
                fail(spot, "paths with no preceding reply")
            else:
                for path, value in step["paths"].items():
                    node = schema.path_type(current_reply, path, spot)
                    if node is not None:
                        check_value(schema, node, value, f"{spot} path {path}")
                    check_capability(spot, path, value, capabilities)
        if "recv" in step:
            event = step["recv"]
            event_name = event.get("event")
            if event_name not in schema.events:
                fail(spot, f"unknown event {event_name!r}")
            else:
                event_type = (("object", {"event": parse_type("string"),
                                          **{k: parse_type(v) for k, v in schema.events[event_name].items()}}), False, False)
                check_value(schema, event_type, event, spot)
                check_required(schema, event_type, event, spot)
                stamp = event.get("revision")
                if not isinstance(stamp, int):
                    fail(spot, "event has no int revision")
                elif step.get("expect_drop"):
                    if stamp >= revision:
                        fail(spot, f"dropped event revision {stamp} is not older than {revision}")
                elif stamp < revision:
                    fail(spot, f"event revision {stamp} is older than the current {revision}, not marked dropped")
                if event_name == "render":
                    for group in ("images", "glyphs", "placements"):
                        for key in (event.get(group) or {}):
                            if key not in seen_ids:
                                fail(spot, f"{group} key {key!r} was never declared by an earlier step")
        if "expect_drop" in step and "recv" not in step:
            fail(spot, "expect_drop without recv")
        ids_in(step, seen_ids)
    if not any("send" in step for step in steps):
        warnings.append(f"{name}: no requests")


def check_forbidden(schema: Schema, files: dict[str, list[dict]]) -> None:
    old_verbs = set(schema.forbidden["verbs"])
    old_fields = set(schema.forbidden["fields"])
    routes = schema.forbidden["routes"]
    for name, steps in files.items():
        for index, step in enumerate(steps, 1):
            spot = f"{name}:step {index}"
            send = step.get("send")
            if isinstance(send, dict) and send.get("verb") in old_verbs:
                fail(spot, f"old verb {send['verb']!r} in a new-protocol fixture")
            error = (step.get("expect") or {}).get("error") or {}
            if error.get("verb") in old_verbs:
                fail(spot, "old verb named by an error")
            # `note` is prose for a human reader and may name the old world on purpose.
            probe = {key: item for key, item in step.items() if key != "note"}
            keys: list[str] = []
            keys_in(probe, keys)
            for key in keys:
                if key in old_fields:
                    fail(spot, f"old field name {key!r} leaked into the fixture")
            values: list[str] = []
            strings_in(probe, values)
            for value in values:
                for route in routes:
                    if route in value:
                        fail(spot, f"old route {route!r} leaked into the fixture")


def check_schema(schema: Schema) -> None:
    def probe(where: str, expr: str) -> None:
        try:
            schema.resolve(parse_type(expr))
            schema.children(parse_type(expr))
        except KeyError as error:
            fail("schema", f"{where}: unknown type {error.args[0]!r}")

    for name, spec in schema.verbs.items():
        for side in ("request", "reply"):
            node = spec[side]
            if isinstance(node, dict):
                for field, expr in node.items():
                    probe(f"{name}.{side}.{field}", expr)
            else:
                probe(f"{name}.{side}", node)
    for name, spec in schema.events.items():
        for field, expr in spec.items():
            try:
                schema.resolve(parse_type(expr))
            except KeyError as error:
                fail("schema", f"event {name}.{field}: unknown type {error.args[0]!r}")
    for field, expr in schema.capabilities.items():
        try:
            schema.resolve(parse_type(expr))
        except KeyError as error:
            fail("schema", f"capabilities.{field}: unknown type {error.args[0]!r}")
    for group, values in schema.enums.items():
        if not values or not all(isinstance(v, str) for v in values):
            fail("schema", f"enum {group} is empty or not all strings")
    for key in ("verbs", "routes", "fields", "why"):
        if key not in schema.forbidden:
            fail("schema", f"forbidden.{key} is missing")


def main() -> int:
    if not SCHEMA.is_file():
        print(f"missing {SCHEMA}", file=sys.stderr)
        return 1
    schema = Schema(json.loads(SCHEMA.read_text(encoding="utf-8-sig")))
    check_schema(schema)
    files = load_fixtures()
    capabilities: dict = {}
    for name, steps in files.items():
        check_fixture(schema, name, steps, capabilities)
    if "view_kinds" not in capabilities:
        fail("protocol/fixtures", "no fixture declares the capability table (01_hello.jsonl)")
    check_forbidden(schema, files)

    for warning in warnings:
        print(f"warning: {warning}")
    if errors:
        for message in errors:
            print(f"error: {message}")
        print(f"\n{len(errors)} error(s) in {len(files)} fixture file(s)")
        return 1
    steps = sum(len(v) for v in files.values())
    print(f"protocol ok: {len(schema.verbs)} verbs, {len(schema.events)} events, "
          f"{len(files)} fixture files, {steps} steps, "
          f"{len(capabilities.get('view_kinds', []))} view kinds declared")
    return 0


if __name__ == "__main__":
    sys.exit(main())
