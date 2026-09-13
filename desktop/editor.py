"""Native text editor with lossless inline formula objects."""
import re
from PyQt5.QtCore import Qt, QMimeData, QTimer, QSize, QPointF
from PyQt5.QtGui import QTextCursor, QTextCharFormat, QFont, QColor, QKeySequence, QTextFormat, QPainter
from PyQt5.QtWidgets import QTextEdit, QPlainTextEdit, QMenu, QApplication, QWidget, QToolTip, QVBoxLayout, QLabel, QFrame
from .model import Projection, difference, difference_at_caret, u16, from_u16, from_byte
from .mathview import FormulaObject, OBJECT, OBJECT_ID

def completion_key(editor,event):
    completer=getattr(editor,'completer',None)
    if not completer or not completer.popup().isVisible():return False
    if event.key()==Qt.Key_Escape:completer.popup().hide();event.accept();return True
    if event.key() in (Qt.Key_Return,Qt.Key_Enter,Qt.Key_Tab):
        index=completer.popup().currentIndex()
        if not index.isValid():index=completer.completionModel().index(0,0)
        label=index.data(Qt.DisplayRole)
        completer.popup().hide()
        if label:completer.activated[str].emit(label)
        event.accept();return True
    return False

def formula_error_format(reason):
    """How a formula that cannot be compiled is marked: a wave, and the reason on hover.

    The colour and the tooltip live together because they are one statement -- a red wave
    says "this is broken", and the tooltip is the only place the person can read *why*.
    """
    fmt=QTextCharFormat();fmt.setUnderlineStyle(QTextCharFormat.WaveUnderline)
    fmt.setUnderlineColor(QColor('#b55245'));fmt.setToolTip(reason or '公式保留源码模式')
    return fmt

def decorate_failed_formulas(view,analysis,to_position):
    """Mark every failed formula of `analysis` in `view`, mapping through `to_position`.

    Both the editor and the source dock show the document, in different coordinates, and
    both must say why a formula was left as source: a formula the core declines to expand
    (`editable` false) and a fragment the layout service rejected (`error`) are equally
    unreadable otherwise. `to_position` is the view's own source-offset to character
    position conversion.
    """
    document=view.document();cursor=QTextCursor(document)
    for style in analysis.get("styles",[]):
        if style.get("kind")!="formula_error":continue
        start,end=to_position(style["start"]),to_position(style["end"])
        # A formula is one object character in the editor's view, so a style covering it maps
        # both ends onto that character's position; a zero-length selection would write the
        # format -- and the tooltip -- nowhere, which is how the reason went missing.
        if end<=start:end=min(start+1,document.characterCount()-1)
        if end<=start:continue
        cursor.setPosition(start);cursor.setPosition(end,QTextCursor.KeepAnchor)
        cursor.mergeCharFormat(formula_error_format(style.get("text")))

class MessageSection(QWidget):
    """One engine's messages, with the heading that says which engine they came from."""
    def __init__(self,title,parent=None):
        super().__init__(parent)
        layout=QVBoxLayout(self);layout.setContentsMargins(0,0,0,0);layout.setSpacing(1)
        heading=QLabel(title);heading.setStyleSheet("color:#5a6672;font-weight:bold;")
        self.body=QPlainTextEdit();self.body.setReadOnly(True);self.body.setFrameShape(QFrame.NoFrame)
        self.body.setLineWrapMode(QPlainTextEdit.WidgetWidth)
        self.body.setMaximumHeight(78)
        self.body.setStyleSheet("background:#fbfbfc;color:#26384a;")
        layout.addWidget(heading);layout.addWidget(self.body)
    def set_messages(self,messages):
        self.body.setPlainText("\n".join(messages))
        self.setVisible(bool(messages))

class MessagePanel(QWidget):
    """What the two engines said about this document, each in its own section.

    A formula can fail in two unrelated ways and they are **not** the same statement.
    The language service (Tinymist) reports what is wrong with the *source* an editor
    holds -- a name that does not resolve, a syntax error -- and it reports it where it
    sits in the document. The layout service reports what the **compiler refused** when
    it had to compile a fragment to draw it, which is a statement about the spliced
    source, made because there is no picture to show. Merging them would give one list
    that cannot say which engine to believe, so each keeps its own heading, its own
    wording, and its own section.
    """
    def __init__(self,parent=None):
        super().__init__(parent)
        layout=QVBoxLayout(self);layout.setContentsMargins(6,4,6,5);layout.setSpacing(5)
        self.language=MessageSection("语言服务 · Tinymist")
        self.render=MessageSection("编译 · Typst")
        layout.addWidget(self.language);layout.addWidget(self.render)
        self.setVisible(False)
    def set_messages(self,language=(),render=()):
        language=list(language);render=list(render)
        self.language.set_messages(language)
        self.render.set_messages(render)
        self.setVisible(bool(language or render))

class SourceEditor(QTextEdit):
    """The raw source beside the editor.

    A QTextEdit rather than a QPlainTextEdit: the document layout of a
    QPlainTextEdit ignores block line heights, and the dock copies the editor's
    per-line heights so that source line N sits at the same place in both panes.
    """
    def __init__(self,parent=None):
        super().__init__(parent)
        self.setAcceptRichText(False);self.setUndoRedoEnabled(False)
        self.setReadOnly(False)

    def keyPressEvent(self,event):
        if not completion_key(self,event):super().keyPressEvent(event)

    def contextMenuEvent(self,event):
        menu=self.createStandardContextMenu();owner=self.window()
        if hasattr(owner,'language_help'):
            position=from_u16(owner.source,self.cursorForPosition(event.pos()).position())
            menu.addSeparator();menu.addAction('跳转定义',lambda:owner.language_help.goto(self,position))
        menu.exec_(event.globalPos());menu.deleteLater()

class LineNumberArea(QWidget):
    def __init__(self,editor):super().__init__(editor);self.editor=editor
    def sizeHint(self):return QSize(self.editor.line_number_width(),0)
    def paintEvent(self,event):self.editor.paint_line_numbers(event)

class Editor(QTextEdit):
    def __init__(self, owner):
        super().__init__();self.owner=owner;self.loading=False
        self.mapping=Projection("");self.object_data={};self.object_by_id={};self.expanded=set();self.source_only=False
        # The message of the box the pointer is on, kept in step with the pointer so that
        # the widget's own tooltip is current when Qt's hover delay expires.
        self.tooltip_message=""
        self.setAcceptRichText(False);self.setUndoRedoEnabled(False)
        self.viewport().setMouseTracking(True)
        self.line_numbers=LineNumberArea(self)
        self.handler=FormulaObject(self)
        self.document().documentLayout().registerHandler(OBJECT,self.handler)
        self.textChanged.connect(self.changed)
        self.textChanged.connect(self.update_line_number_width)
        self.textChanged.connect(self.line_numbers.update)
        self.cursorPositionChanged.connect(self.moved)
        self.cursorPositionChanged.connect(self.line_numbers.update)
        self.verticalScrollBar().valueChanged.connect(lambda _:self.line_numbers.update())
        self.document().documentLayout().documentSizeChanged.connect(lambda _:self.line_numbers.update())
        self.setStyleSheet("QTextEdit {border:0; padding:16px; background:#fff; selection-background-color:#a8cdf3;}")
        self.update_line_number_width()

    def line_number_width(self):
        lines=max(1,self.owner.source.count('\n')+1)
        return 14+self.fontMetrics().horizontalAdvance('9')*len(str(lines))

    def update_line_number_width(self):
        self.setViewportMargins(self.line_number_width(),0,0,0)
        self.line_numbers.setFixedWidth(self.line_number_width())

    def line_metrics(self):
        layout=self.document().documentLayout();scroll=self.verticalScrollBar().value()
        block=self.document().begin();result=[]
        while block.isValid():
            rect=layout.blockBoundingRect(block);top=rect.top()-scroll;bottom=rect.bottom()-scroll
            if bottom>=0 and top<=self.viewport().height():
                display_index=from_u16(self.mapping.text,block.position())
                source_index=self.mapping.boundaries[min(display_index,len(self.mapping.boundaries)-1)]
                result.append((self.owner.source.count('\n',0,source_index)+1,top,bottom,block==self.textCursor().block()))
            if top>self.viewport().height():break
            block=block.next()
        return result

    def paint_line_numbers(self,event):
        painter=QPainter(self.line_numbers);painter.fillRect(event.rect(),QColor('#f5f6f7'))
        painter.setFont(self.font());painter.setPen(QColor('#87919b'));width=self.line_numbers.width()-7
        for number,top,bottom,current in self.line_metrics():
            if current:
                painter.fillRect(0,int(top),self.line_numbers.width(),max(1,int(bottom-top)),QColor('#e8edf2'))
                painter.setPen(QColor('#33475b'))
            painter.drawText(0,int(top),width,max(1,int(bottom-top)),Qt.AlignRight|Qt.AlignVCenter,str(number))
            if current:painter.setPen(QColor('#87919b'))
        painter.setPen(QColor('#d8dde2'));painter.drawLine(self.line_numbers.width()-1,0,self.line_numbers.width()-1,self.line_numbers.height())

    def resizeEvent(self,event):
        super().resizeEvent(event)
        rect=self.contentsRect();self.line_numbers.setGeometry(rect.left(),rect.top(),self.line_number_width(),rect.height())

    def source_selection(self):
        cursor=self.textCursor()
        return self.mapping.source_position(cursor.anchor()),self.mapping.source_position(cursor.position())

    def project(self,selection=None,schedule_raw=True):
        self.loading=True
        if selection is None:selection=self.source_selection()
        scroll=self.verticalScrollBar().value()
        source=self.owner.source
        self.mapping=Projection(source,() if self.source_only else self.owner.projected_objects(),self.expanded)
        self.index_objects()
        self.clear();font=QFont(self.owner.settings["font_family"]);font.setPointSizeF(self.owner.settings["font_size"])
        self.setFont(font);self.document().setDefaultFont(font)
        cursor=QTextCursor(self.document());cursor.insertText(self.mapping.text)
        self.decorate(source,selection,scroll,schedule_raw)

    def incremental_project(self,selection=None,schedule_raw=True,reparsed=None):
        if selection is None:selection=self.source_selection()
        scroll=self.verticalScrollBar().value();source=self.owner.source
        projected=Projection(source,() if self.source_only else self.owner.projected_objects(),self.expanded)
        current=self.toPlainText()
        a,b,replacement=difference(current,projected.text)
        self.loading=True
        # Qt may ask intrinsicSize synchronously during insertText/setCharFormat.
        # Publish the complete new source ranges before any document mutation.
        self.mapping=projected
        self.index_objects()
        cursor=QTextCursor(self.document())
        cursor.setPosition(u16(current[:a]));cursor.setPosition(u16(current[:b]),QTextCursor.KeepAnchor)
        cursor.insertText(replacement)
        self.decorate_incremental(source,selection,scroll,schedule_raw,reparsed)

    def index_objects(self):
        # Unchanged formulas retain their Views and boxes even if the surrounding
        # paragraph was reparsed. Only discard entries whose Views are no longer live.
        live={id(f['view']) for f in self.mapping.objects.values() if 'view' in f}
        self.handler.boxes={key:value for key,value in self.handler.boxes.items() if key in live}
        self.object_data={u16(self.mapping.text[:index]):formula for index,formula in self.mapping.objects.items()}
        self.object_by_id={formula['_object_id']:formula for formula in self.object_data.values()}

    def install_objects(self,force=False,dirty=None):
        self.index_objects()
        for position,formula in self.object_data.items():
            identifier=formula['_object_id']
            cursor=QTextCursor(self.document());cursor.setPosition(position);cursor.setPosition(position+1,QTextCursor.KeepAnchor)
            current=cursor.charFormat()
            if force or current.objectType()!=OBJECT or current.property(OBJECT_ID)!=identifier:
                fmt=QTextCharFormat();fmt.setObjectType(OBJECT);fmt.setProperty(OBJECT_ID,identifier)
                fmt.setVerticalAlignment(QTextCharFormat.AlignMiddle);cursor.setCharFormat(fmt)
            if dirty is not None and formula['start']<dirty[1] and dirty[0]<formula['end']:
                self.document().markContentsDirty(position,1)

    def apply_alignment(self,only=None):
        """Centre a display formula that has its line to itself, the way Typst sets it.

        A display equation is a block-level element in Typst, so the projection puts
        it on its own line; centring that paragraph matches the compiled page. A
        display formula sharing its line with text stays left aligned: Typst would
        break the line, this projection does not.
        """
        wanted=set()
        for index,formula in self.mapping.objects.items():
            if not formula.get("display"):continue
            block=self.document().findBlock(u16(self.mapping.text[:index]))
            if not block.isValid() or block.text().replace('\ufffc','').strip():continue
            wanted.add(block.blockNumber())
        last=self.document().blockCount()-1
        first,last=(0,last) if only is None else (
            self.document().findBlock(self.mapping.display_position(min(only[0],len(self.owner.source)))).blockNumber(),
            self.document().findBlock(self.mapping.display_position(min(only[1],len(self.owner.source)))).blockNumber())
        for number in range(first,last+1):
            block=self.document().findBlockByNumber(number)
            if not block.isValid():continue
            alignment=Qt.AlignHCenter if number in wanted else Qt.AlignLeft
            if block.blockFormat().alignment()==alignment:continue
            fmt=block.blockFormat();fmt.setAlignment(alignment)
            cursor=QTextCursor(block);cursor.setBlockFormat(fmt)

    def decorate_incremental(self,source,selection,scroll,schedule_raw,reparsed):
        dirty=(reparsed or {'start':0,'end':len(source.encode('utf-8'))})
        a=from_byte(source,min(dirty['start'],len(source.encode('utf-8'))));b=from_byte(source,min(dirty['end'],len(source.encode('utf-8'))))
        da=self.mapping.display_position(a);db=self.mapping.display_position(b)
        cursor=QTextCursor(self.document());cursor.setPosition(da);cursor.setPosition(max(da,db),QTextCursor.KeepAnchor)
        default=QTextCharFormat();default.setFont(self.font());cursor.setCharFormat(default)
        self.base_selections=[];self.install_objects(dirty=(dirty['start'],dirty['end']))
        self.apply_alignment(only=(a,b))
        self.apply_styles(source,only=(a,b))
        cursor=QTextCursor(self.document());cursor.setPosition(self.mapping.display_position(min(selection[0],len(source))))
        cursor.setPosition(self.mapping.display_position(min(selection[1],len(source))),QTextCursor.KeepAnchor)
        self.setTextCursor(cursor);self.verticalScrollBar().setValue(scroll);self.loading=False
        self.update_line_number_width();self.line_numbers.raise_();self.line_numbers.update();self.setExtraSelections(self.base_selections)
        if schedule_raw:self.owner.raw_timer.start()

    def decorate(self,source,selection,scroll,schedule_raw):
        # Reset presentation without replacing QTextBlocks or their layouts.
        cursor=QTextCursor(self.document());cursor.select(QTextCursor.Document)
        default=QTextCharFormat();default.setFont(self.font());cursor.setCharFormat(default)
        self.base_selections=[]
        self.install_objects(force=True)
        self.apply_alignment()
        self.apply_styles(source)
        cursor=QTextCursor(self.document())
        cursor.setPosition(self.mapping.display_position(min(selection[0],len(source))))
        cursor.setPosition(self.mapping.display_position(min(selection[1],len(source))),QTextCursor.KeepAnchor)
        self.setTextCursor(cursor);self.verticalScrollBar().setValue(scroll);self.loading=False
        self.update_line_number_width();self.line_numbers.raise_();self.line_numbers.update()
        self.setExtraSelections(self.base_selections)
        if schedule_raw:self.owner.raw_timer.start()

    def apply_styles(self,source,only=None):
        cursor=QTextCursor(self.document())
        for style in self.owner.analysis.get("styles",[]):
            a,b=(from_byte(source,style[key]) for key in ("start","end"))
            kind=style["kind"]
            if kind=="let":
                block=self.document().findBlock(self.mapping.display_position(a));block_end=self.mapping.display_position(b)
                while block.isValid() and block.position()<block_end:
                    highlight=QTextEdit.ExtraSelection();highlight.cursor=QTextCursor(block)
                    highlight.format.setBackground(QColor("#f4f5f7"));highlight.format.setProperty(QTextFormat.FullWidthSelection,True)
                    self.base_selections.append(highlight);block=block.next()
            if only and (b<=only[0] or a>=only[1]):continue
            start,end=self.mapping.display_position(a),self.mapping.display_position(b)
            # A formula is one object character in this view, so a style that covers it maps
            # both its ends onto that character's position. A zero-length selection would
            # write the format nowhere -- including the tooltip that says why the formula is
            # shown as source -- so the character itself is what gets styled.
            if end<=start:end=min(start+1,self.document().characterCount()-1)
            cursor.setPosition(start);cursor.setPosition(end,QTextCursor.KeepAnchor)
            fmt=QTextCharFormat()
            if kind=="strong":fmt.setFontWeight(QFont.Bold)
            elif kind=="emph":fmt.setFontItalic(True)
            elif kind=="comment":fmt.setForeground(QColor("#758579"))
            elif kind=="heading":fmt.setFontWeight(QFont.Bold);fmt.setForeground(QColor("#1b6098"))
            elif kind=="let":fmt.setBackground(QColor("#f0f1f4"))
            elif kind=="formula_error":
                fmt=formula_error_format(style.get("text"))
            cursor.mergeCharFormat(fmt)
        # Literal text color calls are a safe presentation hint; source stays visible.
        for match in re.finditer(r'#text\(\s*(red|blue|green|orange|purple|black)\s*\)\[([^\]]*)\]',source):
            if only and (match.end(2)<=only[0] or match.start(2)>=only[1]):continue
            cursor.setPosition(self.mapping.display_position(match.start(2)))
            cursor.setPosition(self.mapping.display_position(match.end(2)),QTextCursor.KeepAnchor)
            fmt=QTextCharFormat();fmt.setForeground(QColor(match.group(1)));cursor.mergeCharFormat(fmt)

    def changed(self):
        if self.loading:return
        after=self.toPlainText()
        # Qt has already put the caret where the person is, and that is what says which of
        # the equivalent edit positions was meant (see `model.difference_at_caret`).
        a,b,replacement=difference_at_caret(self.mapping.text,after,self.textCursor().position())
        start,end=self.mapping.boundaries[a],self.mapping.boundaries[b]
        self.owner.replace(start,end,replacement,typed=True)

    def moved(self):
        if self.loading or self.owner.loading:return
        self.owner.reposition_math()
        if self.textCursor().hasSelection():return
        _,position=self.source_selection()
        remove=[]
        for start in self.expanded:
            formula=next((f for f in self.owner.analysis.get("formulas",[]) if from_byte(self.owner.source,f["start"])==start),None)
            if not formula or not start<=position<=from_byte(self.owner.source,formula["end"]):remove.append(start)
        if remove:
            self.expanded.difference_update(remove)
            QTimer.singleShot(0,lambda:self.project((position,position)))

    def createMimeDataFromSelection(self):
        data=QMimeData();a,b=sorted(self.source_selection());data.setText(self.owner.source[a:b]);return data

    def insertFromMimeData(self,data):
        a,b=sorted(self.source_selection());self.owner.replace(a,b,data.text(),typed=True)

    def contextMenuEvent(self,event):
        menu=QMenu(self)
        for title,callback in [("撤销",self.owner.undo),("重做",self.owner.redo),("复制",self.copy),("剪切",self.cut),("粘贴",self.paste),("全选",self.selectAll)]:menu.addAction(title,callback)
        menu.addSeparator()
        position=self.mapping.source_position(self.cursorForPosition(event.pos()).position())
        menu.addAction('跳转定义',lambda:self.owner.language_help.goto(self,position))
        menu.exec_(event.globalPos())

    def cut(self):
        self.copy();a,b=sorted(self.source_selection());self.owner.replace(a,b,"")

    def wheelEvent(self,event):
        if event.modifiers()&Qt.ControlModifier:
            self.owner.change_font(1 if event.angleDelta().y()>0 else -1);event.accept()
        else:super().wheelEvent(event)

    def mousePressEvent(self,event):
        if not event.modifiers()&Qt.ShiftModifier:
            cursor=self.cursorForPosition(event.pos());position=cursor.position()
            # Hit test object glyph, not the adjacent source character.
            for at in (position,position-1):
                formula=self.object_data.get(at)
                if formula:
                    c=QTextCursor(self.document());c.setPosition(at)
                    rect=self.cursorRect(c)
                    box=self.handler.box(formula)
                    from PyQt5.QtCore import QSizeF
                    size=QSizeF(min(box.width+8,self.viewport().width()-30),box.height+6)
                    if rect.x()-3<=event.pos().x()<=rect.x()+size.width()+3:
                        self.owner.activate(formula["start"],self,at);event.accept();return
        self.owner.finish_formula(focus=False)
        super().mousePressEvent(event)

    def mouseMoveEvent(self,event):
        """Keep the box under the pointer able to explain itself.

        Qt shows a widget's **own** `toolTip` after its hover delay, beside the cursor, and
        that is the only tooltip path this application has ever shown: `DefinitionDraft`'s
        "Enter 确认退出…" is exactly that, set with `setToolTip`. A `QEvent::ToolTip` handler
        does not reach these viewports -- the body's symbol hover has never popped for that
        reason -- so the text is kept current here and Qt does the rest. Nothing is drawn on
        the box: the message is attached to it, and appears as the ordinary tooltip.
        """
        if self.object_data:
            message=self.box_message(event.pos()) or ""
        else:
            # No formula under the pointer at all: a message left over from another
            # document must not stay on the viewport.
            message=""
        if message!=self.tooltip_message:
            self.tooltip_message=message
            self.viewport().setToolTip(message)
            if not message:QToolTip.hideText()
        super().mouseMoveEvent(event)

    def box_message(self, point):
        """What the box under `point` has to say, or None. `point` is in viewport coordinates.

        Two things can be wrong with what a formula draws, and both are answered here
        because both are asked by the same hover. A fragment whose image never came back is
        drawn as a dashed box holding its own source -- that box *is* the thing to explain,
        so the message is read off the node it was laid out from. A formula the core keeps
        as source is one object character with no parts, so its reason is the whole answer.

        Qt does not ask a character format for a tooltip when the character is a text
        object: `mergeCharFormat` cannot even store one there -- the format reads back with
        an empty tooltip -- because the glyph is drawn by the object handler. That is why
        both answers are resolved from the pointer's position instead.
        """
        position=self.cursorForPosition(point).position()
        for at in (position,position-1):
            formula=self.object_data.get(at)
            if not formula:continue
            # The formula is drawn inside its object character's rectangle, offset by what
            # `drawObject` translates the box by: `Box.raws` is in that box's own
            # coordinates, so the pointer is moved into them before the hit test.
            cursor=QTextCursor(self.document());cursor.setPosition(at)
            rect=self.cursorRect(cursor)
            node=self.handler.fragment_at(formula,QPointF(point)-QPointF(rect.x()+4,rect.y()+3))
            message=node.get('_render_error') if node else None
            if message:return message
            # The object under the pointer may *contain* the failed formula rather than be
            # it: a `#let` body is drawn as one object holding the whole definition, and a
            # formula inside it keeps its source. So the reason belongs to any failure whose
            # range falls inside this object's range.
            start,end=from_byte(self.owner.source,formula['start']),from_byte(self.owner.source,formula['end'])
            return next((style.get('text') for style in self.owner.analysis.get('styles',[])
                         if style.get('kind')=='formula_error'
                         and start<=from_byte(self.owner.source,style['start'])
                         and from_byte(self.owner.source,style['end'])<=end),None)
        return None

    def keyPressEvent(self,event):
        if completion_key(self,event):return
        if event.matches(QKeySequence.Copy):self.copy();return
        if event.matches(QKeySequence.Cut):self.cut();return
        if event.matches(QKeySequence.Paste):self.paste();return
        if event.matches(QKeySequence.Undo):self.owner.undo();return
        if event.matches(QKeySequence.Redo):self.owner.redo();return
        arrows=(Qt.Key_Left,Qt.Key_Right,Qt.Key_Up,Qt.Key_Down)
        if event.key() in arrows and not event.modifiers()&Qt.ShiftModifier and not self.textCursor().hasSelection():
            position=self.textCursor().position();at=position-1 if event.key()==Qt.Key_Left else position
            if event.key() in (Qt.Key_Up,Qt.Key_Down):
                probe=QTextCursor(self.textCursor());probe.movePosition(QTextCursor.Up if event.key()==Qt.Key_Up else QTextCursor.Down)
                candidates=[p for p in self.object_data if min(position,probe.position())<=p<=max(position,probe.position())]
                at=min(candidates,key=lambda p:abs(p-probe.position()),default=-1)
            if at in self.object_data:
                self.owner.activate(self.object_data[at]["start"],self,at,last=event.key() in (Qt.Key_Left,Qt.Key_Up));return
        super().keyPressEvent(event)
