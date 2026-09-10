"""Adapt Typst's glyph-only symbols to Qt 5's SVG Tiny renderer.

An overflow-visible glyph symbol without a viewport has the same geometry
as a referenced group. Original SVG stays intact for export; the editor's bitmap
cache asks for the monochrome form instead, because a fragment is an image taken
out of the document and the document may colour it.
"""
from functools import lru_cache
import xml.etree.ElementTree as ET

SVG="http://www.w3.org/2000/svg"
XLINK="http://www.w3.org/1999/xlink"
ET.register_namespace("xlink",XLINK)

@lru_cache(maxsize=128)
def qt_svg(source,monochrome=False):
    root=ET.fromstring(source)
    for element in root.iter():
        if element.tag==f"{{{SVG}}}symbol" and element.get("overflow")=="visible" and not any(key in element.attrib for key in ("viewBox","width","height")):
            element.tag=f"{{{SVG}}}g"
        if isinstance(element.tag,str) and element.tag.startswith("{"+SVG+"}"):
            element.tag=element.tag.split("}",1)[1]
        href=element.get("href")
        if href and f"{{{XLINK}}}href" not in element.attrib:element.set(f"{{{XLINK}}}href",href)
        # Math in a light editor is read, not admired: a theme that fills its text
        # white (slides do) or a coloured `#text(...)` around a formula would
        # otherwise hand the editor an image it cannot show. `none` is kept, since
        # dropping it would fill shapes that are meant to be stroked only.
        if monochrome:
            for key in ("fill","stroke"):
                if element.get(key) not in (None,"none"):element.set(key,"#000000")
    root.set("xmlns",SVG)
    definitions=[element for element in root if element.tag=="defs"]
    for element in definitions:root.remove(element)
    for index,element in enumerate(definitions):root.insert(index,element)
    return ET.tostring(root,encoding="utf-8")
