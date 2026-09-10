"""Native application window. Full source is the sole undoable document."""
import json
import os
import re
import base64,tempfile
from pathlib import Path
from PyQt5.QtCore import Qt, QTimer, QRect, QRectF, QSizeF, QUrl
from PyQt5.QtGui import QFont, QKeySequence, QTextCursor, QTextCharFormat, QTextBlockFormat, QPainter, QPdfWriter, QPageSize, QPageLayout, QCursor, QDesktopServices
from PyQt5.QtSvg import QSvgWidget, QSvgRenderer
from PyQt5.QtWidgets import (QApplication,QMainWindow,QWidget,QSplitter,QDockWidget,QTreeWidget,QTreeWidgetItem,
    QPlainTextEdit,QScrollArea,QVBoxLayout,QAction,QFileDialog,QMessageBox,QInputDialog,QDialog,QDialogButtonBox,
    QLineEdit,QPushButton,QFormLayout,QComboBox,QListWidget,QTextEdit)
from .model import ROOT,load_settings,validate_settings,config_path,atomic_write,from_byte,to_byte,u16,from_u16,difference
from .bridge import Core,Services
from .editor import Editor,SourceEditor
from .mathview import Typesetter,MathCanvas
from .svg import qt_svg
from .rawcache import RawCache,signature
from .incremental import merge as incremental_merge

def initial_window_geometry(available):
    """Fit and center a top-level window inside one screen's work area."""
    margin_x=max(24,round(available.width()*.05))
    margin_y=max(24,round(available.height()*.05))
    width=min(1400,max(640,available.width()-2*margin_x),available.width())
    height=min(900,max(480,available.height()-2*margin_y),available.height())
    return QRect(
        available.x()+(available.width()-width)//2,
        available.y()+(available.height()-height)//2,
        width,height,
    )

class Page(QSvgWidget):
    def __init__(self,page,owner):
        super().__init__();self.data=page;self.owner=owner;self.load(qt_svg(page["svg"]))
    def mouseDoubleClickEvent(self,event):
        if self.owner.preview_revision!=self.owner.revision:
            self.owner.report("预览已过期，请等待当前文档编译完成后再跳转。");return
        targets=self.data.get("mapping",[])
        if targets:
            x=event.pos().x()*self.data["width"]/self.width();y=event.pos().y()*self.data["height"]/self.height()
            target=min(targets,key=lambda t:(t["x"]-x)**2+(t["y"]-y)**2)
            self.owner.jump_byte(target["start"])
    def wheelEvent(self,event):
        if event.modifiers()&Qt.ControlModifier:self.owner.zoom_preview(.1 if event.angleDelta().y()>0 else -.1)
        else:super().wheelEvent(event)

class Window(QMainWindow):
    windows=[]
    def __init__(self,path=None,screen=None):
        super().__init__();self.loading=True;self.source="";self.saved="";self.path=None
        self.history=[];self.future=[];self.revision=0;self.analysis={};self.math_state=None
        self.semantic_spans=[];self.engine_spans=[]
        self.active_editor=None;self.active_position=0;self.pages=[];self.preview_revision=-1;self.preview_zoom=1.0
        self.settings=load_settings();self.typesetter=Typesetter(self.settings)
        self.raw_cache=RawCache();self.raw_pending=set()
        # Attachment requests per live formula view, so a background cycle does
        # not walk every view node and rebuild every definition prefix.
        self.attachments={}
        self.warmup_status={}
        # Reported fragment status, so only a change costs a core round trip.
        self.raw_signature=None
        # (revision, fragments) of the last failed render request, if any.
        self.raw_error=None
        # Source lines whose height the dock copies from the editor, by block number.
        self.source_line_heights={}
        self.formula_serial=0;self.last_reparsed=None
        self.core=Core(self);self.services=None;self.lsp=None;self.workspace=None
        screen=screen or QApplication.screenAt(QCursor.pos()) or QApplication.primaryScreen()
        if screen:self.setGeometry(initial_window_geometry(screen.availableGeometry()))
        else:self.resize(1200,760)
        self.setWindowTitle("Visual Typst")
        self.splitter=QSplitter();self.setCentralWidget(self.splitter)
        self.editor=Editor(self);self.editors=[self.editor];self.splitter.addWidget(self.editor)
        self.source_view=SourceEditor();self.source_view.setLineWrapMode(QTextEdit.NoWrap)
        self.source_dock=QDockWidget("Typst 源码",self);self.source_dock.setWidget(self.source_view)
        self.addDockWidget(Qt.RightDockWidgetArea,self.source_dock);self.source_dock.hide()
        # Token colours are applied per visible view; open the dock and it is
        # coloured at once instead of paying for a hidden widget on every reply.
        self.source_dock.visibilityChanged.connect(lambda _:(self.apply_highlights(),self.sync_source_lines()))
        self.source_view.textChanged.connect(self.source_changed)
        self.outline=QTreeWidget();self.outline.setHeaderHidden(True)
        dock=QDockWidget("大纲",self);dock.setWidget(self.outline);self.addDockWidget(Qt.LeftDockWidgetArea,dock)
        self.outline.itemClicked.connect(lambda item,_:self.jump_byte(item.data(0,Qt.UserRole)))
        self.preview_container=QWidget();self.preview_layout=QVBoxLayout(self.preview_container);self.preview_layout.setAlignment(Qt.AlignTop)
        self.preview_scroll=QScrollArea();self.preview_scroll.setWidgetResizable(True);self.preview_scroll.setWidget(self.preview_container)
        # Kept off-window only for explicit SVG export; there is no live preview pane.
        self.math_scroll=QScrollArea(self.editor.viewport());self.math_scroll.setWidgetResizable(False)
        self.math_canvas=MathCanvas(self);self.math_scroll.setWidget(self.math_canvas);self.math_scroll.hide()
        self.math_scroll.setStyleSheet("QScrollArea {border:1px solid #458cc9;background:#f6faff;}")
        self.commands={};self.build_actions()
        self.math_popup=QListWidget(self);self.math_popup.setWindowFlags(Qt.ToolTip);self.math_popup.setFocusPolicy(Qt.NoFocus)
        self.math_popup.itemClicked.connect(lambda item:self.math_action('complete',name=item.text()))
        self.raw_timer=QTimer(self);self.raw_timer.setSingleShot(True);self.raw_timer.setInterval(160);self.raw_timer.timeout.connect(self.load_raw)
        self.completion_timer=QTimer(self);self.completion_timer.setSingleShot(True);self.completion_timer.setInterval(300)
        self.completion_timer.timeout.connect(lambda:self.complete(automatic=True))
        self.compile_timer=QTimer(self);self.compile_timer.setSingleShot(True);self.compile_timer.setInterval(550)
        self.compile_timer.timeout.connect(self.background)
        for editor in self.editors:
            editor.verticalScrollBar().valueChanged.connect(self.reposition_math)
            editor.verticalScrollBar().valueChanged.connect(lambda _:self.raw_timer.start())
            editor.verticalScrollBar().valueChanged.connect(lambda _,editor=editor:self.mirror_scroll(editor))
        self.source_view.verticalScrollBar().valueChanged.connect(lambda _:self.mirror_scroll(self.source_view))
        self.loading=False;self.load(path)
        Window.windows.append(self)

    def action(self,menu,identifier,title,callback,key=None,toolbar=None):
        action=QAction(title,self);action.setObjectName(identifier)
        shortcut=self.settings["shortcuts"].get(identifier,key or "")
        action.setShortcut(QKeySequence(shortcut));action.triggered.connect(lambda checked=False:callback())
        menu.addAction(action)
        if toolbar:toolbar.addAction(action)
        self.commands[identifier]=(action,key or "")
        return action

    def build_actions(self):
        files=self.menuBar().addMenu("文件");edit=self.menuBar().addMenu("编辑");view=self.menuBar().addMenu("视图")
        math=self.menuBar().addMenu("数学");tools=self.menuBar().addMenu("工具")
        bar=self.addToolBar("文档");mathbar=self.addToolBar("数学")
        entries=[(files,"newFile","新建",self.new_file,"Ctrl+N",bar),(files,"openFile","打开…",self.open_file,"Ctrl+O",bar),
            (files,"save","保存",self.save,"Ctrl+S",bar),(files,"saveCopy","另存为…",lambda:self.save(True),"Ctrl+Shift+S",None),
            (files,"newWindow","新窗口",self.new_window,"Ctrl+Shift+N",None),(files,"importFile","导入 Typst / 插入资源…",self.import_file,None,None),
            (files,"exportPdf","导出 PDF…",lambda:self.compile_pdf(save_as=True,open_after=False),None,None),(files,"exportSvg","导出 SVG…",lambda:self.export("svg"),None,None),
            (edit,"undo","撤销",self.undo,"Ctrl+Z",bar),(edit,"redo","重做",self.redo,"Ctrl+Y",bar),
            (edit,"copy","复制",lambda:self.focus_edit("copy"),"Ctrl+C",None),(edit,"cut","剪切",lambda:self.focus_edit("cut"),"Ctrl+X",None),
            (edit,"paste","粘贴",lambda:self.focus_edit("paste"),"Ctrl+V",None),(edit,"findReplace","查找 / 替换…",self.find_replace,"Ctrl+H",None),
            (view,"toggleSource","显示 / 隐藏源码栏",lambda:self.source_dock.setVisible(not self.source_dock.isVisible()),None,None),
            (view,"split","分栏",self.split,None,None),
            (view,"increaseEditorFont","放大编辑字号",lambda:self.change_font(1),"Ctrl+=",None),(view,"decreaseEditorFont","缩小编辑字号",lambda:self.change_font(-1),"Ctrl+-",None),
            (math,"insertInline","行内公式",lambda:self.insert_formula(False),"Ctrl+Alt+I",mathbar),
            (math,"insertDisplay","行间公式",lambda:self.insert_formula(True),"Ctrl+Alt+B",mathbar),
            (math,"finishFormula","完成公式",self.finish_formula,"Ctrl+Alt+Return",mathbar),
            (math,"addRow","矩阵增加行",lambda:self.math_action("add_row"),None,mathbar),
            (math,"addColumn","矩阵增加列",lambda:self.math_action("add_column"),None,mathbar),
            (math,"refreshMath","刷新全部 SVG 缓存",self.refresh_svg,None,mathbar),
            (tools,"compilePdf","编译 PDF 并打开",self.compile_pdf,"F5",bar),
            (tools,"completion","自动补全",self.complete,"Ctrl+Space",None),
            (tools,"format","格式化",self.format_source,"Ctrl+Alt+F",None),
            (tools,"settings","设置 / 快捷键…",self.configure,None,None),
            (tools,"packages","浏览 @local / @preview 包…",self.packages,None,None)]
        for entry in entries:self.action(*entry)

    def report(self,message):self.statusBar().showMessage(str(message),12000)

    def guarded(self,callback):
        try:return callback()
        except Exception as error:self.report(error);return None

    def ensure_services(self):
        workspace=self.path.parent if self.path else ROOT/"workspace"
        if workspace==self.workspace:return
        if self.services:self.services.close()
        if self.lsp:self.lsp.close()
        self.workspace=workspace;self.services=Services(workspace,self);self.lsp=Services(workspace,self)

    def body(self):return {"path":self.path.name if self.path else "untitled.typ","source":self.source,"raw":[],"formulas":[],"overlays":{}}

    def load(self,path=None):
        self.finish_formula(focus=False)
        if path:
            path=Path(path).resolve()
            with path.open("r",encoding="utf-8-sig",newline="") as stream:text=stream.read()
            self.newline="\r\n" if "\r\n" in text else "\n";text=text.replace("\r\n","\n")
        else:text="";self.newline="\n"
        self.path=path;self.source=text;self.saved=text;self.history=[];self.future=[];self.semantic_spans=[];self.engine_spans=[]
        self.typesetter.cache.clear();self.typesetter.svg.clear();self.typesetter.placements.clear();self.ensure_services()
        self.core.call("set_source",source=text,reset_warmups=True)
        self.analysis=self.core.call("analyze");self.bind_formula_ids({},self.analysis);self.raw_cache.edits.clear();self.raw_cache.rebind({},self.analysis);self.raw_pending.clear();self.warmup_status.clear();self.revision+=1
        for editor in self.editors:editor.expanded.clear()
        self.project((0,0));self.compile_timer.start()

    def project(self,selection=None,schedule_raw=True,incremental=False):
        self.loading=True
        self.attachments.clear()
        for formula in self.analysis.get("formulas",[]):
            if "view" in formula:self.remember_attachments(formula,self.prepare_view(formula["view"],self.source[:from_byte(self.source,formula["start"])],formula["display"]))
        for editor in self.editors:
            if incremental:editor.incremental_project(selection,schedule_raw,self.last_reparsed)
            else:editor.project(selection,schedule_raw)
        cursor=self.source_view.textCursor();pos=cursor.position()
        current=self.source_view.toPlainText();a,b,replacement=difference(current,self.source)
        cursor=QTextCursor(self.source_view.document());cursor.setPosition(u16(current[:a]));cursor.setPosition(u16(current[:b]),QTextCursor.KeepAnchor);cursor.insertText(replacement)
        cursor=self.source_view.textCursor();cursor.setPosition(min(pos,u16(self.source)));self.source_view.setTextCursor(cursor)
        font=QFont(self.settings["font_family"]);font.setPointSizeF(self.settings["font_size"]);self.source_view.setFont(font)
        self.outline.clear()
        for style in self.analysis.get("styles",[]):
            if style["kind"]=="heading":
                item=QTreeWidgetItem([style["text"].strip()]);item.setData(0,Qt.UserRole,style["start"]);self.outline.addTopLevelItem(item)
        self.setWindowTitle(("* " if self.source!=self.saved else "")+(self.path.name if self.path else "未命名.typ")+" — Visual Typst")
        self.loading=False;self.reposition_math();self.apply_highlights()
        self.loading=True;self.sync_source_lines();self.loading=False

    def mirror_scroll(self,source):
        """Keep the dock and the editor on the same lines.

        `sync_source_lines` gives both panes the same line heights, so their scroll
        values are directly comparable: whichever pane is scrolled, the other follows.
        The equality check ends the ping-pong after one step.
        """
        target=self.editor if source is self.source_view else self.source_view
        value=source.verticalScrollBar().value()
        if target.verticalScrollBar().value()!=value:target.verticalScrollBar().setValue(value)

    def sync_source_lines(self):
        """Put the source dock's lines on the editor's lines.

        Both panes show the same text, but the editor replaces each formula with one
        placeholder, so a line carrying a tall formula is taller there. The dock gets
        the editor's document font, the editor's top offset, and the height each such
        line needs, which is what makes source line N land at the same place in both.
        Only the lines that need it are touched, so this stays proportional to the
        formulas on screen rather than to the document.
        """
        dock=self.source_view;editor=self.editor
        font=QFont(self.settings["font_family"]);font.setPointSizeF(self.settings["font_size"])
        if dock.document().defaultFont().family()!=font.family() or dock.document().defaultFont().pointSizeF()!=font.pointSizeF():
            dock.document().setDefaultFont(font)
        top=editor.viewport().mapTo(editor,editor.viewport().rect().topLeft()).y()+editor.document().documentMargin()
        margin=top-dock.viewport().mapTo(dock,dock.viewport().rect().topLeft()).y()
        if dock.document().documentMargin()!=margin:dock.document().setDocumentMargin(max(0,margin))
        heights={}
        for formula in self.analysis.get("formulas",[]):
            if not formula.get("view"):continue
            line=self.source.count('\n',0,from_byte(self.source,formula["start"]))
            heights[line]=max(heights.get(line,0),editor.handler.box(formula).height+6)
        # A text line can differ too: a bold heading falls back to a CJK face with a
        # taller line than the dock's plain text. Copy what the editor measured, so
        # every line below it stays on the same ruler. An unlaid-out document (a
        # hidden window) reports zero heights and keeps the formula estimate.
        layout=editor.document().documentLayout()
        block=editor.document().begin()
        while block.isValid():
            height=layout.blockBoundingRect(block).height()
            if height>0:heights[block.blockNumber()]=max(heights.get(block.blockNumber(),0),int(height+0.5))
            block=block.next()
        for line in set(self.source_line_heights)|set(heights):
            block=dock.document().findBlockByNumber(line)
            if not block.isValid():continue
            wanted=int(round(heights.get(line,0)))
            if self.source_line_heights.get(line)==wanted:continue
            fmt=block.blockFormat();fmt.setLineHeight(wanted,QTextBlockFormat.MinimumHeight)
            cursor=QTextCursor(block);cursor.setBlockFormat(fmt)
        self.source_line_heights=heights

    def focused_editor(self):
        focus=QApplication.focusWidget()
        return focus if focus in self.editors else self.editor

    def checkpoint(self):
        self.history.append((self.source,self.focused_editor().source_selection()));self.future.clear()
        if len(self.history)>500:self.history.pop(0)

    def update_analysis(self,old_source,start,end,replacement,core_current=False):
        byte_start=to_byte(old_source,start);byte_end=to_byte(old_source,end)
        state=None if core_current else self.core.call("edit_source",start=byte_start,end=byte_end,text=replacement)
        reparsed=(self.math_state if core_current else state).get('reparsed_range',{'start':0,'end':len(self.source.encode('utf-8'))})
        self.last_reparsed=reparsed
        syntax=self.core.call("scan")
        candidate,rebuild=incremental_merge(self.analysis,syntax,self.source,byte_start,byte_end,replacement,reparsed)
        for target in rebuild:
            formula=self.core.call("analyze_formula",start=target)
            if formula is None:
                result=self.core.call("analyze");self.bind_formula_ids(self.analysis,result);self.last_reparsed={'start':0,'end':len(self.source.encode('utf-8'))};return result
            index=next((i for i,item in enumerate(candidate['formulas']) if item['start']==target),None)
            if index is None:
                result=self.core.call("analyze");self.bind_formula_ids(self.analysis,result);self.last_reparsed={'start':0,'end':len(self.source.encode('utf-8'))};return result
            if '_object_id' in candidate['formulas'][index]:formula['_object_id']=candidate['formulas'][index]['_object_id']
            candidate['formulas'][index]=formula
        candidate['styles']=[style for style in candidate.get('styles',[]) if style.get('kind')!='formula_error']
        candidate['styles'].extend({'kind':'formula_error','start':formula['start'],'end':formula['end'],'text':formula.get('reason','公式保留源码模式')} for formula in candidate['formulas'] if formula.get('editable') is False)
        self.bind_formula_ids(self.analysis,candidate)
        return candidate

    def bind_formula_ids(self,old,new):
        existing={(formula.get('start'),formula.get('end')):formula.get('_object_id') for formula in old.get('formulas',[]) if formula.get('_object_id')}
        for formula in new.get('formulas',[]):
            if not formula.get('_object_id'):formula['_object_id']=existing.get((formula.get('start'),formula.get('end')))
            if not formula.get('_object_id'):
                self.formula_serial+=1;formula['_object_id']=f'formula:{self.formula_serial}'

    def replace(self,a,b,text,typed=False):
        if self.source[a:b]==text:return
        if not self.finish_formula(focus=False):
            self.project();return
        self.checkpoint()
        old_source=self.source;self.source=self.source[:a]+text+self.source[b:];self.revision+=1
        self.semantic_spans=[];self.engine_spans=[]
        old=self.analysis
        self.analysis=self.update_analysis(old_source,a,b,text);self.raw_cache.rebind(old,self.analysis)
        caret=a+len(text)
        for editor in self.editors:
            editor.expanded.clear()
            if typed:
                for formula in self.analysis["formulas"]:
                    start,end=(from_byte(self.source,formula[k]) for k in ("start","end"))
                    if start<=caret<=end:editor.expanded.add(start)
        self.project((caret,caret),incremental=True);self.compile_timer.start()
        if typed and text and re.search(r'[#.\w]$',self.source[:caret]):self.completion_timer.start()

    def source_changed(self):
        if self.loading:return
        text=self.source_view.toPlainText();position=from_u16(text,self.source_view.textCursor().position())
        self.replace(0,len(self.source),text,True)
        cursor=self.source_view.textCursor();cursor.setPosition(u16(text[:position]));self.source_view.setTextCursor(cursor)
        if re.search(r'[#.\w]$',text[:position]):self.completion_timer.start()

    def restore(self,stack,other):
        if not stack:return
        if not self.finish_formula(focus=False):return
        other.append((self.source,self.focused_editor().source_selection()))
        old_source=self.source;self.source,selection=stack.pop();self.revision+=1
        self.semantic_spans=[];self.engine_spans=[]
        old=self.analysis
        a,b,text=difference(old_source,self.source)
        self.analysis=self.update_analysis(old_source,a,b,text);self.raw_cache.rebind(old,self.analysis)
        for editor in self.editors:editor.expanded.clear()
        self.project(selection,incremental=True);self.compile_timer.start()
    def undo(self):self.restore(self.history,self.future)
    def redo(self):self.restore(self.future,self.history)

    def activate(self,start,editor=None,position=None,last=False):
        if not self.finish_formula(focus=False):return
        editor=editor or self.focused_editor()
        state=self.core.call("activate_formula",start=start);self.math_state=state;self.active_editor=editor
        self.active_position=position if position is not None else editor.mapping.display_position(from_byte(self.source,start))
        self.math_scroll.setParent(editor.viewport());self.math_canvas.refresh(state)
        if last and self.math_canvas.box.stops:self.math_action("click",cursor=self.math_canvas.box.stops[-1][3])
        self.reposition_math();self.math_scroll.show();self.math_scroll.raise_();self.math_canvas.setFocus()
        self.raw_cache.track(state,self.typesetter.cache);self.raw_timer.start()
        # A new core session knows nothing about the last one's render results.
        self.report_raw_fragments(state,force=True)

    def math_action(self,action,**arguments):
        if not self.math_state:return
        before=self.source;previous=self.math_state
        try:state=self.core.call(action,**arguments)
        except Exception as error:self.report(error);return
        self.math_state=state
        if state["source"]!=before:
            self.checkpoint();self.source=state["source"];self.revision+=1
            self.semantic_spans=[];self.engine_spans=[]
            for key,value in list(self.typesetter.cache.items()):
                if value is None:del self.typesetter.cache[key]
            self.typesetter.touch()
            old=self.analysis;a,b,text=difference(before,self.source)
            self.analysis=self.update_analysis(before,a,b,text,core_current=True);self.raw_cache.rebind(old,self.analysis)
            self.project(incremental=True);self.compile_timer.start()
        self.math_canvas.refresh(state);self.reposition_math()
        self.report_raw_fragments(state)
        invalidated=self.raw_cache.track(state,self.typesetter.cache)
        if invalidated:self.invalidate_raw(invalidated);self.raw_timer.start()
        active=next((stop for stop in self.math_canvas.box.stops if stop[4]),None)
        if active:self.math_scroll.ensureVisible(int(active[0]+6),int(active[1]+6),12,20)
        self.report(" · ".join(state.get("candidates",[])[:10]) or state.get("message", ""))
        self.show_math_completions()
        if state.get('pending') and state.get('command')!=previous.get('command'):self.completion_timer.start()
        arrow=arguments.get('key','')
        if action=='key' and arrow in ('ArrowLeft','ArrowRight','ArrowUp','ArrowDown') and not arguments.get('shift') and not arguments.get('ctrl') and not previous.get('pending') and not previous.get('string_mode') and not previous.get('selected_source'):
            if state['cursor']==previous['cursor']:self.exit_formula(arrow)

    def reposition_math(self,*args):
        if not self.math_state or not self.active_editor:return
        editor=self.active_editor;cursor=QTextCursor(editor.document())
        cursor.setPosition(min(self.active_position,editor.document().characterCount()-1));rect=editor.cursorRect(cursor)
        width=min(int(self.math_canvas.box.width+18),max(100,editor.viewport().width()-32))
        height=min(int(self.math_canvas.box.height+18),280)
        self.math_scroll.setGeometry(max(0,min(rect.x(),editor.viewport().width()-width)),max(0,rect.y()),width,height)

    def finish_formula(self,focus=True):
        if not self.math_state:return True
        if self.math_state.get("pending"):
            self.report("请先确认或取消公式命令草稿");return False
        editor=self.active_editor;end=from_byte(self.source,self.math_state["active_range"]["end"])
        invalidated=self.raw_cache.track(self.math_state,self.typesetter.cache,leaving=True)
        if invalidated:self.invalidate_raw(invalidated);self.raw_timer.start()
        self.math_popup.hide()
        self.core.call("deactivate_formula");self.math_state=None;self.active_editor=None;self.math_scroll.hide()
        if focus and editor:editor.project((end,end));editor.setFocus()
        return True

    def exit_formula(self,direction):
        editor=self.active_editor;bounds=self.math_state['active_range']
        position=from_byte(self.source,bounds['start' if direction in ('ArrowLeft','ArrowUp') else 'end'])
        if not self.finish_formula(focus=False):return
        editor.project((position,position));cursor=editor.textCursor()
        if direction in ('ArrowUp','ArrowDown'):cursor.movePosition(QTextCursor.Up if direction=='ArrowUp' else QTextCursor.Down)
        editor.setTextCursor(cursor);editor.setFocus();editor.ensureCursorVisible()

    def insert_formula(self,display):
        if not self.finish_formula():return
        editor=self.focused_editor();a,b=sorted(editor.source_selection());body=self.source[a:b]
        prefix="\n" if display and a>0 and self.source[a-1]!='\n' else ""
        suffix="\n" if display and b<len(self.source) and self.source[b]!='\n' else ""
        text=prefix+("$ "+body+" $" if display else "$"+body+"$")+suffix
        self.replace(a,b,text);self.activate(to_byte(self.source,a+len(prefix)),editor)

    def focus_edit(self,action):
        focus=QApplication.focusWidget()
        if focus==self.math_canvas:
            if action in ("copy","cut"):
                QApplication.clipboard().setText(self.math_state.get("selected_source", ""))
                if action=="cut":self.math_action("key",key="Backspace")
            elif action=="paste":self.math_action("paste",text=QApplication.clipboard().text())
        elif hasattr(focus,action):getattr(focus,action)()

    def dirty_prompt(self):
        if self.source==self.saved:return True
        answer=QMessageBox.question(self,"未保存文档","保存当前文档后继续？",QMessageBox.Save|QMessageBox.Discard|QMessageBox.Cancel,QMessageBox.Save)
        return self.save() if answer==QMessageBox.Save else answer==QMessageBox.Discard
    def new_file(self):
        if self.finish_formula() and self.dirty_prompt():self.load()
    def open_file(self):
        if not self.finish_formula() or not self.dirty_prompt():return
        path,_=QFileDialog.getOpenFileName(self,"打开 Typst 文件",str(self.path or self.workspace),"Typst (*.typ)")
        if path:self.guarded(lambda:self.load(path))
    def new_window(self):
        window=Window(screen=self.screen());window.show()

    def save(self,as_copy=False):
        if not self.finish_formula():return False
        path=self.path
        if as_copy or not path:
            name,_=QFileDialog.getSaveFileName(self,"另存为",str(path or self.workspace/"untitled.typ"),"Typst (*.typ)")
            if not name:return False
            path=Path(name).resolve()
        try:
            if path==self.path and path.exists():
                current=path.read_text("utf-8-sig").replace("\r\n","\n")
                if current!=self.saved:
                    QMessageBox.warning(self,"文件在外部已修改","为保留外部修改，请使用另存为保存当前版本。");return False
            atomic_write(path,self.source.replace("\n",self.newline).encode("utf-8"))
            self.path=path;self.saved=self.source;self.ensure_services();self.project();self.compile_timer.start();return True
        except Exception as error:QMessageBox.warning(self,"保存失败",str(error));return False

    def split(self):
        if len(self.editors)>1:
            self.finish_formula();other=self.editors.pop();other.setParent(None);other.deleteLater()
        else:
            editor=Editor(self);self.editors.append(editor);self.splitter.addWidget(editor);editor.project((0,0))
            editor.verticalScrollBar().valueChanged.connect(self.reposition_math)
            editor.verticalScrollBar().valueChanged.connect(lambda _:self.raw_timer.start())
            editor.verticalScrollBar().valueChanged.connect(lambda _,editor=editor:self.mirror_scroll(editor))

    def change_font(self,delta):
        self.settings["font_size"]=max(6,min(72,self.settings["font_size"]+delta));self.project()
        if self.math_state:self.math_canvas.refresh(self.math_state);self.reposition_math()

    def jump_byte(self,offset):
        self.finish_formula();position=from_byte(self.source,offset);self.editor.project((position,position));self.editor.setFocus();self.editor.ensureCursorVisible()

    def background(self):
        revision=self.revision;body=self.body()
        def warmed(result,error):
            if revision!=self.revision:return
            if error:self.report(error);return
            results=[]
            for record in result.get("results",[]):
                results.append([record["key"],record["failed"]])
                for item in record.get("items",[]):
                    key=(record["key"],(item["start"],item["end"]))
                    self.typesetter.cache.setdefault(key,item)
            self.typesetter.touch()
            if self.math_state and self.math_state.get("pending"):
                self.compile_timer.start();return
            classification_changed=any(self.warmup_status.get(key,False)!=failed for key,failed in results if failed or key in self.warmup_status)
            self.warmup_status.update(results)
            self.report_raw_fragments()
            before=[(f.get('editable'),signature(f.get('view',{}))) for f in self.analysis.get('formulas',[])]
            state=self.core.call("macro_warmup",results=results)
            if classification_changed:
                old=self.analysis;self.analysis=self.core.call("analyze");self.bind_formula_ids(old,self.analysis);self.raw_cache.rebind(old,self.analysis)
                self.last_reparsed={'start':0,'end':len(self.source.encode('utf-8'))}
            after=[(f.get('editable'),signature(f.get('view',{}))) for f in self.analysis.get('formulas',[])]
            changed=before!=after
            if self.math_state and any(not formula["editable"] and formula["start"]==state["active_range"]["start"] for formula in self.analysis["formulas"]):
                self.math_state=state;self.finish_formula()
            elif self.math_state and changed:self.math_state=state;self.math_canvas.refresh(state)
            if changed:self.project(schedule_raw=False,incremental=True)
            for formula in self.analysis.get("formulas",[]):
                for definitions,expression,display in self.attachment_nodes(formula):
                    key=(definitions,expression,display)
                    if key in self.typesetter.placements:continue
                    def attached(value,error,key=key):
                        if revision!=self.revision:return
                        self.typesetter.placements[key]=value if not error else {}
                        self.typesetter.touch()
                        for formula in self.analysis.get('formulas',[]):
                            if 'view' in formula:self.remember_attachments(formula,self.prepare_view(formula['view'],self.source[:from_byte(self.source,formula['start'])],formula['display']))
                        self.repaint_formulas()
                    self.services.request("/api/attachments",{"path":body["path"],"expression":expression,"definitions":definitions,"display":display},attached,key="attachment:"+str(key))
        self.services.request("/api/prewarm",body,warmed,key="prewarm")
        self.semantic_highlight()

    @staticmethod
    def view_nodes(view):
        yield view
        for child in view.get("children",[]):yield from Window.view_nodes(child)

    def prepare_view(self,view,definitions,display):
        attachments=[]
        for node in self.view_nodes(view):
            if node.get("attachment"):
                node["_placement"]=self.typesetter.placements.get((definitions,node["attachment"],display),{})
                attachments.append((definitions,node["attachment"],display))
        return attachments

    def remember_attachments(self,formula,attachments):
        view=formula.get("view",{})
        if not view:return
        # Projection keeps a view object alive exactly while the text before its
        # formula is unchanged, so the definition prefix can be cached with it.
        self.attachments[id(view)]=(view,tuple(attachments))
        if len(self.attachments)>4*max(1,len(self.analysis.get("formulas",[]))):self.attachments.clear();self.attachments[id(view)]=(view,tuple(attachments))

    def attachment_nodes(self,formula):
        """Attachment requests for one formula, without walking its view again."""
        view=formula.get("view",{})
        if not view:return ()
        entry=self.attachments.get(id(view))
        if entry is not None and entry[0] is view:return entry[1]
        definitions=self.source[:from_byte(self.source,formula["start"])]
        attachments=self.prepare_view(view,definitions,formula["display"])
        self.remember_attachments(formula,attachments)
        return self.attachments[id(view)][1]

    def bind_active_raw(self,state):
        bounds=state.get('active_range')
        if not bounds:return
        formula=next((f for f in self.analysis.get('formulas',[]) if f['start']==bounds['start']),None)
        if formula:self.raw_cache.bind(formula.get('view',{}),state['view'])

    def report_raw_fragments(self,state=None,force=False):
        """Tell the core which Raw fragments of the active formula have no image.

        The core opens a fragment's own source on a horizontal key at its boundary,
        and a fragment without an image has nothing to click, so this is the only
        way to repair one. The two lists are reported as sets: a keystroke that
        changes nothing costs nothing, and a fragment still waiting for its render
        is reported as not failed rather than left in the failed set.
        """
        state=state if state is not None else self.math_state
        if not state:return
        failed=[];rest=[]
        for node in self.view_nodes(state.get('view',{})):
            if node.get('kind')!='raw' or not node.get('edit'):continue
            (failed if self.typesetter.raw(node) is False else rest).append(node.get('text',''))
        signature=(state.get('definitions',''),bool(state.get('display')),tuple(sorted(set(failed))),tuple(sorted(set(rest))))
        if not force and signature==self.raw_signature:return
        self.raw_signature=signature
        definitions,display,failed,rest=signature
        for sources,flag in ((failed,True),(rest,False)):
            if sources:self.core.call('preview_results',sources=list(sources),definitions=definitions,display=display,failed=flag)

    def visible_formula_starts(self):
        """Formula starts currently on screen in any editor.

        Grouping the object positions once keeps load_raw linear in the number of
        objects instead of scanning every object for every formula.
        """
        visible=set()
        if self.math_state:visible.add(self.math_state['active_range']['start'])
        for editor in self.editors:
            if editor.source_only:continue
            height=editor.viewport().height()
            for position,item in editor.object_data.items():
                cursor=QTextCursor(editor.document());cursor.setPosition(position)
                rect=editor.cursorRect(cursor)
                if rect.bottom()>=0 and rect.top()<height:visible.add(item['start'])
        return visible

    def load_raw(self):
        if self.math_state and self.math_state.get('pending'):return
        revision=self.revision;targets={};ranges=[];seen=set();stuck=False;context_end=0
        # A failed request leaves every fragment it covered without an image, and a
        # fragment the renderer refused leaves that one without an image. Either
        # verdict belongs to the revision it was made in: the next edit (usually the
        # one that fixes the document) clears it, so one bad compile cannot leave a
        # viewport full of source text for the rest of the session.
        if self.raw_error and self.raw_error[0]!=revision:
            for shared in self.raw_error[1]:
                if self.typesetter.cache.get(shared) is False:del self.typesetter.cache[shared]
            self.raw_error=None;stuck=True
        visible=self.visible_formula_starts()
        for formula in self.analysis.get('formulas',[]):
            if formula['start'] not in visible:continue
            request=formula.get('render')
            if not request:continue
            by_id={item['id']:item for item in request.get('raw',[])}
            wanted=0
            for node in self.view_nodes(formula.get('view',{})):
                if node.get('kind')!='raw':continue
                stable=node.get('_raw_key');shared=('raw',node.get('text',''))
                if not node.get('render_id'):
                    # No range means the service can never be asked for this fragment,
                    # and no warmup instance means no other way to an image. Record it
                    # as failed: its source is shown, and a horizontal key enters it.
                    if not node.get('warmup_key') and shared not in self.typesetter.cache:
                        self.typesetter.cache[shared]=False;stuck=True
                    continue
                if stable in self.typesetter.cache or shared in self.typesetter.cache or shared in self.raw_pending or shared in seen:continue
                source_id=':'.join(node['render_id'].split(':')[:2]);source_range=by_id.get(source_id)
                if not source_range:continue
                seen.add(shared);targets[source_id]=(stable,shared);ranges.append(source_range);wanted+=1
            # Ask for a source that reaches only as far as the last fragment does:
            # the service then cuts the document there, so a mistake further down
            # cannot take the images of this viewport with it.
            if wanted:context_end=max(context_end,formula['end'])
        if stuck:self.typesetter.touch();self.repaint_formulas()
        if not targets:return
        body=self.body()|{'raw':ranges,'formulas':[],'context_end':context_end}
        pending={shared for _,shared in targets.values()};self.raw_pending.update(pending)
        def rendered(result,error,targets=targets,revision=revision):
            pending={shared for _,shared in targets.values()};self.raw_pending.difference_update(pending)
            if revision!=self.revision:self.raw_timer.start();return
            for shared in pending:self.typesetter.cache[shared]=False
            if error:
                self.raw_error=(revision,pending);self.report(error)
            else:
                # The renderer leaves out a fragment whose own source cannot
                # compile instead of failing the batch. That verdict is about the
                # document text, so the next edit clears it and asks again, the
                # same way a failed request does.
                refused={targets[source_id][1] for source_id in (result or {}).get('failed',[]) if source_id in targets}
                self.raw_error=(revision,refused) if refused else None
                for item in result.get('items',[]):
                    source_id=':'.join(item['id'].split(':')[:2])
                    if source_id in targets:self.typesetter.cache[targets[source_id][1]]=item
            self.typesetter.touch();self.repaint_formulas()
        self.services.request('/api/render',body,rendered,key='raw-batch')

    def repaint_formulas(self):
        """Relay out and repaint after Raw images or fragment statuses changed."""
        for editor in self.editors:editor.document().markContentsDirty(0,editor.document().characterCount());editor.viewport().update()
        if self.math_state:self.math_canvas.refresh(self.math_state);self.reposition_math()
        self.report_raw_fragments()

    def invalidate_raw(self,records):
        for _,text in records:
            shared=('raw',text);self.typesetter.cache.pop(shared,None);self.raw_pending.discard(shared)
        if records:self.typesetter.touch()

    def clear_actual_svg(self):
        self.semantic_spans=[];self.engine_spans=[]
        for key in list(self.typesetter.cache):
            if isinstance(key,str):del self.typesetter.cache[key]
        self.typesetter.touch()

    def compile(self,callback=None):
        revision=self.revision
        self.report("正在编译预览…")
        def done(result,error):
            if revision!=self.revision:
                if callback:callback(None,"导出期间文档已变化，请重新导出。")
                return
            if error:
                self.report("编译失败，保留上次预览："+error)
                if callback:callback(None,error)
                return
            old=self.pages;reused=set()
            while self.preview_layout.count():self.preview_layout.takeAt(0)
            self.pages=[]
            self.engine_spans=[]
            for received in result.get("pages",[]):
                index=len(self.pages);data=received
                if index<len(old) and received.get('svg') is None and old[index].data.get('hash')==received.get('hash'):
                    data=old[index].data|{key:value for key,value in received.items() if value is not None}
                if index<len(old) and old[index].data.get('hash')==data.get('hash'):
                    page=old[index];page.data=data;reused.add(page)
                else:page=Page(data,self)
                self.pages.append(page);self.preview_layout.addWidget(page)
                for item in data.get("mapping",[]):
                    if item.get("color") and item.get("end",item["start"])>item["start"]:
                        color=item["color"]
                        color=color if color.startswith('#') else '#'+color
                        if len(color)==9:color='#'+color[7:9]+color[1:7]
                        self.engine_spans.append((from_byte(self.source,item["start"]),from_byte(self.source,item["end"]),color))
            for page in old:
                if page not in reused:page.setParent(None);page.deleteLater()
            self.apply_highlights()
            self.preview_revision=revision;self.zoom_preview(0);self.report(f"预览已更新 · {len(self.pages)} 页")
            if callback:callback(result,None)
        hashes=[page.data.get('hash','') for page in self.pages]
        self.services.request("/api/preview",self.body()|{"preview":True,"preview_hashes":hashes},done,key=None if callback else "preview")

    def zoom_preview(self,delta):
        self.preview_zoom=max(.15,min(4,self.preview_zoom+delta))
        for page in self.pages:page.setFixedSize(int(page.data["width"]*self.preview_zoom),int(page.data["height"]*self.preview_zoom))

    def fit_preview(self):
        if self.pages:self.preview_zoom=max(.15,(self.preview_scroll.viewport().width()-32)/max(page.data["width"] for page in self.pages));self.zoom_preview(0)

    def reveal_preview(self):
        if self.preview_revision!=self.revision:
            self.compile(lambda result,error:self.reveal_preview() if not error else None);return
        position=to_byte(self.source,self.focused_editor().source_selection()[1])
        targets=[(abs(item["start"]-position),page,item) for page in self.pages for item in page.data.get("mapping",[])]
        if not targets:self.report("当前位置没有可定位的预览内容");return
        _,page,item=min(targets,key=lambda entry:entry[0]);self.preview_dock.show()
        y=page.y()+int(item["y"]*self.preview_zoom)
        self.preview_scroll.ensureVisible(int(item["x"]*self.preview_zoom),y,30,60)

    def compile_pdf(self,save_as=False,open_after=True):
        if not self.finish_formula():return
        revision=self.revision
        if save_as:
            suggested=(self.path or self.workspace/'untitled.typ').with_suffix('.pdf')
            name,_=QFileDialog.getSaveFileName(self,"导出 Typst PDF",str(suggested),"PDF (*.pdf)")
            if not name:return
            destination=Path(name).resolve()
        else:
            directory=Path(tempfile.gettempdir())/'VisualTypst';directory.mkdir(parents=True,exist_ok=True)
            stem=re.sub(r'[^\w.-]+','_',self.path.stem if self.path else 'untitled')
            destination=directory/f'{stem}-{id(self):x}.pdf'
        self.report('正在用 Typst 编译 PDF…')
        def done(result,error):
            if revision!=self.revision:self.report('PDF 编译期间文档已变化，请重新编译');return
            if error:QMessageBox.warning(self,'PDF 编译失败',error);return
            try:
                data=base64.b64decode(result['pdf'],validate=True)
                if not data.startswith(b'%PDF'):raise ValueError('后端没有返回有效 PDF')
                atomic_write(destination,data)
                self.report(f"PDF 已生成 · {result.get('pages',0)} 页 · {destination}")
                if open_after and not QDesktopServices.openUrl(QUrl.fromLocalFile(str(destination))):raise OSError('系统没有可用的默认 PDF 阅读器')
            except Exception as failure:QMessageBox.warning(self,'PDF 处理失败',str(failure))
        self.services.request('/api/pdf',self.body()|{'pdf':True},done,key='pdf-open' if not save_as else None)

    def refresh_svg(self):
        if not self.finish_formula():return
        self.typesetter.cache.clear();self.typesetter.svg.clear()
        self.typesetter.placements.clear()
        qt_svg.cache_clear()
        self.core.call("macro_warmup",clear=True,results=[])
        def cleared(result,error):
            if error:self.report(error)
            else:self.background()
        self.services.request("/api/cache/clear",{},cleared)

    def export(self,kind):
        if kind=='pdf':self.compile_pdf(save_as=True,open_after=False);return
        name,_=QFileDialog.getSaveFileName(self,"导出 "+kind.upper(),str((self.path or self.workspace/"untitled.typ").with_suffix("."+kind)),kind.upper()+" (*."+kind+")")
        if not name:return
        destination=Path(name)
        def exported(result,error):
            if error:QMessageBox.warning(self,"导出失败",error);return
            pages=result["pages"]
            try:
                if not pages:raise ValueError("文档没有页面")
                if kind=="svg":
                    # A single SVG container preserves every page and avoids implicit sibling overwrites.
                    import xml.etree.ElementTree as ET
                    width=max(page["width"] for page in pages);height=sum(page["height"] for page in pages)
                    root=ET.Element("svg",{"xmlns":"http://www.w3.org/2000/svg","width":str(width)+"pt","height":str(height)+"pt","viewBox":f"0 0 {width} {height}"})
                    y=0
                    for index,page in enumerate(pages):
                        svg=ET.fromstring(page["svg"])
                        # Isolate each page's generated IDs in the combined document.
                        mapping={element.attrib["id"]:f"page{index}-"+element.attrib["id"] for element in svg.iter() if "id" in element.attrib}
                        for element in svg.iter():
                            for key,value in list(element.attrib.items()):
                                if key=="id":element.attrib[key]=mapping[value]
                                else:
                                    for old,new in mapping.items():value=value.replace(f"url(#{old})",f"url(#{new})")
                                    if value.startswith("#") and value[1:] in mapping:value="#"+mapping[value[1:]]
                                    element.attrib[key]=value
                        svg.set("x","0");svg.set("y",str(y));svg.set("width",str(page["width"]));svg.set("height",str(page["height"]))
                        root.append(svg);y+=page["height"]
                    atomic_write(destination,ET.tostring(root,encoding="utf-8",xml_declaration=True))
                else:
                    import tempfile
                    handle,temporary=tempfile.mkstemp(suffix=".pdf",dir=destination.parent);os.close(handle)
                    try:
                        writer=QPdfWriter(temporary);writer.setResolution(72);writer.setTitle(destination.stem)
                        from PyQt5.QtCore import QMarginsF
                        def size(page):
                            writer.setPageSize(QPageSize(QSizeF(page["width"],page["height"]),QPageSize.Point))
                            writer.setPageMargins(QMarginsF(0,0,0,0),QPageLayout.Point)
                        size(pages[0]);painter=QPainter(writer)
                        if not painter.isActive():raise ValueError("无法创建 PDF")
                        for i,page in enumerate(pages):
                            size(page)
                            if i:writer.newPage()
                            QSvgRenderer(qt_svg(page["svg"])).render(painter,QRectF(0,0,page["width"],page["height"]))
                        painter.end();del writer
                        os.replace(temporary,destination)
                    finally:
                        if os.path.exists(temporary):os.unlink(temporary)
                self.report("已导出："+str(destination))
            except Exception as failure:QMessageBox.warning(self,"导出失败",str(failure))
        self.compile(exported)

    def request_language(self,method,callback):
        position=from_u16(self.source,self.source_view.textCursor().position()) if self.source_view.hasFocus() else self.focused_editor().source_selection()[1]
        prefix=self.source[:position];line=prefix.count("\n");character=u16(prefix.rsplit("\n",1)[-1])
        revision=self.revision
        def done(result,error):
            if revision!=self.revision:return
            if error:self.report(error);return
            callback(result)
        self.lsp.request("/api/lsp",self.body()|{"method":method,"position":{"line":line,"character":character}},done,key=method)

    def lsp_position(self,position):
        lines=self.source.splitlines(keepends=True);line=position["line"]
        if line>=len(lines):return len(self.source)
        return sum(map(len,lines[:line]))+from_u16(lines[line],position["character"])

    def complete(self,automatic=False):
        if self.math_state:
            self.complete_math();return
        editor=self.source_view if self.source_view.hasFocus() else self.focused_editor()
        caret=editor.textCursor().position()
        def done(reply):
            if self.math_state or editor.textCursor().position()!=caret:return
            value=reply["result"];items=value.get("items",[]) if isinstance(value,dict) else value or []
            if not items:
                if not automatic:self.report("没有补全建议")
                return
            labels=[item.get("label","") for item in items]
            from PyQt5.QtWidgets import QCompleter
            completer=QCompleter(labels,self);completer.setCaseSensitivity(Qt.CaseInsensitive)
            completer.setCompletionMode(QCompleter.UnfilteredPopupCompletion)
            previous=getattr(editor,"completer",None)
            if previous:previous.popup().hide();previous.deleteLater()
            editor.completer=completer;completer.setWidget(editor)
            revision=self.revision
            def accept(label):
                if revision!=self.revision:return
                item=items[labels.index(label)];edit=item.get("textEdit")
                if edit:
                    bounds=edit.get("range") or edit.get("replace");a,b=(self.lsp_position(bounds[key]) for key in ("start","end"));text=edit["newText"]
                else:
                    if editor==self.source_view:
                        c=editor.textCursor();a,b=sorted((from_u16(self.source,c.anchor()),from_u16(self.source,c.position())))
                    else:a,b=sorted(editor.source_selection())
                    text=item.get("insertText",label)
                edits=[(a,b,text)]
                for extra in item.get("additionalTextEdits",[]):
                    start,end=(self.lsp_position(extra["range"][key]) for key in ("start","end"));edits.append((start,end,extra["newText"]))
                source=self.source;last=len(source)+1
                for start,end,value in sorted(edits,reverse=True):
                    if end>last:self.report("补全编辑范围重叠");return
                    source=source[:start]+value+source[end:];last=start
                from .model import difference
                start,end,value=difference(self.source,source);self.replace(start,end,value,True)
            completer.activated[str].connect(accept);completer.complete(editor.cursorRect())
            completer.popup().setCurrentIndex(completer.completionModel().index(0,0))
        self.request_language("completion",done)

    def show_math_completions(self):
        state=self.math_state
        if not state or not state.get('pending') or state.get('string_mode'):
            self.math_popup.hide();return
        labels=state.get('candidates',[])
        if not labels:self.math_popup.hide();return
        self.math_popup.clear();self.math_popup.addItems(labels)
        self.math_popup.setCurrentRow(state.get('completion_index',0))
        self.math_popup.resize(280,min(210,25*len(labels)+6))
        from PyQt5.QtCore import QPoint
        point=self.math_scroll.mapToGlobal(QPoint(0,self.math_scroll.height()+3))
        self.math_popup.move(point);self.math_popup.show()

    def complete_math(self):
        state=self.math_state
        if not state or not state.get('pending') or state.get('string_mode'):return
        self.show_math_completions();command=state.get('command')
        if not command:return
        def done(result,error):
            if not self.math_state or self.math_state.get('command')!=command:return
            if error:
                self.report(error);return
            self.math_action('lsp_completions',draft=command['draft'],caret=command['draft_caret'],items=result.get('items',[]))
        self.lsp.request('/api/completion',{key:command[key] for key in ('source','start','end','caret')},done,key='math-completion')

    def format_source(self):
        def done(reply):
            edits=reply["result"] or [];source=self.source;intervals=[]
            for edit in edits:
                a,b=(self.lsp_position(edit["range"][key]) for key in ("start","end"));intervals.append((a,b,edit["newText"]))
            last=len(source)+1
            for a,b,text in sorted(intervals,reverse=True):
                if b>last:self.report("Tinymist 返回了重叠编辑");return
                source=source[:a]+text+source[b:];last=a
            self.replace(0,len(self.source),source)
        self.request_language("formatting",done)

    def semantic_highlight(self):
        def done(reply):
            legend=reply.get("legend",{}).get("tokenTypes",[]);data=(reply.get("result") or {}).get("data",[])
            line=0;character=0;spans=[]
            colors={"keyword":"#8250a3","function":"#795d19","string":"#267349","number":"#146ba0","comment":"#7a837b","operator":"#95602c","variable":"#26384a"}
            for i in range(0,len(data)-4,5):
                dl,dc,length,kind,_=data[i:i+5];line+=dl;character=dc if dl else character+dc
                role=legend[kind] if kind<len(legend) else "variable"
                spans.append((self.lsp_position({"line":line,"character":character}),self.lsp_position({"line":line,"character":character+length}),colors.get(role,"#26384a")))
            self.semantic_spans=spans;self.apply_highlights()
        self.request_language("semanticTokens/full",done)

    def highlight_views(self):
        """Views that show source text: the editors, plus the source dock when open."""
        views=list(self.editors)
        if not self.source_dock.isHidden():views.append(self.source_view)
        return views

    def apply_highlights(self):
        """Colour the semantic and engine spans in every view.

        Token lists are replaced whenever they change (the editor clears them on
        every source edit and the LSP reply repopulates them), so this runs once
        per reply and has to stay cheap: one format per colour instead of a new
        Qt object for every span, and no work for a hidden source dock.
        """
        from PyQt5.QtGui import QColor
        for editor in self.highlight_views():
            selections=[];formats={}
            convert=editor.mapping.display_position if isinstance(editor,Editor) else lambda p:u16(self.source[:p])
            for a,b,color in self.semantic_spans+self.engine_spans:
                fmt=formats.get(color)
                if fmt is None:
                    fmt=QTextCharFormat();fmt.setForeground(QColor(color));formats[color]=fmt
                selection=QTextEdit.ExtraSelection();selection.cursor=QTextCursor(editor.document())
                selection.cursor.setPosition(convert(a));selection.cursor.setPosition(convert(b),QTextCursor.KeepAnchor)
                selection.format=fmt;selections.append(selection)
            editor.setExtraSelections((editor.base_selections if isinstance(editor,Editor) else [])+selections)

    def find_replace(self):
        dialog=QDialog(self);dialog.setWindowTitle("查找 / 替换 Typst 源码")
        layout=QFormLayout(dialog);find=QLineEdit();replacement=QLineEdit();layout.addRow("查找",find);layout.addRow("替换为",replacement)
        next_button=QPushButton("查找下一个");one=QPushButton("替换选中");all_button=QPushButton("全部替换")
        layout.addRow(next_button);layout.addRow(one);layout.addRow(all_button)
        def locate():
            text=find.text()
            if not text:return
            position=self.focused_editor().source_selection()[1];at=self.source.find(text,position)
            if at<0:at=self.source.find(text)
            if at<0:self.report("未找到");return
            for formula in self.analysis.get("formulas",[]):
                a,b=(from_byte(self.source,formula[key]) for key in ("start","end"))
                if a<at+len(text) and at<b:self.editor.expanded.add(a)
            self.editor.project((at,at+len(text)));self.editor.ensureCursorVisible()
        def replace_one():
            a,b=sorted(self.editor.source_selection())
            if find.text() and self.source[a:b]==find.text():self.replace(a,b,replacement.text())
            locate()
        def replace_all():
            if find.text():self.replace(0,len(self.source),self.source.replace(find.text(),replacement.text()))
        next_button.clicked.connect(locate);one.clicked.connect(replace_one);all_button.clicked.connect(replace_all);dialog.exec_()

    def configure(self):
        dialog=QDialog(self);dialog.setWindowTitle("JSON 设置 · "+str(config_path()));dialog.resize(780,650)
        layout=QVBoxLayout(dialog);text=QPlainTextEdit()
        value=self.settings.copy();value["shortcuts"]={key:self.settings["shortcuts"].get(key,default) for key,(_,default) in self.commands.items()}
        text.setPlainText(json.dumps(value,ensure_ascii=False,indent=2));layout.addWidget(text)
        buttons=QDialogButtonBox(QDialogButtonBox.Save|QDialogButtonBox.Cancel);layout.addWidget(buttons)
        def apply():
            try:
                value=validate_settings(json.loads(text.toPlainText()));used={}
                for identifier,key in value["shortcuts"].items():
                    sequence=QKeySequence(key).toString(QKeySequence.PortableText)
                    if key and not sequence:raise ValueError("无效快捷键："+key)
                    if sequence and sequence in used:raise ValueError("快捷键冲突："+identifier+" / "+used[sequence])
                    if sequence:used[sequence]=identifier
                path=config_path();path.parent.mkdir(parents=True,exist_ok=True)
                atomic_write(path,json.dumps(value,ensure_ascii=False,indent=2).encode())
                self.settings=value;self.typesetter.settings=value
                for identifier,(action,default) in self.commands.items():action.setShortcut(QKeySequence(value["shortcuts"].get(identifier,default)))
                self.project()
                if self.math_state:self.math_canvas.refresh(self.math_state);self.reposition_math()
                dialog.accept()
            except Exception as error:QMessageBox.warning(dialog,"设置无效",str(error))
        buttons.accepted.connect(apply);buttons.rejected.connect(dialog.reject);dialog.exec_()

    def import_file(self):
        name,_=QFileDialog.getOpenFileName(self,"导入 Typst 模块或图片",str(self.workspace),"支持的资源 (*.typ *.svg *.png *.jpg *.jpeg)")
        if not name:return
        path=Path(name).resolve()
        try:relative=path.relative_to(self.workspace).as_posix()
        except ValueError:
            QMessageBox.information(self,"项目资源","请先把资源放入当前文档所在目录或其子目录，再插入引用。");return
        quoted=json.dumps(relative,ensure_ascii=False)
        text=f"#import {quoted}: *\n" if path.suffix.lower()==".typ" else f"#image({quoted})"
        a,b=sorted(self.focused_editor().source_selection());self.replace(a,b,text)

    def packages(self):
        dialog=QDialog(self);dialog.setWindowTitle("Typst 包");dialog.resize(700,500)
        layout=QVBoxLayout(dialog);namespace=QComboBox();namespace.addItems(["@local（本机）","@preview（官方）"])
        search=QLineEdit();search.setPlaceholderText("包名或描述");button=QPushButton("搜索 / 刷新")
        listing=QListWidget();insert=QPushButton("插入 import（缺少的官方包会先安装）")
        for widget in (namespace,search,button,listing,insert):layout.addWidget(widget)
        records=[];closed=[False]
        def populate(result,error):
            if closed[0]:return
            if error:QMessageBox.warning(dialog,"包查询失败",error);return
            records[:]=result.get("items",[]);listing.clear()
            for item in records:listing.addItem(f"{item.get('namespace','preview')}/{item['name']}:{item['version']}  {item.get('description','')}")
        def refresh():
            action="local" if namespace.currentIndex()==0 else "search"
            self.services.request("/api/packages",{"action":action,"query":search.text()},populate,key="packages")
        def choose():
            row=listing.currentRow()
            if row<0:return
            item=records[row];spec=f"@{item.get('namespace','preview')}/{item['name']}:{item['version']}"
            def installed(result,error):
                if closed[0]:return
                if error:QMessageBox.warning(dialog,"安装失败",error);return
                self.replace(0,0,result["import"]);dialog.accept()
            if item.get("namespace")=="local":installed({"import":f'#import "{spec}": *\n'},None)
            else:self.services.request("/api/packages",{"action":"install","spec":spec},installed)
        button.clicked.connect(refresh);namespace.currentIndexChanged.connect(refresh);insert.clicked.connect(choose)
        refresh();dialog.exec_();closed[0]=True

    def closeEvent(self,event):
        if not self.finish_formula() or not self.dirty_prompt():event.ignore();return
        self.compile_timer.stop();self.core.close()
        self.completion_timer.stop()
        self.raw_timer.stop();self.math_popup.hide()
        if self.services:self.services.close()
        if self.lsp:self.lsp.close()
        if self in Window.windows:Window.windows.remove(self)
        event.accept()

    def resizeEvent(self,event):
        super().resizeEvent(event)
        if hasattr(self,"math_scroll"):QTimer.singleShot(0,self.reposition_math)
