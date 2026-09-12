"""Lossless source/projection mapping. All Python offsets are Unicode indices."""
import json
import os
from pathlib import Path
from .runtime import resource_root

ROOT = resource_root()
DEFAULTS = json.loads((ROOT / "config/desktop-settings.json").read_text("utf-8"))

def u16(text):
    return len(text.encode("utf-16-le")) // 2

def from_u16(text, offset):
    return len(text.encode("utf-16-le")[:offset * 2].decode("utf-16-le", errors="ignore"))

def from_byte(text, offset):
    return len(text.encode("utf-8")[:offset].decode("utf-8"))

def to_byte(text, offset):
    return len(text[:offset].encode("utf-8"))

class Projection:
    def __init__(self, source, formulas=(), expanded=()):
        self.source = source
        self.objects = {}
        self.boundaries = [0]
        self.text = ""
        position = 0
        for formula in sorted(formulas, key=lambda f: f["start"]):
            a, b = (from_byte(source, formula[key]) for key in ("start", "end"))
            if not formula.get("editable") or a in expanded:
                continue
            for index in range(position, a):
                self.text += source[index]
                self.boundaries.append(index + 1)
            self.objects[len(self.text)] = formula
            self.text += "\ufffc"
            self.boundaries.append(b)
            position = b
        for index in range(position, len(source)):
            self.text += source[index]
            self.boundaries.append(index + 1)

    def source_position(self, qt_position):
        return self.boundaries[min(from_u16(self.text, qt_position), len(self.boundaries) - 1)]

    def display_position(self, source_position):
        import bisect
        index = bisect.bisect_left(self.boundaries, source_position)
        return u16(self.text[:index])

    def copy(self, start, end):
        return self.source[self.source_position(start):self.source_position(end)]

def difference(before, after):
    a = 0
    while a < min(len(before), len(after)) and before[a] == after[a]:
        a += 1
    b, c = len(before), len(after)
    while b > a and c > a and before[b - 1] == after[c - 1]:
        b -= 1
        c -= 1
    return a, b, after[a:c]

def config_path():
    base = Path(os.environ.get("APPDATA", Path.home() / ".config"))
    return Path(os.environ.get("TYPFORMULA_CONFIG", base / "TypFormula" / "settings.json"))

def validate_settings(value):
    if not isinstance(value, dict):
        raise ValueError("设置必须是 JSON 对象")
    result = DEFAULTS | value
    if not isinstance(result["font_family"], str) or not result["font_family"].strip():
        raise ValueError("font_family 不能为空")
    # The formula font is resolved against the installed families, so an
    # uninstalled name silently mixes several designs in one formula.
    if not isinstance(result["math_font"], str) or not result["math_font"].strip():
        raise ValueError("math_font 不能为空")
    if not isinstance(result["font_size"], (int, float)) or not 6 <= result["font_size"] <= 72:
        raise ValueError("font_size 应在 6–72 pt 之间")
    if not isinstance(result["svg_scale"], (int, float)) or not .1 <= result["svg_scale"] <= 8:
        raise ValueError("svg_scale 应在 0.1–8 之间")
    if not isinstance(result["shortcuts"], dict) or any(not isinstance(v, str) for v in result["shortcuts"].values()):
        raise ValueError("shortcuts 必须是命令名称到快捷键字符串的映射")
    return result

def load_settings():
    path = config_path()
    return validate_settings(json.loads(path.read_text("utf-8"))) if path.exists() else DEFAULTS.copy()

def atomic_write(path, data):
    """Replace atomically, without leaving a partially written document."""
    import tempfile
    path = Path(path)
    handle, temporary = tempfile.mkstemp(prefix="." + path.name, dir=path.parent)
    try:
        with os.fdopen(handle, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
