"""Qt formula typesetting and hit testing over the Rust display tree."""
from dataclasses import dataclass, field
from collections import OrderedDict
import hashlib,math
from PyQt5.QtCore import Qt, QRectF, QPointF, QSizeF, QObject
from PyQt5.QtGui import QFont, QFontMetricsF, QColor, QPen, QPainter, QTextObjectInterface, QTextFormat, QImage, QPixmap
from PyQt5.QtSvg import QSvgRenderer
from PyQt5.QtWidgets import QWidget, QApplication, QInputDialog
from . import mathfont
from . import rawcache
from .svg import qt_svg

OBJECT = QTextFormat.UserObject + 1
OBJECT_ID = QTextFormat.UserProperty + 1

class BitmapCache:
    """Bounded device-pixel cache; SVG paths are interpreted only once.

    Every fragment is rasterized in black: the cached image is editor text, and
    the document a fragment was cut out of may have coloured it.
    """
    def __init__(self,limit=64*1024*1024):self.limit=limit;self.size=0;self.entries=OrderedDict()
    def clear(self):self.entries.clear();self.size=0
    def __len__(self):return len(self.entries)
    def draw(self,painter,svg,target):
        device=painter.device();dpr=float(device.devicePixelRatioF() if hasattr(device,'devicePixelRatioF') else 1)
        scale=max(1,dpr);scale=min(scale,4096/max(1,target.width()),4096/max(1,target.height()))
        width=max(1,math.ceil(target.width()*scale));height=max(1,math.ceil(target.height()*scale))
        digest=hashlib.blake2b(svg.encode('utf-8'),digest_size=16).digest();key=(digest,width,height,round(scale,3))
        record=self.entries.pop(key,None)
        if record is None:
            image=QImage(width,height,QImage.Format_ARGB32_Premultiplied);image.fill(Qt.transparent)
            renderer=QSvgRenderer(qt_svg(svg,True));paint=QPainter(image);renderer.render(paint,QRectF(0,0,width,height));paint.end()
            pixmap=QPixmap.fromImage(image);cost=width*height*4
            while self.entries and self.size+cost>self.limit:
                _,(_,removed)=self.entries.popitem(last=False);self.size-=removed
            record=(pixmap,cost);self.size+=cost
        self.entries[key]=record
        pixmap,_=record
        painter.drawPixmap(target,pixmap,QRectF(0,0,pixmap.width(),pixmap.height()))

@dataclass
class Box:
    width: float
    height: float
    baseline: float
    operations: list = field(default_factory=list)
    stops: list = field(default_factory=list)
    raws: list = field(default_factory=list)

    def add(self, other, x=0, y=0):
        self.operations += [(kind, px+x, py+y, value) for kind, px, py, value in other.operations]
        self.stops += [(px+x, py+y, height, cursor, active) for px, py, height, cursor, active in other.stops]
        self.raws += [(QRectF(rect).translated(x,y), node) for rect,node in other.raws]

class Typesetter:
    # The arrangements this frontend knows how to draw. `kind` is the name the
    # backend declares for a node (`slots::Decl::view`), and everything in this
    # set has a branch in `layout`. A name outside it is a frontend/backend
    # mismatch, not a node to guess at.
    ARRANGEMENTS = frozenset({
        "char", "symbol", "raw", "text", "unknown", "parameter",
        "draft-text", "draft-placeholder", "draft-caret", "absent", "stop",
        "cell", "empty-cell", "fraction", "sqrt", "root", "script",
        "grid", "aligned", "delim", "decoration",
        "macro", "macro-argument", "macro-collapsed", "template-call",
    })

    def __init__(self, settings, cache=None):
        self.settings = settings
        self.cache = cache if cache is not None else {}
        self.svg = BitmapCache()
        self.placements = {}
        # Structural metrics: baseline and line height, from the editor text font.
        # A math font's own ascent is TeX sized (the bundled New Computer Modern
        # Math reports more than three em, for four-line delimiters), so taking the
        # line box from it would put every formula in a mostly empty box.
        self.lines = {}
        # Glyph metrics, per family and style size: one pair is enough for each.
        self.fonts = {}
        self.requested = None
        self.math_family = ""
        # Bumped whenever the Raw/attachment results that layout reads change.
        self.version = 0
        # Arrangements already reported as unknown, so one mismatch is said once
        # instead of once per formula. `warn` is set by the window.
        self.unknown = set()
        self.warn = None

    def touch(self):
        """Invalidate cached boxes after Raw SVGs or attachment placements move."""
        self.version += 1

    def settings_signature(self):
        return (self.settings.get("font_size"), self.settings.get("svg_scale"), self.family())

    def family(self):
        """The family that draws math glyphs, resolved against what Qt really has.

        Re-resolved when the settings change: an uninstalled name must never reach
        QFont, because the substitution is silent and the fallback for a glyph the
        substitute lacks is a third font again.
        """
        requested = (self.settings.get("math_font") or "", self.settings.get("font_family") or "")
        if requested != self.requested:
            self.requested = requested
            self.math_family = mathfont.resolve(*requested)
            self.fonts.clear()
            self.lines.clear()
        return self.math_family

    def style_size(self, factor):
        """Point size of one math style: script levels step down, never below .55."""
        return self.settings["font_size"] * max(.55, factor)

    def style_em(self, factor, metrics):
        """One em of this style in device pixels, for the ratios Typst reports in em."""
        return self.style_size(factor) * metrics.fontDpi() / 72

    def line(self, factor):
        """(font, metrics, height, ascent, descent) of one math style's line box."""
        size = self.style_size(factor)
        entry = self.lines.get(size)
        if entry is None:
            font = QFont(self.settings.get("font_family") or "")
            font.setPointSizeF(size)
            metrics = QFontMetricsF(font)
            entry = (font, metrics, metrics.height(), metrics.ascent(), metrics.descent())
            self.lines[size] = entry
        return entry

    def font(self, family, factor):
        """(font, metrics) used to draw one run, at this style's size."""
        size = self.settings["font_size"] * max(.55, factor)
        entry = self.fonts.get((family, size))
        if entry is None:
            font = QFont(family)
            font.setPointSizeF(size)
            entry = (font, QFontMetricsF(font))
            self.fonts[(family, size)] = entry
        return entry

    def run(self, text, factor, text_mode=False):
        """(characters, font, advance) for one text run of the display tree."""
        family, glyph = mathfont.glyph(self.family(), text, text_mode)
        font, metrics = self.font(family, factor)
        return glyph, font, max(2, metrics.horizontalAdvance(glyph))

    def source_run(self, text, factor):
        """A Raw shown as source instead of an image keeps the editor's own font.

        Its text is Typst source, not math: mapping letters to the math italic
        range would draw `cases(1 & x > 0)` as a formula.
        """
        font, metrics = self.font(self.settings.get("font_family") or self.family(), factor)
        return text, font, max(2, metrics.horizontalAdvance(text))

    def raw(self, node):
        """The rendered record of a Raw node, or False once a request failed.

        False and None are different answers: False means the fragment was asked
        for and produced no image, None that nothing has been asked yet. A fragment
        inside a macro template is asked for like any other, at the definition's
        own source range, so there is one namespace for every fragment.
        """
        shared = rawcache.raw_key(node)
        if shared in self.cache:
            return self.cache[shared]
        for key in (node.get("_raw_key"), node.get("render_id")):
            if key and key in self.cache:
                return self.cache[key]
        return None

    def note_unknown(self, kind):
        """Report an arrangement this frontend does not implement, once per kind."""
        if kind in self.unknown: return
        self.unknown.add(kind)
        if self.warn: self.warn(f"未知的排布 {kind!r}：已按横排显示，前后端可能不同步")

    @staticmethod
    def slot(children, role, index):
        """The child that fills `role`, falling back to its position.

        The backend declares every cell's role, so a layout strategy finds its
        children by meaning rather than by cell order -- a kind that reuses an
        arrangement is then placed correctly without touching this file. The
        positional fallback keeps an older core driving a newer frontend.
        """
        for child in children:
            if child.get("role") == role: return child
        if index < len(children): return children[index]
        return {"kind": "absent", "children": []}

    def layout(self, node, factor=1.0, text_mode=False):
        font, metrics, em, ascent, descent = self.line(factor)
        kind = node.get("kind", "cell")
        children = node.get("children", [])
        if kind not in self.ARRANGEMENTS: self.note_unknown(kind)
        if kind == "absent":
            return Box(0,0,0)
        if kind == "empty-cell":
            # An empty editable slot is drawn as a hole, not as a glyph: after
            # `\frac` + Enter the numerator and denominator must show where the
            # content goes (LyX draws the same dashed box). The cell's own stop is
            # folded in, so the caret can still sit inside the box.
            width=max(6,em*.45);height=em
            box=Box(width,height,ascent,[("slot",0,0,(width,height))])
            for child in children: box.add(self.layout(child,factor,text_mode),0,0)
            if node.get("selected"): box.operations.insert(0,("selection",0,0,(box.width,box.height)))
            return box
        if kind == "stop":
            return Box(2,em,ascent,stops=[(1,0,em,node["cursor"],node.get("active",False))])
        if kind == "raw":
            item = self.raw(node)
            if isinstance(item, dict):
                # Metrics are ratios relative to the actual Typst environment.
                size = self.settings["font_size"] * metrics.fontDpi() / 72 * self.settings["svg_scale"]
                width = max(1, item["base_font_size_pt"] * size)
                height = max(1, item["base_font_height_pt"] * size)
                base = item.get("base_font_baseline_pt", item["base_font_height_pt"]) * size
                box = Box(width,height,base,[("svg",0,0,(item["svg"],width,height))])
                box.raws.append((QRectF(0,0,width,height),node))
                return box
            if item is False:
                # Asked for and refused. Its source is shown instead, marked with
                # dashes and warm ground, and the core lets a horizontal key enter
                # it so it can be repaired in place.
                glyph, draw_font, width = self.source_run(node.get("text", ""), factor)
                box = Box(width,em,ascent,[("text",0,ascent,(glyph,draw_font,kind))])
                box.operations.insert(0,("failed",0,0,(width,em)))
                box.raws.append((QRectF(0,0,width,em),node))
                return box
        if not children:
            text = node.get("display_glyph") or node.get("text", "")
            if kind == "draft-caret": text = "│"
            if kind in ("raw","draft-text","draft-placeholder","draft-caret"):
                # Raw fragments and command drafts are Typst source, not math: they
                # keep the editor's own font, so a half-typed command reads like the
                # text around the formula instead of like the compiled glyphs.
                glyph, draw_font, width = self.source_run(text, factor)
            else: glyph, draw_font, width = self.run(text, factor, text_mode)
            box = Box(width,em,ascent,[("text",0,ascent,(glyph,draw_font,kind))])
            if kind == "raw": box.raws.append((QRectF(0,0,width,em),node))
        elif kind == "fraction":
            numerator,denominator = [self.layout(self.slot(children,role,index),factor*.9)
                                     for role,index in (("numerator",0),("denominator",1))]
            width = max(numerator.width,denominator.width)+8
            box = Box(width,numerator.height+denominator.height+6,numerator.height+5+denominator.baseline*.45)
            box.add(numerator,(width-numerator.width)/2)
            box.add(denominator,(width-denominator.width)/2,numerator.height+6)
            box.operations.append(("line",2,numerator.height+3,(width-4,0)))
        elif kind in ("sqrt","root"):
            body=self.layout(self.slot(children,"radicand",0),factor)
            index=self.layout(self.slot(children,"index",1),factor*.55) if kind=="root" else Box(0,0,0)
            lead=max(em*.6,index.width+4);top=max(3,index.height-body.height*.4)
            box=Box(lead+body.width+3,body.height+top+3,top+body.baseline)
            box.add(body,lead,top);box.add(index,0,0)
            points=[(lead-em*.6,top+body.height*.6),(lead-em*.45,top+body.height*.5),(lead-em*.22,top+body.height),(lead,top),(box.width,top)]
            for (x,y),(xx,yy) in zip(points,points[1:]):box.operations.append(("line",x,y,(xx-x,yy-y)))
        elif kind == "script":
            base = self.layout(self.slot(children,"base",0),factor)
            up,down = [self.layout(self.slot(children,role,index),factor*.7)
                       for role,index in (("upper",1),("lower",2))]
            placement=node.get("_placement") or {}
            centered_up=placement.get("upper")=="limits";centered_down=placement.get("lower")=="limits"
            if not (centered_up or centered_down):
                # Side scripts use Typst's own shifts for the bundled font, measured
                # from the compiled SVG of `x^2` and `x_1`: a superscript's baseline
                # sits .36 em above the base's baseline and a subscript's .25 em below
                # it. A composed base (fraction, root, delimiter, fragment image) has
                # its real geometry, so the script clears its ascent/descent the way
                # Typst's `max(shift, ascent - drop)` does; a single glyph has only
                # the line box's metrics, which are leading-inflated, so it keeps the
                # flat shift. Before this the subscript was put on its own ascent,
                # which dropped it ~0.6 em instead of .25 em.
                em=self.style_em(factor,metrics)
                atoms=[child for child in children[0].get("children",[]) if child.get("kind")!="stop"]
                leaf=len(atoms)==1 and atoms[0].get("kind") in ("char","symbol","text","empty-cell")
                ascent=0.0 if leaf else base.baseline
                descent=0.0 if leaf else base.height-base.baseline
                rise=max(em*.36,ascent-em*.4)
                drop=max(em*.25,descent+em*.05)
                lift=max(0,rise+up.baseline-base.baseline)
                baseline=lift+base.baseline
                up_y=baseline-rise-up.baseline
                down_y=baseline+drop-down.baseline
                box=Box(base.width+max(up.width,down.width),
                        max(lift+base.height,up_y+up.height,down_y+down.height),baseline)
                box.add(base,0,lift)
                box.add(up,base.width,up_y)
                box.add(down,base.width,down_y)
            else:
                core_width=max(base.width,up.width if centered_up else 0,down.width if centered_down else 0)
                side_width=max(0 if centered_up else up.width,0 if centered_down else down.width)
                lift=up.height+2 if centered_up else max(0,up.height-base.height*.4)
                down_y=lift+base.height+2 if centered_down else lift+base.baseline
                box=Box(core_width+side_width,max(lift+base.height,down_y+down.height),lift+base.baseline)
                box.add(base,(core_width-base.width)/2,lift)
                box.add(up,(core_width-up.width)/2 if centered_up else core_width,0)
                box.add(down,(core_width-down.width)/2 if centered_down else core_width,down_y)
        elif kind in ("grid","aligned"):
            cells=[self.layout(child,factor) for child in children]
            columns=max(1,node.get("columns",1));rows=(len(cells)+columns-1)//columns
            widths=[max((c.width for i,c in enumerate(cells) if i%columns==j),default=0) for j in range(columns)]
            heights=[max(c.height for c in cells[r*columns:(r+1)*columns]) for r in range(rows)]
            box=Box(sum(widths)+12*(columns-1),sum(heights)+4*(rows-1),sum(heights)/2+em*.25)
            for i,cell in enumerate(cells):
                row,col=divmod(i,columns)
                align=(widths[col]-cell.width)/2 if kind=="grid" or columns==1 else widths[col]-cell.width if col%2==0 else 0
                box.add(cell,sum(widths[:col])+12*col+align,sum(heights[:row])+4*row)
            if kind=="grid":
                inner=box;box=Box(inner.width+12,inner.height,inner.baseline);box.add(inner,6,0)
                for x,direction in [(1,1),(box.width-1,-1)]:
                    box.operations.extend([("line",x,0,(0,box.height)),("line",x,0,(direction*4,0)),("line",x,box.height,(direction*4,0))])
        else:
            parts=[self.layout(child,factor,text_mode or kind=="text") for child in children]
            if kind == "delim":
                left,right=(node.get("text", "(\n)").split("\n")+[""])[:2]
                parts=[self.layout({"kind":"symbol","text":left},factor)]+parts+[self.layout({"kind":"symbol","text":right},factor)]
            baseline=max((part.baseline for part in parts),default=ascent)
            height=baseline+max((part.height-part.baseline for part in parts),default=descent)
            box=Box(sum(p.width for p in parts),height,baseline)
            x=0
            for part in parts:
                box.add(part,x,baseline-part.baseline);x+=part.width
            if kind=="decoration":
                name=node.get("text","")
                if name in ("underline","underbrace","underbracket","underparen"):
                    box.operations.append(("line",0,box.height,(box.width,0)));box.height+=3
                else:
                    old=box;box=Box(old.width,old.height+5,old.baseline+5);box.add(old,0,5)
                    if name in ("hat","widehat"):
                        box.operations.extend([("line",0,4,(box.width/2,-4)),("line",box.width/2,0,(box.width/2,4))])
                    elif name in ("dot","ddot","dddot"):
                        dots={"dot":"·","ddot":"··","dddot":"···"}[name]
                        glyph,draw_font,width=self.run(dots,factor)
                        box.operations.append(("text",(box.width-width)/2,ascent*.4,(glyph,draw_font,"symbol")))
                    else:
                        box.operations.append(("line",0,2,(box.width,0)))
                        if name in ("arrow","vec"):
                            box.operations.extend([("line",box.width,2,(-4,-2)),("line",box.width,2,(-4,2))])
        if kind in ('unknown','text'):
            inner=box;box=Box(inner.width+8,inner.height+4,inner.baseline+2)
            mode='string' if kind=='text' or node.get('_string_mode') else 'command'
            box.operations.append(('mode',0,0,(box.width,box.height,mode)))
            box.add(inner,4,2)
        if node.get("selected"):
            box.operations.insert(0,("selection",0,0,(box.width,box.height)))
        return box

    def paint(self, painter, box, x=0,y=0,active=False):
        painter.save();painter.translate(x,y)
        painter.setRenderHint(QPainter.Antialiasing)
        for kind,px,py,value in box.operations:
            if kind == "text":
                text,font,role=value;painter.setFont(font)
                painter.setPen(QColor("#aa6633" if role=="raw" else "#172331"))
                painter.drawText(QPointF(px,py),text)
            elif kind == "line":
                painter.setPen(QPen(QColor("#172331"),1))
                painter.drawLine(QPointF(px,py),QPointF(px+value[0],py+value[1]))
            elif kind == "selection":
                painter.fillRect(QRectF(px,py,*value),QColor("#a8cdf3"))
            elif kind == "failed":
                # A fragment with no image: dashes, warm ground.
                width,height=value
                painter.setBrush(QColor("#fff2eb"))
                painter.setPen(QPen(QColor("#b3654e"),1,Qt.DashLine))
                painter.drawRoundedRect(QRectF(px-.5,py+1,width+1,max(2,height-2)),2,2)
                painter.setBrush(Qt.NoBrush)
            elif kind == "slot":
                # An empty slot is a hole: a dashed box with no fill, so the caret
                # inside it stays readable.
                width,height=value
                painter.setBrush(Qt.NoBrush)
                painter.setPen(QPen(QColor("#a9b4c0"),1,Qt.DashLine))
                painter.drawRect(QRectF(px+.5,py+.5,max(1,width-1),max(1,height-1)))
            elif kind == 'mode':
                width,height,mode=value
                painter.setBrush(QColor('#fff8e9' if mode=='string' else '#edf4ff'))
                painter.setPen(QColor('#d9c6a0' if mode=='string' else '#b9cce5'))
                painter.drawRoundedRect(QRectF(px+.5,py+.5,width-1,height-1),3,3)
                painter.setBrush(Qt.NoBrush)
            elif kind == "svg":
                svg,width,height=value
                self.svg.draw(painter,svg,QRectF(px,py,width,height))
        if active:
            painter.setPen(QPen(QColor("#166ac5"),1.5))
            for px,py,height,_,selected in box.stops:
                if selected:painter.drawLine(QPointF(px,py),QPointF(px,py+height))
        painter.restore()

class FormulaObject(QObject,QTextObjectInterface):
    def __init__(self, editor):
        super().__init__(editor);self.editor=editor;self.boxes={};self.signature=None

    def box(self,formula):
        """Box for one formula view, reused across the layout, paint and click paths.

        Qt asks for the same object from intrinsicSize and drawObject, and a
        click asks again before activating. One entry per live view is all this
        needs, and holding the view keeps its id() unique.
        """
        typesetter=self.editor.owner.typesetter
        signature=(typesetter.version,typesetter.settings_signature())
        if signature!=self.signature:self.boxes.clear();self.signature=signature
        view=formula["view"]
        entry=self.boxes.get(id(view))
        if entry is not None and entry[0] is view:return entry[1]
        box=typesetter.layout(view)
        self.boxes[id(view)]=(view,box)
        return box

    def intrinsicSize(self, document, position, format):
        node=self.editor.object_by_id.get(format.property(OBJECT_ID))
        if not node:return QSizeF(20,20)
        box=self.box(node)
        # QTextDocument uses this height for the whole visual line. Do not cap
        # it: a tall fraction/matrix must enlarge its line instead of clipping.
        return QSizeF(min(box.width+8,max(80,self.editor.viewport().width()-30)),box.height+6)

    def drawObject(self,painter,rect,document,position,format):
        node=self.editor.object_by_id.get(format.property(OBJECT_ID))
        if not node:return
        painter.save();painter.setClipRect(rect)
        painter.fillRect(rect,QColor("#f1f6fb"))
        painter.setPen(QColor("#c8d6e2"));painter.drawRoundedRect(rect.adjusted(.5,.5,-.5,-.5),2,2)
        self.editor.owner.typesetter.paint(painter,self.box(node),rect.x()+4,rect.y()+3)
        painter.restore()

class MathCanvas(QWidget):
    def __init__(self,owner):
        super().__init__();self.owner=owner;self.box=Box(10,20,15);self.dragging=False
        self.setFocusPolicy(Qt.StrongFocus)
        self.setAttribute(Qt.WA_InputMethodEnabled,True)

    def refresh(self,state):
        self.owner.bind_active_raw(state)
        for node in self.owner.view_nodes(state['view']):
            if node.get('kind')=='unknown':node['_string_mode']=state.get('string_mode',False)
        self.owner.prepare_view(state["view"],state.get("formula_definitions",""),state.get("display",False))
        self.state=state;self.box=self.owner.typesetter.layout(state["view"])
        self.resize(int(self.box.width+12),int(self.box.height+12))
        self.update()
        stops=[{"cursor":cursor,"x":x,"y":y+h/2} for x,y,h,cursor,_ in self.box.stops]
        self.owner.core.call("geometry",stops=stops)

    def paintEvent(self,event):
        painter=QPainter(self);painter.fillRect(self.rect(),QColor("#f6faff"))
        self.owner.typesetter.paint(painter,self.box,6,6,True)

    def hit(self,point,shift=False):
        if not self.box.stops:return
        x,y=point.x()-6,point.y()-6
        best=min(self.box.stops,key=lambda s:(s[0]-x)**2+4*(s[1]+s[2]/2-y)**2)
        self.owner.math_action("click",cursor=best[3],shift=shift)

    def mousePressEvent(self,event):
        self.setFocus();self.dragging=True
        self.hit(event.pos(),bool(event.modifiers()&Qt.ShiftModifier))

    def mouseMoveEvent(self,event):
        if self.dragging:self.hit(event.pos(),True)

    def mouseReleaseEvent(self,event):self.dragging=False

    def mouseDoubleClickEvent(self,event):
        for rect,node in self.box.raws:
            if rect.contains(QPointF(event.pos())-QPointF(6,6)) and node.get("edit"):
                text,ok=QInputDialog.getMultiLineText(self,"编辑 Raw 源码","Typst",node["text"])
                if ok:self.owner.math_action("edit_source",cursor=node["edit"],source=text)
                return

    def inputMethodEvent(self,event):
        if event.commitString():self.owner.math_action("input",text=event.commitString())
        event.accept()

    def keyPressEvent(self,event):
        ctrl=bool(event.modifiers()&Qt.ControlModifier);shift=bool(event.modifiers()&Qt.ShiftModifier)
        if ctrl and event.key()==Qt.Key_C:
            QApplication.clipboard().setText(self.state.get("selected_source", ""));return
        if ctrl and event.key()==Qt.Key_V:
            self.owner.math_action("paste",text=QApplication.clipboard().text());return
        if ctrl and event.key()==Qt.Key_X:
            QApplication.clipboard().setText(self.state.get("selected_source", ""))
            self.owner.math_action("key",key="Backspace");return
        if event.key()==Qt.Key_Escape and not self.state.get("pending"):
            self.owner.finish_formula();return
        keys={Qt.Key_Left:"ArrowLeft",Qt.Key_Right:"ArrowRight",Qt.Key_Up:"ArrowUp",Qt.Key_Down:"ArrowDown",Qt.Key_Backspace:"Backspace",Qt.Key_Delete:"Delete",Qt.Key_Tab:"Tab",Qt.Key_Backtab:"Tab",Qt.Key_Return:"Enter",Qt.Key_Enter:"Enter",Qt.Key_Escape:"Escape",Qt.Key_Home:"Home",Qt.Key_End:"End"}
        if event.key() in keys:
            self.owner.math_action("key",key=keys[event.key()],shift=shift,ctrl=ctrl)
        elif event.text() and not ctrl:self.owner.math_action("input",text=event.text())
        else:super().keyPressEvent(event)

    def wheelEvent(self,event):
        if event.modifiers()&Qt.ControlModifier:
            self.owner.change_font(1 if event.angleDelta().y()>0 else -1);event.accept()
        else:event.ignore()
