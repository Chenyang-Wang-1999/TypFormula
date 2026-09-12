"""Native text editor with lossless inline formula objects."""
import re
from PyQt5.QtCore import Qt, QMimeData, QTimer, QSize
from PyQt5.QtGui import QTextCursor, QTextCharFormat, QFont, QColor, QKeySequence, QTextFormat, QPainter
from PyQt5.QtWidgets import QTextEdit, QPlainTextEdit, QMenu, QApplication, QWidget
from .model import Projection, difference, u16, from_u16, from_byte
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
        self.setAcceptRichText(False);self.setUndoRedoEnabled(False)
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
        cursor=QTextCursor(self.document())
        cursor.setPosition(u16(current[:a]));cursor.setPosition(u16(current[:b]),QTextCursor.KeepAnchor)
        cursor.insertText(replacement)
        self.mapping=projected
        self.decorate_incremental(source,selection,scroll,schedule_raw,reparsed)

    def install_objects(self,force=False,dirty=None):
        # Unchanged formulas retain their Views and boxes even if the surrounding
        # paragraph was reparsed. Only discard entries whose Views are no longer live.
        live={id(f['view']) for f in self.mapping.objects.values() if 'view' in f}
        self.handler.boxes={key:value for key,value in self.handler.boxes.items() if key in live}
        self.object_data={};self.object_by_id={}
        for index,formula in self.mapping.objects.items():
            position=u16(self.mapping.text[:index]);identifier=formula['_object_id']
            self.object_data[position]=formula;self.object_by_id[identifier]=formula
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
            cursor.setPosition(self.mapping.display_position(a));cursor.setPosition(self.mapping.display_position(b),QTextCursor.KeepAnchor)
            fmt=QTextCharFormat()
            if kind=="strong":fmt.setFontWeight(QFont.Bold)
            elif kind=="emph":fmt.setFontItalic(True)
            elif kind=="comment":fmt.setForeground(QColor("#758579"))
            elif kind=="heading":fmt.setFontWeight(QFont.Bold);fmt.setForeground(QColor("#1b6098"))
            elif kind=="let":fmt.setBackground(QColor("#f0f1f4"))
            elif kind=="formula_error":
                fmt.setUnderlineStyle(QTextCharFormat.WaveUnderline);fmt.setUnderlineColor(QColor('#b55245'));fmt.setToolTip(style.get('text','公式保留源码模式'))
            cursor.mergeCharFormat(fmt)
        # Literal text color calls are a safe presentation hint; source stays visible.
        for match in re.finditer(r'#text\(\s*(red|blue|green|orange|purple|black)\s*\)\[([^\]]*)\]',source):
            if only and (match.end(2)<=only[0] or match.start(2)>=only[1]):continue
            cursor.setPosition(self.mapping.display_position(match.start(2)))
            cursor.setPosition(self.mapping.display_position(match.end(2)),QTextCursor.KeepAnchor)
            fmt=QTextCharFormat();fmt.setForeground(QColor(match.group(1)));cursor.mergeCharFormat(fmt)

    def changed(self):
        if self.loading:return
        after=self.toPlainText();a,b,replacement=difference(self.mapping.text,after)
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
