"""Language help for source text in both the projected editor and source dock."""
import html
from pathlib import Path
from PyQt5.QtCore import QObject,QEvent,QUrl
from PyQt5.QtWidgets import QToolTip
from .model import from_u16


def hover_text(contents):
    if isinstance(contents,str):return contents
    if isinstance(contents,list):return '\n\n'.join(filter(None,(hover_text(item) for item in contents)))
    if isinstance(contents,dict):return str(contents.get('value',''))
    return ''


def definition_locations(result):
    if isinstance(result,dict):result=[result]
    return [(item.get('targetUri') or item.get('uri'),item.get('targetSelectionRange') or item.get('range'))
            for item in result or [] if isinstance(item,dict)]


class LanguageHelp(QObject):
    def __init__(self,owner):
        super().__init__(owner);self.owner=owner;self.editors={};self.token=0

    def install(self,editor):
        viewport=editor.viewport();self.editors[viewport]=editor
        viewport.installEventFilter(self);viewport.setMouseTracking(True)
        editor.destroyed.connect(lambda:self.editors.pop(viewport,None))

    def cancel(self):
        self.token+=1;QToolTip.hideText()

    def position(self,editor,cursor):
        if hasattr(editor,'mapping'):return editor.mapping.source_position(cursor.position())
        return from_u16(self.owner.source,cursor.position())

    def eventFilter(self,obj,event):
        editor=self.editors.get(obj)
        if editor is None:return False
        if event.type() in (QEvent.MouseMove,QEvent.Leave,QEvent.MouseButtonPress,QEvent.KeyPress):self.cancel()
        if event.type()==QEvent.ToolTip:
            self.hover(editor,event.pos(),event.globalPos());return True
        return False

    def hover(self,editor,point=None,global_point=None):
        if self.owner.definition_draft is not None:return
        cursor=editor.cursorForPosition(point) if point is not None else editor.textCursor()
        position=self.position(editor,cursor)
        global_point=global_point or editor.viewport().mapToGlobal(editor.cursorRect(cursor).bottomLeft())
        self.token+=1;token=self.token
        messages=[d['message'] for d in self.owner.text_diagnostics if d['start']<=position<max(d['end'],d['start']+1)]
        def show(parts):
            text='\n\n'.join(filter(None,parts))
            if text:QToolTip.showText(global_point,'<div style="white-space:pre-wrap">'+html.escape(text)+'</div>',editor)
        show(messages)
        def arrived(reply):
            if token!=self.token:return
            value=reply.get('result') or {}
            show(messages+[hover_text(value.get('contents'))])
        self.owner.request_language('hover',arrived,position=position)

    def goto(self,editor=None,position=None):
        if self.owner.definition_draft is not None:
            self.owner.report('请先确认或取消宏定义草稿');return
        if editor is None:editor=self.owner.source_view if self.owner.source_view.hasFocus() else self.owner.focused_editor()
        if position is None:position=self.position(editor,editor.textCursor())
        def arrived(reply):
            locations=definition_locations(reply.get('result'))
            if not locations:self.owner.report('没有找到定义');return
            choices=[]
            for uri,bounds in locations:
                url=QUrl(uri or '')
                if not url.isLocalFile() or not bounds:continue
                choices.append((Path(url.toLocalFile()).resolve(),bounds))
            if not choices:self.owner.report('定义位置不是可打开的本地文件');return
            index=0
            if len(choices)>1:
                from PyQt5.QtWidgets import QInputDialog
                labels=[f'{i+1}. {p.name}:{r["start"]["line"]+1}:{r["start"]["character"]+1}' for i,(p,r) in enumerate(choices)]
                label,ok=QInputDialog.getItem(self.owner,'跳转定义','选择定义',labels,0,False)
                if not ok:return
                index=labels.index(label)
            path,bounds=choices[index]
            current=self.owner.path or self.owner.workspace/self.owner.body()['path']
            if path==current.resolve():self.owner.reveal_definition(bounds)
            else:
                try:
                    # A separate window preserves the current document and undo history.
                    window=type(self.owner)(path);window.reveal_definition(bounds);window.show()
                except (OSError,ValueError,RuntimeError) as error:self.owner.report(str(error))
        self.owner.request_language('definition',arrived,position=position)
