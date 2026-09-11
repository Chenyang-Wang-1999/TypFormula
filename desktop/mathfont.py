"""Editor math fonts: name the bundled families exactly and pick glyphs per run.

The editor draws every structured atom with Qt text, so the family is resolved
here instead of being handed to QFont as a hopeful string: a name Qt cannot find
is substituted silently (a Chinese system font on the development machine), and
per-character fallback then mixes several designs inside one formula.

Only the *glyphs* come from the math font. Its line metrics are TeX sized -- the
bundled New Computer Modern Math reports an ascent of more than three em, since
it has to contain four-line delimiters -- so callers take the structural baseline
and line height from the editor text font and ask this module for glyphs alone.
"""
from PyQt5.QtGui import QFont, QFontDatabase, QRawFont

from .model import ROOT

# The editor fonts the project bundles. The family Qt reports for a file is not
# always the file name, so install() reads the name back instead of assuming it.
BUNDLED = ("fonts/NewCMMath-Regular.otf", "fonts/NewCM10-Italic.otf")
# Typst's own math font: the family NewCMMath-Regular.otf reports, and the one
# its compiled preview is set in.
DEFAULT_MATH = "NewComputerModern Math"
# Shipped with Windows, so a machine without the bundle still gets real math.
FALLBACKS = ("Cambria Math", "Latin Modern Math", "DejaVu Serif")

_installed = None
_families = None
_math_alphabet = {}


def install():
    """Register the bundled fonts once; Qt ignores an unknown family silently."""
    global _installed
    if _installed is None:
        _installed = []
        database = QFontDatabase()
        for relative in BUNDLED:
            path = ROOT / relative
            if not path.is_file():
                continue
            identifier = database.addApplicationFont(str(path))
            if identifier >= 0:
                _installed.extend(database.applicationFontFamilies(identifier))
    return list(_installed)


def families():
    """Every family Qt can render, the bundled ones first."""
    global _families
    if _families is None:
        bundled = install()
        listed = list(QFontDatabase().families())
        _families = [family for family in listed if family in bundled]
        _families += [family for family in bundled if family not in _families]
        _families += [family for family in listed if family not in _families]
    return list(_families)


def resolve(requested, text_family=""):
    """A font family Qt really has, in preference order.

    A name that is not installed must never reach QFont: it is substituted in
    silence, and per-character fallback then draws one formula in several designs.
    """
    available = families()
    for family in (requested, DEFAULT_MATH, *FALLBACKS, text_family):
        if family and family in available:
            return family
    return available[0] if available else requested


def has_math_alphabet(family):
    """Whether the family carries the Unicode math alphanumerics.

    New Computer Modern Math and Cambria Math do. A text or slab family does not,
    and an italic text font has to be given the plain letter instead.
    """
    if family not in _math_alphabet:
        font = QFont(family)
        font.setStyleStrategy(QFont.NoFontMerging)
        raw = QRawFont.fromFont(font)
        _math_alphabet[family] = bool(raw.isValid() and raw.supportsCharacter(0x1D44E))
    return _math_alphabet[family]


def glyph(family, text, text_mode=False, substituted=False):
    """The characters to draw for one atom, in the family that has them.

    A math variable is an italic letter, and a math font's italic alphabet is the
    Unicode range Typst typesets with, so the editor and the compiled preview
    agree. A text cell is upright, and a family without the range is asked for the
    plain letter.

    `substituted` is a third case, and it is not a style choice: the run came from the
    **engine** (`/api/glyphs`), which has already replaced the codepoints it wants —
    `upright(A)` is a plain `A`, `bold(A)` is `𝐀`. Mapping the plain letters here again
    would undo exactly what was asked for, which is how `upright` came out italic.
    """
    if substituted or len(text) != 1 or text_mode or not has_math_alphabet(family):
        return family, text
    if "a" <= text <= "z":
        # U+1D455 (mathematical italic small h) is **unassigned in Unicode**, so no font
        # can draw it; the engine typesets the italic h as Planck's constant `ℎ` U+210E
        # instead, and this is the one letter where its choice differs from the plain
        # mapping above — measured against the real adapter for all 52 letters, and pinned
        # on both sides (`native-adapter/src/main.rs::the_italic_default_has_one_hole_*`).
        return family, "ℎ" if text == "h" else chr(0x1D44E + ord(text) - ord("a"))
    if "A" <= text <= "Z":
        return family, chr(0x1D434 + ord(text) - ord("A"))
    return family, text
