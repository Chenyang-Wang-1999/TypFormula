"""Document-local definition blocks and uncommitted inline source drafts."""
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QTextCursor, QTextDocument, QTextCharFormat, QColor, QTextOption
from PyQt5.QtWidgets import QWidget, QVBoxLayout, QPlainTextEdit, QFrame
from .model import from_byte, to_byte, u16


def blocks(source, styles, formulas=()):
    result=[]
    for item in sorted((s for s in styles if s['kind']=='let'),key=lambda s:s['start']):
        if any(f['start']<=item['start'] and item['end']<=f['end'] for f in formulas):continue
        a,b=(from_byte(source,item[k]) for k in ('start','end'))
        if result and a<result[-1]['end']:continue
        if a==0 or source[a-1]!='#':continue
        a-=1
        if result and not source[result[-1]['end']:a].strip():
            result[-1]['end']=b;result[-1]['count']+=1
        else:result.append({'start':a,'end':b,'count':1})
    return [dict(item,start=to_byte(source,item['start']),end=to_byte(source,item['end']),
                 definition_block=True,editable=True,display=False,
                 _object_id=f"definitions:{item['start']}") for item in result]


class DefinitionSource(QPlainTextEdit):
    def keyPressEvent(self,event):
        if event.key() in (Qt.Key_Return,Qt.Key_Enter) and not event.modifiers()&Qt.ShiftModifier:
            self.parent().owner.confirm_definitions();event.accept();return
        if event.key()==Qt.Key_Escape:
            self.parent().owner.cancel_definitions();event.accept();return
        super().keyPressEvent(event)


def source_document(editor, block):
    """Lay out the committed source using the editor's font and token colours."""
    owner=editor.owner
    start,end=(from_byte(owner.source,block[k]) for k in ('start','end'))
    text=owner.source[start:end]
    spans=[]
    for style in owner.analysis.get('styles',[]):
        if style['kind']=='comment':
            a,b=(from_byte(owner.source,style[k]) for k in ('start','end'))
            if a<end and start<b:spans.append((max(a,start)-start,min(b,end)-start,'#758579'))
    spans.extend((max(a,start)-start,min(b,end)-start,color)
                 for a,b,color in owner.semantic_spans+owner.engine_spans if a<end and start<b)
    width=max(72,editor.viewport().width()-50)
    key=(text,editor.font().toString(),width,tuple(spans))
    cached=block.get('_source_document')
    if cached is not None and cached[0]==key:return cached[1]
    document=QTextDocument();document.setDocumentMargin(0)
    document.setDefaultFont(editor.font());document.setPlainText(text)
    option=document.defaultTextOption();option.setWrapMode(QTextOption.WrapAtWordBoundaryOrAnywhere)
    document.setDefaultTextOption(option)
    for a,b,color in spans:
        cursor=QTextCursor(document);cursor.setPosition(u16(text[:a]))
        cursor.setPosition(u16(text[:b]),QTextCursor.KeepAnchor)
        fmt=QTextCharFormat();fmt.setForeground(QColor(color));cursor.mergeCharFormat(fmt)
    document.setTextWidth(min(width,max(1,document.idealWidth())))
    block['_source_document']=(key,document)
    return document


class DefinitionDraft(QWidget):
    def __init__(self, owner, editor, block, last=False):
        super().__init__(editor.viewport())
        self.owner=owner;self.editor=editor;self.block=block
        self.start,self.end=(from_byte(owner.source,block[k]) for k in ('start','end'))
        self.original=owner.source[self.start:self.end]
        self.setStyleSheet('DefinitionDraft {background:#eef3f8; border:1px solid #7895af;}')
        layout=QVBoxLayout(self);layout.setContentsMargins(2,2,2,2)
        self.source=DefinitionSource(self);self.source.setFont(editor.font())
        self.source.setLineWrapMode(QPlainTextEdit.NoWrap);self.source.setFrameShape(QFrame.NoFrame)
        self.source.setToolTip('Enter 确认退出 · Shift+Enter 换行 · Esc 取消')
        self.source.setPlainText(self.original);layout.addWidget(self.source)
        if last:self.source.moveCursor(QTextCursor.End)
        self.source.textChanged.connect(self.resize_content)

    def content_height(self):
        return min(8,max(1,self.source.blockCount()))*self.source.fontMetrics().lineSpacing()+16

    def content_width(self):
        width=max((self.source.fontMetrics().horizontalAdvance(line) for line in self.source.toPlainText().splitlines()),default=0)+32
        return min(max(180,width),max(180,self.editor.viewport().width()-50))

    def resize_content(self):
        position=self.editor.mapping.display_position(self.start)
        self.editor.document().markContentsDirty(position,1)
        self.owner.reposition_definitions()
