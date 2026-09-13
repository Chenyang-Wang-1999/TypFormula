"""Offscreen integration tests. Never opens a visible application window."""
import os,base64
os.environ["QT_QPA_PLATFORM"]="offscreen"
import unittest
from unittest.mock import patch
from tempfile import TemporaryDirectory
from pathlib import Path
from PyQt5.QtWidgets import QApplication
from PyQt5.QtGui import QTextCursor,QImage,QPainter,QFontDatabase,QFont
from PyQt5.QtCore import Qt,QEventLoop,QTimer,QRect
from .model import Projection,to_byte,from_byte,u16,from_u16,validate_settings,atomic_write
from .bridge import Core
from .mathview import Typesetter,active_list
from .window import Window,initial_window_geometry
from .rawcache import signature,raw_key

APPLICATION=QApplication.instance() or QApplication([])
from .model import ROOT
for font in [ROOT/'fonts/NewCMMath-Regular.otf',Path('C:/Windows/Fonts/segoeui.ttf'),Path('C:/Windows/Fonts/msyh.ttc'),Path('C:/Windows/Fonts/consola.ttf')]:
    if font.exists():QFontDatabase.addApplicationFont(str(font))
APPLICATION.setFont(QFont('Microsoft YaHei',9))

class MappingTest(unittest.TestCase):
    def test_initial_window_fits_and_centers_each_monitor_work_area(self):
        for available in (QRect(0,0,1463,775),QRect(1707,0,1707,912),QRect(-1280,120,1280,680)):
            geometry=initial_window_geometry(available)
            self.assertTrue(available.contains(geometry),f'{available} does not contain {geometry}')
            self.assertLessEqual(abs(geometry.center().x()-available.center().x()),1)
            self.assertLessEqual(abs(geometry.center().y()-available.center().y()),1)
            self.assertLessEqual(geometry.width(),1400)
            self.assertLessEqual(geometry.height(),900)

    def test_qt_glyph_symbol_compatibility_preserves_use_style_and_geometry(self):
        from PyQt5.QtCore import qInstallMessageHandler,QRectF
        from PyQt5.QtSvg import QSvgRenderer
        from .svg import qt_svg
        svg='<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="30" height="30"><use xlink:href="#glyph" x="5" y="6" fill="#ff0000"/><defs><symbol id="glyph" overflow="visible"><path d="M0 0H10V10H0Z"/></symbol></defs></svg>'
        messages=[];previous=qInstallMessageHandler(lambda kind,context,message:messages.append(message))
        try:
            original=QSvgRenderer(svg.encode())
            self.assertTrue(any('undefined' in message for message in messages),'fixture must reproduce Qt 5 symbol rejection')
            messages.clear();renderer=QSvgRenderer(qt_svg(svg))
            image=QImage(30,30,QImage.Format_ARGB32);image.fill(Qt.white)
            painter=QPainter(image);renderer.render(painter,QRectF(0,0,30,30));painter.end()
            self.assertFalse(messages,messages)
            self.assertEqual(image.pixelColor(7,8).name(),'#ff0000')
            self.assertEqual(image.pixelColor(2,2).name(),'#ffffff')
        finally:qInstallMessageHandler(previous)

    def test_unicode_and_formula_copy_round_trip(self):
        source="中文😀 $a/b$ 尾 $x$"
        a=source.index('$');b=source.index('$',a+1)+1
        formula={"start":to_byte(source,a),"end":to_byte(source,b),"editable":True}
        projection=Projection(source,[formula])
        self.assertEqual(projection.text,"中文😀 \ufffc 尾 $x$")
        self.assertEqual(projection.copy(0,u16(projection.text)),source)
        self.assertEqual(projection.copy(u16(source[:a]),u16(source[:a])+1),"$a/b$")
        for index in (0,a,b,len(source)):
            self.assertEqual(projection.source_position(projection.display_position(index)),index)
        for index in range(len(source)+1):
            self.assertEqual(from_byte(source,to_byte(source,index)),index)
            self.assertEqual(from_u16(source,u16(source[:index])),index)

    def test_new_formula_can_stay_in_source(self):
        source="$x$";formula={"start":0,"end":3,"editable":True}
        self.assertEqual(Projection(source,[formula],[0]).text,source)
        formula["editable"]=False
        self.assertEqual(Projection(source,[formula]).text,source)

    def test_settings_and_atomic_utf8_save(self):
        with self.assertRaises(ValueError):validate_settings({"font_size":0})
        with self.assertRaises(ValueError):validate_settings({"svg_scale":float('nan')})
        with TemporaryDirectory() as directory:
            path=Path(directory)/"中文.typ";atomic_write(path,"😀 $x$".encode());self.assertEqual(path.read_text('utf-8'),"😀 $x$")

class NativeTest(unittest.TestCase):
    def setUp(self):
        self.window=Window();self.window.compile_timer.stop()
    def tearDown(self):
        self.window.math_state=None;self.window.saved=self.window.source
        self.window.close();self.window.deleteLater()

    def load(self,source):
        window=self.window;window.replace(0,len(window.source),source)
        window.compile_timer.stop()

    def test_command_commit_updates_all_formula_objects_before_qt_relayout(self):
        import sys
        errors=[]
        window=self.window
        self.load('中文 $ x $ 后面正文 $ y $')
        window.split();window.show();APPLICATION.processEvents()
        # Exceptions in Qt virtual callbacks go to excepthook, not the caller.
        with patch.object(sys,'excepthook',side_effect=lambda kind,error,tb:errors.append(error)):
            window.activate(window.analysis['formulas'][0]['start'])
            window.math_action('input',text='\\dots')
            window.math_action('key',key='Enter')
            APPLICATION.processEvents()
            self.assertEqual(window.source,'中文 $ dots x $ 后面正文 $ y $')
            window.finish_formula()
            window.undo();APPLICATION.processEvents()
            window.redo();APPLICATION.processEvents()
            # Also exercise insertText (body edits) and full projection (load).
            window.replace(0,0,'😀中文 ');APPLICATION.processEvents()
            self.load('短 $ z $ 后面 $ q $');APPLICATION.processEvents()
        self.assertEqual(errors,[],str(errors))
        for editor in window.editors:
            for formula in editor.object_by_id.values():
                self.assertEqual(window.source.encode()[formula['start']:formula['end']].decode()[0],'$')

    def test_typing_a_then_script_shows_an_empty_slot_and_visible_caret(self):
        window=self.window
        for key,role in (('^','upper'),('_','lower')):
            window.finish_formula();self.load('$ $');window.activate(0)
            window.math_action('input',text='a');window.math_action('input',text=key)
            node=next(n for n in window.view_nodes(window.math_state['view']) if n['kind']=='scripts')
            slot=next(c for c in node['children'] if c.get('role')==role)
            self.assertEqual(slot['kind'],'empty-cell')
            self.assertTrue(any(op[0]=='slot' and op[3][0]>0 and op[3][1]>0 for op in window.math_canvas.box.operations))
            self.assertTrue(any(stop[4] for stop in window.math_canvas.box.stops))
            window.math_action('input',text='2')
            self.assertIn('a'+key+'(2)',window.source)
            self.assertFalse(any(op[0]=='slot' for op in window.math_canvas.box.operations))

    def test_scoped_value_commands_remain_values_after_real_lsp_completion(self):
        window=self.window
        definitions='#[\n#let mathbf(x) = $bold(upright(#x))$\n#let rme = $upright(e)$\n#let rmi = $upright(i)$\n'
        for command in ('rme','rmi','rmi + 1'):
            window.finish_formula();self.load(definitions+'$ x $\n]')
            window.activate(window.analysis['formulas'][-1]['start'])
            window.math_action('input',text='\\'+command)
            ctx=window.math_state['command']
            if command in ('rme','rmi'):
                result=self.service('/api/completion',{key:ctx[key] for key in ('source','start','end','caret')},window.lsp)
                window.math_action('lsp_completions',draft=ctx['draft'],caret=ctx['draft_caret'],items=result.get('items',[]))
            window.math_action('key',key='Enter')
            self.assertFalse(window.math_state['pending'])
            self.assertEqual(window.source,definitions+f'$ {command} x $\n]')
            self.assertFalse(any(n['kind']=='empty-cell' for n in window.view_nodes(window.math_state['view'])))
            values=[n for n in window.view_nodes(window.math_state['view']) if n['kind']=='macro' and n['text'] in ('rme','rmi')]
            self.assertEqual(len(values),1)

    def test_multiline_sum_uses_engine_limits_in_scoped_document(self):
        window=self.window
        source='#[\n#let mathbf(x) = $bold(upright(#x))$\n#let rme = $upright(e)$\n#let rmi = $upright(i)$\n\n$ H_("int") = & g sum_(j) sigma_(x) (a rme^(i) + a^(dagger)) '+chr(92)+'\n= & x $\n]'
        self.load(source);formula=window.analysis['formulas'][-1]
        window.activate(formula['start']);window.background()
        key=(window.formula_context(formula),'sum_(j)',True)
        loop=QEventLoop();timer=QTimer()
        timer.timeout.connect(lambda:loop.quit() if key in window.typesetter.placements else None)
        deadline=QTimer();deadline.setSingleShot(True);deadline.timeout.connect(loop.quit)
        timer.start(10);deadline.start(15000);loop.exec_();timer.stop();deadline.stop()
        self.assertEqual(window.typesetter.placements.get(key,{}).get('lower'),'limits')
        node=next(n for n in window.view_nodes(window.math_state['view']) if n.get('attachment')=='sum_(j)')
        self.assertEqual(node['_placement']['lower'],'limits')
        centered=window.typesetter.layout(node)
        side=window.typesetter.layout(dict(node,_placement={}))
        xpos=lambda box:next(x for kind,x,y,value in box.operations if kind=='text' and value[0]=='\U0001D457')
        self.assertLess(xpos(centered),xpos(side))
        preview=self.service('/api/preview',window.body()|{'preview':True})
        self.assertTrue(preview['pages'])
        self.assertEqual(window.source,source,'排版和取图不应修改用户源码')

    def test_text_diagnostics_underline_both_editors_with_unicode_projection(self):
        self.load('中文😀 $x$\n#unknownname');window=self.window
        from PyQt5.QtGui import QTextCharFormat
        diagnostic={'range':{'start':{'line':1,'character':1},'end':{'line':1,'character':12}},'message':'unknown name','severity':1}
        window.source_dock.show();window.mark_diagnostics([diagnostic])
        for editor in (window.editor,window.source_view):
            marks=[s for s in editor.extraSelections() if s.format.underlineStyle()==QTextCharFormat.WaveUnderline]
            self.assertEqual(len(marks),1)
            self.assertEqual(marks[0].cursor.selectedText(),'unknownname')
            self.assertEqual(marks[0].format.toolTip(),'unknown name')
            self.assertEqual(marks[0].format.underlineColor().name(),'#c43e3e')
        window.mark_diagnostics([dict(diagnostic,severity=2)])
        self.assertEqual(window.diagnostic_spans,[],'警告不应让公式变成失败源码框')
        self.assertEqual(window.editor.extraSelections()[-1].format.underlineColor().name(),'#ad7800')
        window.mark_diagnostics([])
        self.assertFalse(any(s.format.underlineStyle()==QTextCharFormat.WaveUnderline for s in window.editor.extraSelections()))

    def test_hover_maps_source_position_and_discards_old_tooltips(self):
        from PyQt5.QtWidgets import QToolTip
        self.load('中文😀 $x$ #text("ok")');window=self.window;asked=[]
        at=window.source.index('text')+2
        cursor=window.editor.textCursor();cursor.setPosition(window.editor.mapping.display_position(at));window.editor.setTextCursor(cursor)
        def request(route,body,callback,key=None,dropped=None):asked.append((body,callback))
        with patch.object(window.lsp,'request',side_effect=request),patch.object(QToolTip,'showText') as shown:
            window.language_help.hover(window.editor)
            self.assertEqual(asked[0][0]['position'],{'line':0,'character':u16(window.source[:at])})
            asked[0][1]({'result':{'contents':{'kind':'markdown','value':'text <script>literal</script>'}}},None)
            self.assertIn('&lt;script&gt;',shown.call_args.args[1])
            shown.reset_mock();window.language_help.hover(window.editor);window.language_help.cancel()
            asked[1][1]({'result':{'contents':'obsolete'}},None)
            shown.assert_not_called()
        self.assertEqual(window.commands['definition'][0].shortcut().toString(),'F12')

    def test_definition_link_reveals_folded_definition_without_opening_draft(self):
        self.load('#let fnn(x) = x\n#fnn(1)');window=self.window
        bounds={'start':{'line':0,'character':5},'end':{'line':0,'character':8}}
        result={'targetUri':(window.workspace/'untitled.typ').as_uri(),'targetSelectionRange':bounds,'targetRange':bounds}
        def request(route,body,callback,key=None,dropped=None):callback({'result':[result]},None)
        with patch.object(window.lsp,'request',side_effect=request):window.language_help.goto(position=window.source.rindex('fnn'))
        self.assertIsNone(window.definition_draft)
        self.assertFalse(window.source_dock.isHidden())
        self.assertEqual(window.source_view.textCursor().selectedText(),'fnn')

    def test_real_lsp_diagnoses_a_missing_file_and_its_unsaved_changes(self):
        import uuid
        window=self.window;window.compile_timer.stop();window.diagnostic_timer.stop()
        name='lsp-unsaved-'+uuid.uuid4().hex+'.typ'
        body=window.body()|{'path':name,'source':'#unknownname','method':'diagnostics'}
        first=self.service('/api/lsp',body,window.lsp)
        self.assertTrue(any('unknownname' in d['message'] for d in first['diagnostics']))
        second=self.service('/api/lsp',body|{'source':'#otherunknown'},window.lsp)
        self.assertEqual(second['version'],first['version']+1)
        self.assertTrue(any('otherunknown' in d['message'] for d in second['diagnostics']))
        self.assertFalse((window.workspace/name).exists(),'LSP 同步不应创建占位文件')

    def test_real_lsp_hover_and_definition_reach_text_editor(self):
        from PyQt5.QtWidgets import QToolTip
        window=self.window
        with TemporaryDirectory() as directory:
            path=Path(directory)/'main.typ';path.write_text('#let named = 1\n#named',encoding='utf8')
            try:
                window.load(path);window.compile_timer.stop();window.diagnostic_timer.stop()
                at=window.source.rindex('named')+2
                cursor=window.editor.textCursor();cursor.setPosition(window.editor.mapping.display_position(at));window.editor.setTextCursor(cursor)
                def wait_for(predicate):
                    loop=QEventLoop();timer=QTimer();timer.timeout.connect(lambda:loop.quit() if predicate() else None)
                    deadline=QTimer();deadline.setSingleShot(True);deadline.timeout.connect(loop.quit)
                    timer.start(10);deadline.start(10000);loop.exec_();timer.stop();deadline.stop();self.assertTrue(predicate())
                with patch.object(QToolTip,'showText') as shown:
                    window.language_help.hover(window.editor);wait_for(lambda:shown.called)
                    self.assertTrue(shown.call_args.args[1])
                window.language_help.goto(position=at)
                wait_for(lambda:window.source_view.textCursor().selectedText()=='named')
                self.assertFalse(window.source_dock.isHidden())
            finally:window.load()

    def test_cross_file_definition_preserves_unsaved_source_and_history(self):
        window=self.window;opened=[]
        with TemporaryDirectory() as directory:
            main=Path(directory)/'main.typ';target=Path(directory)/'defs.typ'
            main.write_text('#import "defs.typ": named\n#named',encoding='utf8')
            target.write_text('#let named = 1',encoding='utf8')
            try:
                window.load(main);window.replace(len(window.source),len(window.source),'\nUnsaved')
                source=window.source;history=list(window.history)
                window.compile_timer.stop();window.diagnostic_timer.stop()
                previous=set(Window.windows)
                window.language_help.goto(position=source.rindex('#named')+3)
                loop=QEventLoop();timer=QTimer()
                def check():
                    opened[:]=[w for w in Window.windows if w not in previous]
                    if opened:loop.quit()
                timer.timeout.connect(check);timer.start(10)
                deadline=QTimer();deadline.setSingleShot(True);deadline.timeout.connect(loop.quit);deadline.start(10000)
                loop.exec_();timer.stop();deadline.stop()
                self.assertEqual(len(opened),1)
                self.assertEqual(opened[0].path,target.resolve())
                self.assertEqual(opened[0].source_view.textCursor().selectedText(),'named')
                self.assertEqual(window.source,source);self.assertEqual(window.history,history)
            finally:
                for child in opened:child.saved=child.source;child.close();child.deleteLater()
                window.load()

    def test_bound_style_has_a_visible_editable_argument_caret(self):
        window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):
            self.load('#let styled(x) = $bold(upright(#x))$\n$styled(a)$')
            window.activate(window.analysis['formulas'][-1]['start'])
            window.math_action('key',key='ArrowRight')
            self.assertTrue(any(stop[4] for stop in window.math_canvas.box.stops))
            self.assertTrue(any(n.get('_active') for n in window.view_nodes(window.math_state['view']) if n['kind']=='style'))
            window.math_action('input',text='z')
            self.assertTrue(window.source.endswith('$styled(z a)$'))

    def test_failed_raw_macro_enters_source_and_can_be_repaired(self):
        self.load('$unknownfn(a)$');window=self.window
        formula=window.analysis['formulas'][0]
        node=next(n for n in window.view_nodes(formula['view']) if n['kind']=='raw_macro')
        window.typesetter.cache[raw_key(node)]=False;window.typesetter.touch()
        self.assertTrue(any(op[0]=='failed' for op in window.editor.handler.box(formula).operations))
        window.activate(0);window.math_action('key',key='ArrowRight')
        self.assertTrue(window.math_state['pending']);self.assertEqual(window.math_state['command']['draft'],'unknownfn(a)')
        window.math_action('key',key='Escape')
        self.assertTrue(any(n['kind']=='raw_macro' for n in window.view_nodes(window.math_state['view'])))
        window.math_action('key',key='ArrowLeft')
        window.math_action('key',key='a',ctrl=True);window.math_action('input',text='sqrt(3)')
        window.math_action('key',key='Enter');window.finish_formula()
        self.assertEqual(window.source,'$sqrt(3)$')

    def test_style_failure_is_cached_and_reply_only_relayouts_its_readers(self):
        window=self.window;asked=[]
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/glyphs':asked.append((body,callback))
        with patch.object(window.services,'request',side_effect=request):
            self.load('$bold(x)$ between $frac(a,b)$')
            unrelated=window.analysis['formulas'][1];box=window.editor.handler.box(unrelated)
            asked[0][1](None,'cannot resolve glyphs')
            self.assertIs(window.typesetter.glyphs[('', 'bold(x)',False)],False)
            self.assertIs(window.editor.handler.box(unrelated),box)
            window.load_glyphs();window.replace(len(window.source),len(window.source),'!')
            self.assertEqual(len(asked),1)
            style=window.analysis['formulas'][0]
            self.assertTrue(any(op[0]=='failed' for op in window.editor.handler.box(style).operations))
            window.refresh_svg();self.assertEqual(len(asked),2)
            asked[-1][1]({'items':[{'glyphs':''}]},None)
            window.load_glyphs();self.assertEqual(len(asked),2)
            self.assertFalse(any(op[0]=='failed' for op in window.editor.handler.box(style).operations))

    def test_unicode_diagnostics_reach_static_and_active_calls_and_clear(self):
        source='中文😀\n第二行 $unknownfn(a)$';self.load(source);window=self.window
        formula=window.analysis['formulas'][0];window.activate(formula['start'])
        line=source.splitlines()[1];a=line.index('unknownfn');b=a+len('unknownfn')
        diagnostic={'range':{'start':{'line':1,'character':u16(line[:a])},'end':{'line':1,'character':u16(line[:b])}},'message':'unknown variable','severity':1}
        window.mark_diagnostics([diagnostic])
        for view in (formula['view'],window.math_state['view']):
            node=next(n for n in window.view_nodes(view) if n['kind']=='raw_macro')
            self.assertEqual(node['error'],'unknown variable')
        window.math_action('key',key='ArrowRight');self.assertTrue(window.math_state['pending']);self.assertEqual(window.math_state['command']['draft'],'unknownfn(a)')
        window.math_action('key',key='Escape');window.mark_diagnostics([])
        self.assertTrue(all(not n.get('error') for n in window.view_nodes(window.math_state['view'])))
        window.mark_diagnostics([dict(diagnostic,severity=2)])
        self.assertTrue(all(not n.get('error') for n in window.view_nodes(window.math_state['view'])))

    def test_active_raw_macro_fetches_inner_fragment_without_overlap(self):
        self.load('$bold(arrow.r)$');window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):
            window.load_raw();window.activate(0);window.math_action('key',key='ArrowRight')
            calls.clear();window.load_raw()
        ranges=[r for body in calls if 'raw' in body for r in body['raw']]
        self.assertEqual([window.source.encode()[r['start']:r['end']].decode() for r in ranges],['arrow.r'])
        self.assertTrue(any(op[0]=='svg' for op in window.math_canvas.box.operations))

    def test_template_call_images_are_distinct_through_real_frontend_service(self):
        self.load('#let wrap(x) = $bold(#x/2)$\n$wrap(a) + wrap(b b b b)$');window=self.window
        requests=[];original=window.services.request
        def observed(route,body,callback,key=None,dropped=None):
            if route=='/api/render':requests.append(body)
            return original(route,body,callback,key,dropped)
        with patch.object(window.services,'request',side_effect=observed):window.load_raw()
        loop=QEventLoop();timer=QTimer();timer.timeout.connect(lambda:loop.quit() if not window.raw_pending else None)
        limit=QTimer();limit.setSingleShot(True);limit.timeout.connect(loop.quit)
        timer.start(10);limit.start(15000);loop.exec_();timer.stop();limit.stop()
        self.assertFalse(window.raw_pending)
        formula=window.analysis['formulas'][-1]
        calls=[n for n in window.view_nodes(formula['view']) if n['kind']=='raw_macro']
        items=[window.typesetter.raw(n) for n in calls]
        self.assertTrue(all(isinstance(item,dict) for item in items),items)
        self.assertGreater(items[1]['width'],items[0]['width'])
        self.assertEqual(len(requests[0]['raw']),2)
        self.assertNotEqual(raw_key(calls[0]),raw_key(calls[1]))
        identities=[raw_key(n) for n in calls]
        with patch.object(window.services,'request',side_effect=observed):
            window.replace(0,0,'中文😀\n\n');window.load_raw()
        moved=[n for n in window.view_nodes(window.analysis['formulas'][-1]['view']) if n['kind']=='raw_macro']
        self.assertEqual([raw_key(n) for n in moved],identities)
        self.assertEqual(len(requests),1,'移动公式只更新请求坐标，不丢弃已渲染的实例')

    def test_a_replaced_render_batch_does_not_strand_its_fragments(self):
        """A batch replaced in the queue must not leave its fragments pending for good.

        `Services.request` coalesces by key and a replaced request is never sent, so its
        callback never runs. `load_raw` skips fragments by `raw_pending`, which only a
        reply clears, and the pass that replaces a batch asks for a different set of
        fragments -- so a replaced batch used to strand the fragments it was carrying:
        never requested again, not marked failed either, drawn as source for the rest of
        the session. It is what made a long document lose images while scrolling.
        """
        window=self.window
        self.load('$floor(a)$')
        window.compile_timer.stop();window.raw_timer.stop()
        window.show();APPLICATION.processEvents();window.compile_timer.stop()
        fragment=window.raw_fragments(window.analysis['formulas'][0]['view'])[0]
        bounds=(fragment['render_request']['start'],fragment['render_request']['end'])
        wanted=raw_key(fragment)
        seen=[];original=window.services.request
        def observed(route,body,callback,key=None,dropped=None):
            if route!='/api/render':return original(route,body,callback,key,dropped)
            record={'ranges':[(r['start'],r['end']) for r in body['raw']],'called':0,'dropped':0}
            seen.append(record)
            def counted(result,error,record=record):
                record['called']+=1;return callback(result,error)
            def replaced(record=record):
                record['dropped']+=1
                if dropped:dropped()
            # Forward `dropped` only when the sender passed one: without the fix there is
            # no such argument, and this test is about the strands, not about the shim.
            return original(route,body,counted,key,**({'dropped':replaced} if dropped else {}))
        with patch.object(window.services,'request',side_effect=observed):
            # One request occupies the pipe, so the next two both stay queued: the second
            # replaces the first before either is sent.
            original('/api/render',window.body()|{'raw':[],'formulas':[],'context_end':0},lambda result,error:None,key='blocker')
            window.load_raw()
            self.assertEqual([record['ranges'] for record in seen],[[bounds]])
            # A second formula gives the next pass a fragment the first one was not asked
            # for, which is exactly what makes it a different batch rather than a rerun.
            window.replace(len(window.source),len(window.source),'\n\n$ceil(b)$')
            window.compile_timer.stop();window.raw_timer.stop()
            window.load_raw()
            self.assertEqual(len(seen),2)
            for _ in range(80):
                loop=QEventLoop();timer=QTimer();timer.setSingleShot(True);timer.timeout.connect(loop.quit);timer.start(50);loop.exec_()
                if wanted not in window.raw_pending and isinstance(window.typesetter.raw(fragment),dict):break
            window.compile_timer.stop();window.raw_timer.stop()
        # The strand first, because that is the half a reader can see: a batch with no
        # bookkeeping leaves its fragments pending, and the next pass asks for a different
        # set, so nothing ever asks for them again.
        self.assertNotIn(wanted,window.raw_pending,'a replaced batch must not leave its fragments pending')
        self.assertTrue(any(bounds in record['ranges'] for record in seen[1:]),'the stranded fragment has to be asked for again')
        self.assertIsInstance(window.typesetter.raw(fragment),dict,'and its image has to arrive')
        self.assertEqual(seen[0]['dropped'],1,'a replaced batch has to hear that it never ran')
        self.assertEqual(seen[0]['called'],0,'and its callback must not run')

    def test_native_objects_copy_and_edit_undo(self):
        self.load("中文😀 $ a/b $ 末尾")
        window=self.window;editor=window.editor
        self.assertEqual(len(editor.object_data),1)
        editor.selectAll();self.assertEqual(editor.createMimeDataFromSelection().text(),window.source)
        before=window.source;window.replace(0,2,"正文")
        window.undo();self.assertEqual(window.source,before)
        window.redo();self.assertTrue(window.source.startswith("正文"))

    def test_opaque_macro_stays_code_and_loaded_formula_folds(self):
        self.load("#let opaque(x) = $lr(#x, size: #100%)$\n$opaque(y)$")
        self.assertEqual(len(self.window.editor.object_data),2)
        self.assertNotIn("$lr(#x, size: #100%)$",self.window.editor.toPlainText())
        self.window.editor.selectAll()
        self.assertEqual(self.window.editor.createMimeDataFromSelection().text(),self.window.source)

    def test_definition_block_drafts_commit_once_and_cancel_without_updating(self):
        source='#let dbl(x) = $#x + 1$\n#let value = 2\n\n正文 $dbl(a)$'
        self.load(source);window=self.window
        block=next(f for f in window.editor.object_data.values() if f.get('definition_block'))
        self.assertEqual(block['count'],2)
        history=len(window.history);revision=window.revision
        window.activate(block['start'])
        self.assertIsNotNone(window.definition_draft)
        with patch.object(window.core,'call',wraps=window.core.call) as calls:
            window.definition_draft.source.setPlainText('#let dbl(x) = $#x - 1$\n#let value = 2')
            self.assertEqual(window.source,source);self.assertEqual(window.revision,revision)
            self.assertEqual(calls.call_count,0)
        window.cancel_definitions();self.assertEqual(window.source,source)
        window.activate(block['start'])
        window.definition_draft.source.setPlainText('#let dbl(x) = $#x - 1$\n#let value = 2')
        window.confirm_definitions()
        self.assertIsNone(window.definition_draft)
        self.assertIn('#x - 1',window.source);self.assertEqual(len(window.history),history+1)
        window.undo();self.assertEqual(window.source,source)

    def test_definition_block_displays_full_source_with_editor_token_colours(self):
        from .definitions import source_document
        source='#let caption = "中文😀"\n#let twice(x) = $x + x$\n\n$twice(a)$'
        self.load(source);window=self.window;editor=window.editor
        block=next(f for f in editor.object_data.values() if f.get('definition_block'))
        expected=source[:from_byte(source,block['end'])]
        quoted=source.index('"');end=source.index('"',quoted+1)+1
        window.semantic_spans=[(1,4,'#8250a3'),(quoted,end,'#267349')]
        window.apply_highlights()
        document=source_document(editor,block)
        self.assertEqual(document.toPlainText(),expected)
        self.assertEqual(document.defaultFont(),editor.font())
        self.assertIs(source_document(editor,block),document)
        def colour_at(doc,position):
            cursor=QTextCursor(doc);cursor.setPosition(position)
            cursor.setPosition(position+1,QTextCursor.KeepAnchor)
            return cursor.charFormat().foreground().color().name()
        self.assertEqual(colour_at(document,1),'#8250a3')
        self.assertEqual(colour_at(document,u16(source[:quoted])),'#267349')
        window.semantic_spans=[(1,4,'#146ba0')];window.apply_highlights()
        self.assertEqual(colour_at(source_document(editor,block),1),'#146ba0')
        self.assertEqual(window.source,source)

    def test_definition_source_wraps_long_lines_without_truncation(self):
        from .definitions import source_document
        source='#let caption = "'+('字😀'*80)+'"'
        self.load(source);editor=self.window.editor;editor.resize(240,400)
        block=next(f for f in editor.object_data.values() if f.get('definition_block'))
        document=source_document(editor,block)
        self.assertEqual(document.toPlainText(),source)
        self.assertLessEqual(document.size().width(),max(72,editor.viewport().width()-50))
        self.assertGreater(document.size().height(),editor.fontMetrics().lineSpacing()*2)
        self.assertAlmostEqual(editor.handler.box(block).height,document.size().height())

    def test_plain_context_edit_keeps_macro_definition_block_and_distant_view(self):
        source='正文\n\n#let dbl(x) = $#x + 1$\n\n'+('paragraph\n\n'*6)+'$dbl(a)$'
        self.load(source);window=self.window
        before=signature(window.analysis['formulas'][-1]['view'])
        with patch.object(window.core,'call',wraps=window.core.call) as calls:
            window.replace(0,0,'新')
        targets=[c.kwargs.get('start') for c in calls.call_args_list if c.args[0]=='analyze_formula']
        self.assertNotIn(window.analysis['formulas'][-1]['start'],targets)
        self.assertEqual(signature(window.analysis['formulas'][-1]['view']),before)

    def test_matrix_padding_is_visible_and_editable(self):
        self.load('$mat(a, b; c)$');window=self.window
        window.activate(0)
        cursors=[s[3] for s in window.math_canvas.box.stops]
        target=next(c for c in cursors if c['slices'] and c['slices'][-1]['cell']==3)
        window.math_action('click',cursor=target);window.math_action('input',text='z')
        self.assertEqual(window.source,'$mat(a, b; c, z)$')
        window.finish_formula();window.activate(0)
        self.assertIn('z',str(window.math_state['view']))

    def test_the_list_toolbar_follows_the_caret_into_a_matrix(self):
        self.load('$mat(1, 2; 3, 4)$');window=self.window
        # `isVisible` answers for the whole ancestor chain, so the window has to be on
        # screen for the toolbar's own state to be what it reports.
        window.show();APPLICATION.processEvents()
        self.assertFalse(window.listbar.isVisible(),'没有活动公式时不该有列表工具栏')
        window.activate(0)
        self.assertFalse(window.listbar.isVisible(),'光标还在公式根上，矩阵没有被激活')
        window.math_action('key',key='ArrowRight')
        self.assertEqual(window.math_state['cursor']['slices'][0]['cell'],0)
        self.assertTrue(window.listbar.isVisible(),'光标进了矩阵的格子，列表工具栏该出来')
        window.math_action('key',key='ArrowRight')
        self.assertTrue(window.listbar.isVisible(),'在同一行的下一格仍在这个列表里')
        window.finish_formula()
        self.assertFalse(window.listbar.isVisible(),'离开公式后列表工具栏该收起来')
        window.hide()

    def test_the_list_toolbar_also_serves_an_alignment(self):
        self.load('$a & b \\\\ c & d$');window=self.window
        window.show();APPLICATION.processEvents()
        window.activate(0);window.math_action('key',key='ArrowRight')
        self.assertEqual(active_list(window.math_state['view'])['kind'],'multiline')
        self.assertTrue(window.listbar.isVisible())
        # 光标移回公式根：对齐容器还在，但列表不再“被激活”。
        window.math_action('key',key='ArrowLeft')
        self.assertIsNone(active_list(window.math_state['view']))
        self.assertFalse(window.listbar.isVisible())
        window.hide()

    def test_the_list_toolbar_commands_edit_the_list_the_caret_is_in(self):
        self.load('$mat(a, b; c, d)$');window=self.window
        window.activate(0);window.math_action('key',key='ArrowRight')
        window.commands['removeColumn'][0].trigger()
        self.assertEqual(window.source,'$mat(b; d)$')
        window.commands['addRow'][0].trigger()
        self.assertEqual(window.source,'$mat(b; d; "")$')
        window.commands['removeRow'][0].trigger()
        self.assertEqual(window.source,'$mat(d; "")$')
        window.commands['removeRow'][0].trigger()
        self.assertEqual(window.source,'$mat("")$')
        window.commands['removeColumn'][0].trigger()
        self.assertEqual(window.source,'$mat("")$')

    def test_deleting_the_last_row_of_a_list_says_why_it_cannot(self):
        self.load('$mat(a)$');window=self.window
        window.activate(0);window.math_action('key',key='ArrowRight')
        self.assertEqual(window.math_state['cursor']['slices'][0]['cell'],0)
        window.commands['removeRow'][0].trigger()
        self.assertEqual(window.source,'$mat(a)$')
        self.assertEqual(window.math_state['message'],'只剩一行，无法删除')
        self.assertIn('只剩一行',window.statusBar().currentMessage())

    def test_definition_block_enters_by_arrows_and_enter_commits(self):
        from PyQt5.QtGui import QKeyEvent
        source='before\n#let dbl(x) = $#x + 1$\nafter $dbl(a)$'
        self.load(source);window=self.window;editor=window.editor
        block=next(f for f in editor.object_data.values() if f.get('definition_block'))
        for key,position in [(Qt.Key_Right,block['start']),(Qt.Key_Left,block['end']),
                             (Qt.Key_Down,0),(Qt.Key_Up,block['end']+1)]:
            editor.project((position,position))
            APPLICATION.sendEvent(editor,QKeyEvent(6,key,Qt.NoModifier))
            self.assertIsNotNone(window.definition_draft,f'arrow {key} must enter the definition block')
            self.assertLess(window.definition_draft.content_height(),100)
            APPLICATION.sendEvent(window.definition_draft.source,QKeyEvent(6,Qt.Key_Escape,Qt.NoModifier))
            self.assertEqual(window.source,source)
        window.activate(block['start'])
        draft=window.definition_draft.source
        draft.setPlainText('#let dbl(x) = $#x - 1$')
        APPLICATION.sendEvent(draft,QKeyEvent(6,Qt.Key_Return,Qt.NoModifier))
        self.assertIsNone(window.definition_draft);self.assertIn('#x - 1',window.source)

    def test_definition_draft_leaves_on_an_arrow_that_cannot_move(self):
        """The macro box is left by losing focus; an arrow at the edge is one way to ask.

        Enter and Esc were the only exits, so clicking away left the box open with the
        caret outside it -- unable to type, unable to close. The arrows are the same
        request as clicking: at the first line Up, at the last line Down, at the very
        start Left, at the very end Right. Inside the text they must still move the caret,
        or a multi-line definition could not be edited.
        """
        from PyQt5.QtGui import QKeyEvent
        source='before\n#let dbl(x) = $#x + 1$\nafter $dbl(a)$'
        for key in (Qt.Key_Up,Qt.Key_Left):
            self.load(source);window=self.window
            block=next(f for f in window.editor.object_data.values() if f.get('definition_block'))
            window.activate(block['start']);draft=window.definition_draft.source
            APPLICATION.sendEvent(draft,QKeyEvent(6,key,Qt.NoModifier))
            self.assertIsNone(window.definition_draft,f'{key} at the start of the box must leave it')
            self.assertIn('dbl(x)',window.source,'leaving by an arrow keeps the text, like Enter')
        # A multi-line definition keeps its internal movement: Down walks to line two.
        self.load('before\n#let dbl(x) = $#x + \\\n  1$\nafter $dbl(a)$')
        window=self.window
        block=next(f for f in window.editor.object_data.values() if f.get('definition_block'))
        window.activate(block['start']);draft=window.definition_draft.source
        self.assertGreater(draft.document().blockCount(),1,'fixture must be multi-line')
        APPLICATION.sendEvent(draft,QKeyEvent(6,Qt.Key_Up,Qt.NoModifier))
        self.assertIsNone(window.definition_draft)
        window.activate(block['start']);draft=window.definition_draft.source
        APPLICATION.sendEvent(draft,QKeyEvent(6,Qt.Key_Down,Qt.NoModifier))
        self.assertIsNotNone(window.definition_draft,'Down has a line to move to, so it must stay')
        self.assertEqual(draft.textCursor().blockNumber(),1)
        window.cancel_definitions()

    def test_definition_draft_leaves_when_focus_goes_elsewhere(self):
        """Clicking outside the box is the general exit, and it commits like Enter.

        The draft is a floating widget over the document, not a mode: nothing about it
        says the caret may not go elsewhere. Walking away is a person saying "this is what
        I meant", so the edit is kept rather than silently dropped.
        """
        source='before\n#let dbl(x) = $#x + 1$\nafter $dbl(a)$'
        from PyQt5.QtTest import QTest
        self.load(source);window=self.window;window.show();APPLICATION.processEvents()
        block=next(f for f in window.editor.object_data.values() if f.get('definition_block'))
        window.activate(block['start']);draft=window.definition_draft.source
        draft.setPlainText('#let dbl(x) = $#x - 1$')
        APPLICATION.processEvents()
        # Moving focus to the editor is what a click in the document does.
        window.editor.setFocus();APPLICATION.processEvents()
        QTest.qWait(20);APPLICATION.processEvents()
        self.assertIsNone(window.definition_draft,'focus moving away must close the box')
        self.assertIn('#x - 1',window.source,'leaving by clicking keeps the edit')
        window.hide()

    def test_a_string_box_is_left_by_arrows_back_to_its_parent(self):
        """The `"..."` box hands the caret back to the formula, and only a root arrow exits.

        Leaving the box is *not* leaving the formula: the caret returns to the level above
        the string, which is an ordinary position inside the formula. The formula session
        stays open -- what a further arrow at the very root does is the pre-existing
        rule, and it must not have moved.
        """
        self.load('Before $x$ After');window=self.window
        window.activate(window.analysis['formulas'][0]['start'])
        window.math_action('input',text='"');window.math_action('input',text='ab')
        self.assertTrue(window.math_state['string_mode'],'quote enters the text box')
        window.math_action('key',key='ArrowLeft');window.math_action('key',key='ArrowLeft')
        self.assertTrue(window.math_state['string_mode'],'walking to the string head stays in the box')
        window.math_action('key',key='ArrowLeft')
        self.assertFalse(window.math_state['string_mode'],'one more Left leaves the box')
        self.assertIsNotNone(window.math_state,'leaving the box returns to the formula, not out of it')
        self.assertEqual(window.math_state['cursor']['slices'],[])
        self.assertEqual(window.source,'Before $"ab" x$ After','the string itself is untouched')
        window.math_action('key',key='ArrowLeft')
        self.assertIsNone(window.math_state,'an arrow already at the formula root leaves the formula')

    def test_formula_session_edits_authoritative_source(self):
        self.load("Before $a/b$ after")
        window=self.window;formula=window.analysis['formulas'][0];before=window.source
        window.activate(formula['start']);window.math_action('input',text='z')
        self.assertNotEqual(window.source,before)
        self.assertTrue(window.source.startswith('Before $'))
        self.assertTrue(window.source.endswith('$ after'))
        self.assertGreater(len(window.math_canvas.box.stops),0)
        window.finish_formula();window.undo();self.assertEqual(window.source,before)

    def test_split_views_share_source_and_selections_copy_source(self):
        self.load("$a$ + $b$");window=self.window;window.split()
        window.replace(0,0,"中文")
        self.assertEqual(window.editors[0].toPlainText(),window.editors[1].toPlainText())
        for editor in window.editors:
            editor.selectAll();self.assertEqual(editor.createMimeDataFromSelection().text(),window.source)

    def test_typing_dollars_preserves_source_until_leaving(self):
        window=self.window;window.replace(0,0,"$x$",typed=True)
        self.assertEqual(window.editor.toPlainText(),"$x$")
        window.editor.expanded.clear();window.editor.project()
        self.assertEqual(window.editor.toPlainText(),"\ufffc")

    def test_enter_lands_on_the_line_it_just_opened(self):
        """The caret belongs on the new line, not at the next line's first character.

        Enter at the end of `Text one`, with a blank line and a formula after it, must give
        `Text one` + newline + **caret** + the two newlines that were already there. The text
        is the same whichever newline the insertion is recorded at, so the *edit position* is
        ambiguous and only Qt's caret says which was meant.
        """
        from PyQt5.QtTest import QTest
        window=self.window;self.load("Text one\n\n$ a + b $\n\nText two")
        editor=window.editor;window.show();editor.setFocus();APPLICATION.processEvents()
        cursor=editor.textCursor();cursor.setPosition(editor.mapping.display_position(8))
        editor.setTextCursor(cursor);APPLICATION.processEvents()
        QTest.keyClick(editor,Qt.Key_Return);APPLICATION.processEvents()
        self.assertEqual(window.source,"Text one\n\n\n$ a + b $\n\nText two")
        self.assertEqual(editor.toPlainText(),"Text one\n\n\n\ufffc\n\nText two")
        self.assertEqual(editor.expanded,set(),"回车不得展开下一行的公式")
        # The caret is on the line the break opened: right after the newline it inserted,
        # which is source offset 9 -- not 11, where the formula starts.
        self.assertEqual(editor.source_selection(),(9,9))
        self.assertEqual(editor.textCursor().position(),9)
        self.assertEqual(editor.textCursor().blockNumber(),1)

    def test_typing_beside_an_identical_character_keeps_the_caret_next_to_it(self):
        """The same ambiguity without Enter: an insertion inside a run of the same character.

        Typing `x` at the start of `xx` makes `xxx`, and every one of the three positions
        produces that text. The caret must stay after the character just typed.
        """
        from PyQt5.QtTest import QTest
        window=self.window;self.load("xx")
        editor=window.editor;window.show();editor.setFocus();APPLICATION.processEvents()
        cursor=editor.textCursor();cursor.setPosition(0);editor.setTextCursor(cursor)
        QTest.keyClicks(editor,"x");APPLICATION.processEvents()
        self.assertEqual(window.source,"xxx")
        self.assertEqual(editor.textCursor().position(),1,"光标应停在刚输入的字符之后")

    def test_enter_at_the_start_of_a_document_inserts_one_line(self):
        from PyQt5.QtTest import QTest
        window=self.window;self.load("alpha\nbeta")
        editor=window.editor;window.show();editor.setFocus();APPLICATION.processEvents()
        cursor=editor.textCursor();cursor.setPosition(0);editor.setTextCursor(cursor)
        QTest.keyClick(editor,Qt.Key_Return);APPLICATION.processEvents()
        self.assertEqual(window.source,"\nalpha\nbeta")
        self.assertEqual(editor.textCursor().position(),1)

    def test_enter_in_the_source_dock_keeps_both_carets_on_the_new_line(self):
        """The dock is a second entry point for the same edit, with its own caret.

        It captures its caret before applying the edit, and the editor derives its own from
        the edit's position, so the two agree only if the edit is placed where the dock's
        caret is -- not at the end of the run of newlines, where a bare diff puts it.
        """
        from PyQt5.QtTest import QTest
        window=self.window;self.load("Text one\n\n$ a + b $\n\nText two")
        window.show();window.source_dock.show();APPLICATION.processEvents()
        view=window.source_view;view.setFocus();APPLICATION.processEvents()
        cursor=view.textCursor();cursor.setPosition(8);view.setTextCursor(cursor)
        APPLICATION.processEvents()
        QTest.keyClick(view,Qt.Key_Return);APPLICATION.processEvents()
        self.assertEqual(window.source,"Text one\n\n\n$ a + b $\n\nText two")
        self.assertEqual(view.textCursor().position(),9)
        self.assertEqual(window.editor.source_selection(),(9,9),"编辑区光标要跟着源码栏落到新行")

    def test_backspace_inside_blank_lines_stays_where_it_deleted(self):
        """Deleting has the same ambiguity as inserting: which newline of the run went?

        A backspace in a run of blank lines removes *the one before the caret*, and the
        caret belongs where that line was. Taking the diff's answer -- the end of the run --
        moves the caret past the remaining blank lines, to the first character after them.
        """
        from PyQt5.QtTest import QTest
        window=self.window;self.load("$x$\n\n\n\nsome text")
        editor=window.editor;window.show();editor.setFocus();APPLICATION.processEvents()
        # The caret sits between the second and third blank line (source offset 5).
        cursor=editor.textCursor();cursor.setPosition(editor.mapping.display_position(5))
        editor.setTextCursor(cursor);APPLICATION.processEvents()
        self.assertEqual(editor.source_selection(),(5,5))
        QTest.keyClick(editor,Qt.Key_Backspace);APPLICATION.processEvents()
        self.assertEqual(window.source,"$x$\n\n\nsome text")
        self.assertEqual(editor.source_selection(),(4,4),"退格后光标应停在被删掉的那一行上")

    def test_backspace_in_a_plain_text_newline_run(self):
        from PyQt5.QtTest import QTest
        window=self.window;self.load("a\n\n\nb")
        editor=window.editor;window.show();editor.setFocus();APPLICATION.processEvents()
        cursor=editor.textCursor();cursor.setPosition(3);editor.setTextCursor(cursor)
        QTest.keyClick(editor,Qt.Key_Backspace);APPLICATION.processEvents()
        self.assertEqual(window.source,"a\n\nb")
        self.assertEqual(editor.textCursor().position(),2)

    def test_a_formula_kept_as_source_shows_its_reason_on_hover(self):
        """The reason a formula cannot be compiled has to be readable, not just implied.

        A red wave says "this is broken"; the tooltip is the only place the *reason* is
        written. Two things used to swallow it: the editor draws a formula as one object
        character, so the style's source range collapsed to a zero-length selection that
        `mergeCharFormat` writes nothing into, and the source dock is fed plain text, so it
        never received the style at all.
        """
        from PyQt5.QtGui import QTextCursor,QMouseEvent
        from PyQt5.QtCore import QEvent,QPoint
        from .model import u16
        window=self.window;self.load("#let hidden = [inside $x+y$]")
        window.show();window.source_dock.show();APPLICATION.processEvents()
        failure=next(f for f in window.analysis["formulas"] if f.get("editable") is False)
        reason=failure["reason"]
        self.assertTrue(reason)
        style=next(s for s in window.analysis["styles"] if s["kind"]=="formula_error")
        self.assertEqual(style["text"],reason)

        def move_at(point):
            APPLICATION.sendEvent(window.editor.viewport(),
                                  QMouseEvent(QEvent.MouseMove,point,Qt.NoButton,Qt.NoButton,Qt.NoModifier))

        # The source dock shows the formula's own text, so its tooltip is on that text.
        # It is written as a character format, because the dock gets plain text.
        dock_at=u16(window.source[:style["start"]+2])
        cursor=QTextCursor(window.source_view.document());cursor.setPosition(dock_at)
        self.assertEqual(cursor.charFormat().toolTip(),reason,"源码栏的失败公式要带报错原因")
        # The editor draws the whole formula as one object character, whose format cannot
        # carry a tooltip, so the reason becomes the viewport's own tooltip as the pointer
        # moves over it -- which is the tooltip Qt then shows after its hover delay.
        over=QTextCursor(window.editor.document())
        over.setPosition(window.editor.mapping.display_position(style["start"]))
        move_at(window.editor.cursorRect(over).center())
        self.assertEqual(window.editor.viewport().toolTip(),reason,"编辑区悬停公式对象要给出报错原因")
        # A pristine document has no failure to explain, so hovering explains nothing.
        self.load("plain text, nothing broken")
        window.show();APPLICATION.processEvents()
        move_at(QPoint(4,4))
        self.assertEqual(window.editor.viewport().toolTip(),"")

    def test_a_refused_fragment_comes_back_with_the_compilers_own_message(self):
        """End to end through the real adapter: the fragment id *and* why it failed.

        `#let z = 1` needs the semicolon its range excludes, so it cannot compile on its
        own while `cancel(y)` can. The id says which fragment; only the engine's diagnostic
        says why, and it is what the dock has to show.
        """
        source='$ #let z = 1; z + cancel(y) $'
        a=source.index('#let z = 1');b=a+len('#let z = 1')
        c=source.index('cancel(y)');d=c+len('cancel(y)')
        to_bytes=lambda text:len(text.encode('utf-8'))
        body={'path':'main.typ','source':source,'formulas':[],
              'raw':[{'id':'0','start':to_bytes(source[:a]),'end':to_bytes(source[:b]),'occurrence':0},
                     {'id':'1','start':to_bytes(source[:c]),'end':to_bytes(source[:d]),'occurrence':0}]}
        result=self.service('/api/render',body)
        self.assertTrue(result.get('salvaged'),result)
        self.assertIn('0',result['failed'],result)
        self.assertNotIn('1',result['failed'],"能编译的片段不该被报为失败")
        message=(result.get('errors') or {}).get('0')
        self.assertTrue(message,"失败片段必须带回引擎自己的诊断：%s"%result)
        self.assertEqual(len(result['items']),1)
        # The message is the compiler's, about the spliced source -- not the language
        # service's wording, and not a document position.
        self.assertIn('semicolon',message,message)

    def test_the_dock_shows_the_compile_error_of_the_fragment_it_came_from(self):
        from unittest.mock import patch
        window=self.window;self.load('$ cancel(x) + undefined_op(y) $')
        window.compile_timer.stop();window.raw_timer.stop()
        seen=[]
        def request(route,body,callback,key=None,dropped=None):
            if route!='/api/render':callback({},None);return
            ids=[r['id'] for r in body['raw']];seen.append(ids)
            callback({'items':[],'failed':ids,'errors':{ids[0]:'unknown variable: undefined'}},None)
        with patch.object(window.services,'request',side_effect=request):
            window.load_raw();APPLICATION.processEvents();window.raw_timer.stop()
        self.assertTrue(seen,"必须真的问过渲染服务")
        self.assertTrue(window.render_errors,"失败片段的原因要留下")
        window.show();window.source_dock.show();APPLICATION.processEvents()
        shown=window.source_messages.render.body.toPlainText()
        self.assertIn('unknown variable: undefined',shown,shown)
        self.assertTrue(window.source_messages.render.isVisible())
        # The language service said nothing, so its section stays empty and the two are
        # never mixed into one list.
        self.assertEqual(window.source_messages.language.body.toPlainText(),"")
        # A refusal belongs to the revision it was made in, so it takes an edit to retry
        # the fragment -- and the image that comes back ends the error.
        def drawing(route,body,callback,key=None,dropped=None):
            if route!='/api/render':callback({},None);return
            ids=[r['id'] for r in body['raw']]
            callback({'items':[{'id':ids[0],'start':0,'end':1,'svg':'<svg/>','width':1.0,'height':1.0,
                                'baseline':0.0,'base_font_size_pt':1.0,'base_font_height_pt':1.0,
                                'base_font_baseline_pt':1.0}],'failed':[],'errors':{}},None)
        with patch.object(window.services,'request',side_effect=drawing):
            window.replace(0,0," ");window.load_raw();APPLICATION.processEvents();window.raw_timer.stop()
        self.assertEqual(window.render_errors,{},"画出来了就不该再报")

    def test_a_failed_render_is_announced_in_the_bottom_message_bar(self):
        """A box that comes back dashed has to say why without being hovered.

        A hover must be discovered, and the moment this matters most is right after a
        command is confirmed: a box the person just asked for comes back wearing the
        "nothing to lay out here" dress, and the reason is the only thing that tells them
        what to type instead. The bottom bar is where every other failure of this window is
        already said, so the reason is said there too.
        """
        window=self.window;self.load('$ cancel(x) + undefined_op(y) $')
        window.compile_timer.stop();window.raw_timer.stop()
        window.statusBar().clearMessage()
        def request(route,body,callback,key=None,dropped=None):
            if route!='/api/render':callback({},None);return
            ids=[r['id'] for r in body['raw']]
            callback({'items':[],'failed':ids,'errors':{ids[0]:'expected semicolon or line break'}},None)
        with patch.object(window.services,'request',side_effect=request):
            window.load_raw();APPLICATION.processEvents();window.raw_timer.stop()
        shown=window.statusBar().currentMessage()
        self.assertIn('expected semicolon or line break',shown,"渲染失败要出现在底部消息栏：%r"%shown)
        self.assertIn('渲染失败',shown,shown)
        # A batch that draws everything says nothing: the bar is for failures.
        window.statusBar().clearMessage()
        def drawing(route,body,callback,key=None,dropped=None):
            if route!='/api/render':callback({},None);return
            ids=[r['id'] for r in body['raw']]
            callback({'items':[{'id':i,'start':0,'end':1,'svg':'<svg/>','width':9.0,'height':9.0,
                                'baseline':6.0,'base_font_size_pt':9.0,'base_font_height_pt':9.0,
                                'base_font_baseline_pt':6.0} for i in ids],'failed':[],'errors':{}},None)
        with patch.object(window.services,'request',side_effect=drawing):
            window.replace(0,0," ");window.load_raw();APPLICATION.processEvents();window.raw_timer.stop()
        self.assertEqual(window.statusBar().currentMessage(),"","渲染成功不该弹消息")

    def test_a_failed_glyph_request_is_also_announced(self):
        """The other box of the same colour: a font variant whose glyphs never came back.

        It is drawn exactly like a fragment whose image did not come back, so it has to say
        why in the same place rather than only looking broken.
        """
        window=self.window;self.load('$ bold(A) + B $')
        window.compile_timer.stop()
        # The load above already asked (unpatched), so its answer is pending; clear both
        # caches so this pass is the one that asks and fails.
        window.typesetter.glyphs.clear();window.glyph_pending.clear()
        window.statusBar().clearMessage()
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/glyphs':callback({'items':[{'error':'字体替换失败'}]},None)
            else:callback({},None)
        with patch.object(window.services,'request',side_effect=request):
            window.load_glyphs();APPLICATION.processEvents()
        shown=window.statusBar().currentMessage()
        self.assertIn('字体替换失败',shown,"取字形失败要出现在底部消息栏：%r"%shown)

    def test_entering_a_failed_box_and_confirming_asks_the_renderer_again(self):
        """A refusal is scoped to its revision, so opening the box alone asked for nothing.

        Measured before this change: one `/api/render` for the whole round trip -- failing,
        entering the formula, entering the fragment's source, escaping and leaving all
        produced none, because the text had not changed and the fragment was cached as
        refused. Entering the box is how a person says "look at this one again", and
        confirming a draft is how they say what it should be, so both now clear the refusal
        and let the next pass ask.
        """
        from .rawcache import raw_key
        window=self.window;self.load('$ undefinedname $')
        window.compile_timer.stop();window.diagnostic_timer.stop();window.completion_timer.stop()
        asked=[]
        def request(route,body,callback,key=None,dropped=None):
            if route!='/api/render':callback({},None);return
            ids=[r['id'] for r in body['raw']];asked.append(ids)
            callback({'items':[],'failed':ids,
                      'errors':{i:'unknown variable: undefinedname' for i in ids}},None)
        def settle(milliseconds=300):
            """Let the render timer fire; stopping it would hide what is being tested."""
            loop=QEventLoop();QTimer.singleShot(milliseconds,loop.quit);loop.exec_()
        with patch.object(window.services,'request',side_effect=request):
            window.load_raw();settle()
            self.assertEqual(len(asked),1,"第一次失败要问一次")
            node=next(n for n in window.view_nodes(window.analysis['formulas'][0]['view']) if n.get('kind')=='raw')
            self.assertIs(window.typesetter.cache[raw_key(node)],False,"失败要被记下")
            # Entering the box.
            window.statusBar().clearMessage()
            window.activate(0);settle()
            self.assertEqual(len(asked),2,"进入失败框要重新问一次")
            self.assertIs(window.typesetter.cache[raw_key(node)],False,"再问一次还是失败，就再记一次")
            self.assertIn('unknown variable: undefinedname',window.statusBar().currentMessage(),
                          "重新渲染又失败，原因要再说一次")
            # Enter inside the draft, with the text unchanged.
            window.math_action('key',key='ArrowRight');settle()
            self.assertTrue(window.math_state['pending'],"光标要进入片段草稿")
            before=len(asked)
            window.math_action('key',key='Enter');settle()
            self.assertEqual(len(asked),before+1,"回车确认后要重新渲染")
            # Idle passes do not keep asking: exactly one attempt per deliberate act.
            settled=len(asked)
            for _ in range(3):
                window.load_raw();settle(50)
            self.assertEqual(len(asked),settled,"没有新动作就不该反复编译")

    def test_the_dock_reports_a_whole_batch_compile_failure_separately(self):
        from unittest.mock import patch
        window=self.window;self.load('$ cancel(x) + undefined_op(y) $')
        window.compile_timer.stop();window.raw_timer.stop()
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/render':callback(None,'expected expression, found end of file')
            else:callback({},None)
        with patch.object(window.services,'request',side_effect=request):
            window.load_raw();APPLICATION.processEvents();window.raw_timer.stop()
        self.assertEqual(window.render_error,'expected expression, found end of file')
        window.show();window.source_dock.show();APPLICATION.processEvents()
        shown=window.source_messages.render.body.toPlainText()
        self.assertIn('expected expression, found end of file',shown,shown)

    def test_the_dock_reports_language_service_diagnostics_with_their_line(self):
        from unittest.mock import patch
        from .model import u16
        window=self.window;self.load('first line\n$ x + lr(a, size: #100%) $')
        window.compile_timer.stop();window.diagnostic_timer.stop()
        formula=next(f for f in window.analysis['formulas'] if f.get('view'))
        marked=next(n for n in window.view_nodes(formula['view']) if n.get('render_id'))
        start,end=(int(part) for part in marked['render_id'].split(':')[:2])
        prefix=window.source[:start];line=prefix.count('\n');line_start=prefix.rfind('\n')+1
        diagnostic={'range':{'start':{'line':line,'character':u16(window.source[line_start:start])},
                             'end':{'line':line,'character':u16(window.source[line_start:end])}},
                    'message':'unknown variable'}
        with patch.object(window.lsp,'request',side_effect=lambda r,b,cb,key=None:cb({'diagnostics':[diagnostic]},None)):
            window.request_diagnostics()
        window.show();window.source_dock.show();APPLICATION.processEvents()
        shown=window.source_messages.language.body.toPlainText()
        self.assertIn('unknown variable',shown,shown)
        self.assertIn('第 2 行',shown,"语言服务的诊断要带它在文档里的行号：%s"%shown)
        # The layout service failed nothing, so its section is empty: the two engines are
        # shown apart, never as one merged list.
        self.assertEqual(window.source_messages.render.body.toPlainText(),"")
        self.assertTrue(window.source_messages.isVisible())

    def test_hovering_a_refused_fragment_box_says_why_it_could_not_be_drawn(self):
        """The message is attached to the box, not only listed somewhere else.

        A fragment with no image is drawn as a dashed box holding its own source, and that
        box is what the person is looking at, so hovering it is where the reason has to be.
        The fragment that *did* draw stays silent: otherwise the mark would be smeared over
        the whole formula and would not say which part failed.
        """
        from PyQt5.QtGui import QTextCursor
        from PyQt5.QtCore import QPoint,QPointF,QEvent
        from PyQt5.QtGui import QMouseEvent
        window=self.window;self.load('$ cancel(x) + undefined_op(y) $')
        window.compile_timer.stop();window.raw_timer.stop()
        formula=window.analysis['formulas'][0]
        nodes=[n for n in window.view_nodes(formula['view']) if n.get('kind') in ('raw','raw_macro')]
        self.assertEqual(len(nodes),2,nodes)
        refused,drawn=nodes[0],nodes[1]
        message='unknown variable: undefined'
        def source_id(node):return ':'.join(node['render_id'].split(':')[:2])
        def request(route,body,callback,key=None,dropped=None):
            if route!='/api/render':callback({},None);return
            ids=sorted(r['id'] for r in body['raw'])
            self.assertEqual(ids,sorted(source_id(n) for n in nodes),"请求的片段 id 要与视图节点对得上")
            callback({'items':[{'id':source_id(drawn),'start':0,'end':1,'svg':'<svg/>','width':9.0,'height':9.0,
                                'baseline':6.0,'base_font_size_pt':9.0,'base_font_height_pt':9.0,
                                'base_font_baseline_pt':6.0}],
                      'failed':[source_id(refused)],'errors':{source_id(refused):message}},None)
        with patch.object(window.services,'request',side_effect=request):
            window.load_raw();APPLICATION.processEvents();window.raw_timer.stop()
        # The message rides on the node the box is laid out from, and only on that one.
        self.assertEqual(refused.get('_render_error'),message,"消息要附到框所在的节点上")
        self.assertIsNone(drawn.get('_render_error'),"画出来的片段不该被标")
        window.show();APPLICATION.processEvents()
        editor=window.editor;box=editor.handler.box(formula)
        places={node.get('text'):rect for rect,node in box.raws}
        self.assertEqual(len(places),2,places)
        at=next(iter(editor.object_data))
        cursor=QTextCursor(editor.document());cursor.setPosition(at)
        origin=editor.cursorRect(cursor).topLeft()+QPoint(4,3)   # what `drawObject` translates by
        def move(view,target):
            """A mouse move, which is what keeps the widget's tooltip current.

            Qt shows a widget's own `toolTip` after its hover delay, beside the cursor: that
            is the tooltip the person sees, and it is not reachable from a synthetic
            `QEvent::ToolTip` -- an offscreen platform never runs Qt's hover timer at all.
            So what is asserted here is the text the widget carries, which is the input to
            that machinery.
            """
            point=(origin+target).toPoint() if view is editor.viewport() else (QPoint(6,6)+target).toPoint()
            APPLICATION.sendEvent(view,QMouseEvent(QEvent.MouseMove,point,Qt.NoButton,Qt.NoButton,Qt.NoModifier))
        move(editor.viewport(),places[refused.get('text')].center())
        self.assertEqual(editor.viewport().toolTip(),message,"悬停失败片段的框要给出原因")
        move(editor.viewport(),places[drawn.get('text')].center())
        self.assertEqual(editor.viewport().toolTip(),"","画出来的片段不该弹提示")
        # The formula being edited draws the same box, and says the same thing there.
        window.activate(formula['start']);window.raw_timer.stop();APPLICATION.processEvents()
        canvas=window.math_canvas
        canvas_places={node.get('text'):rect for rect,node in canvas.box.raws}
        self.assertIn(refused.get('text'),canvas_places,"编辑框里也要画出这个片段")
        self.assertEqual(next(n for _,n in canvas.box.raws if n.get('text')==refused.get('text')).get('_render_error'),
                         message,"编辑会话的新视图也要盖上这条消息")
        move(canvas,canvas_places[refused.get('text')].center())
        self.assertEqual(canvas.toolTip(),message,"公式编辑框里同一个框也要说明原因")

    def test_svg_metrics_keep_natural_size_and_font_ratio(self):
        settings={"font_size":12,"svg_scale":1,"font_family":"Consolas"}
        raw={'kind':'raw','text':'raw','render_id':'raw'}
        cache={'raw':{'svg':'<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>','base_font_size_pt':3,'base_font_height_pt':1,'base_font_baseline_pt':.8}}
        typesetter=Typesetter(settings,cache);small=typesetter.layout(raw)
        settings['font_size']=24;large=typesetter.layout(raw)
        self.assertAlmostEqual(large.width,small.width*2)
        self.assertAlmostEqual(large.height,small.height*2)

    def test_native_painter_renders_fraction_without_changing_source(self):
        self.load("$ (a+b)/(c+d) $")
        before=self.window.source;view=self.window.analysis['formulas'][0]['view']
        box=self.window.typesetter.layout(view)
        self.assertGreater(box.height,20)
        image=QImage(600,300,QImage.Format_ARGB32);image.fill(Qt.white)
        painter=QPainter(image);self.window.typesetter.paint(painter,box,10,10);painter.end()
        self.assertEqual(self.window.source,before)
        self.assertTrue(any(image.pixelColor(x,y)!=Qt.white for x in range(10,min(200,int(box.width)+10)) for y in range(10,min(200,int(box.height)+10))))

    def test_a_fraction_places_its_slots_by_role_not_by_position(self):
        """Each cell carries the role its parent declared, so order cannot move it.

        Reversing a fraction's children must not swap the numerator and the
        denominator: the layout asks for the child whose role is `numerator`,
        not for the first one. The two slots deliberately have different
        heights -- a nested fraction over a single letter -- because the
        baseline is what exposes a swap, and equal-height slots would hide it.
        """
        self.load("$ frac(frac(a, b), c) $")
        view=self.window.analysis['formulas'][0]['view']
        fraction=next(node for node in self.window.view_nodes(view) if node['kind']=='fraction')
        self.assertEqual([child.get('role') for child in fraction['children']],['numerator','denominator'])
        direct=self.window.typesetter.layout(view)
        fraction['children'].reverse()
        reordered=self.window.typesetter.layout(view)
        self.assertAlmostEqual(direct.width,reordered.width)
        self.assertAlmostEqual(direct.height,reordered.height)
        self.assertAlmostEqual(direct.baseline,reordered.baseline)

    def test_a_number_run_is_a_container_with_a_known_arrangement(self):
        """A run keeps its digits in one cell, so the caret can sit between them.

        One `number` node is one `MathKind::Number`, which is what lets the writer
        spell `12.5` as a single token; the cell inside it is what the caret walks.
        The arrangement must be one this frontend knows, or every formula with a
        plain number would report itself as a frontend/backend mismatch.
        """
        self.load("$12.5$")
        view=self.window.analysis['formulas'][0]['view']
        numbers=[node for node in self.window.view_nodes(view) if node['kind']=='number']
        self.assertEqual(len(numbers),1)
        self.assertEqual(numbers[0]['text'],'')
        cell=numbers[0]['children'][0]
        self.assertEqual(cell['role'],'inner')
        self.assertEqual([child['text'] for child in cell['children'] if child['kind']=='char'],list('12.5'))
        self.window.typesetter.unknown.clear()
        box=self.window.typesetter.layout(view)
        self.assertEqual(self.window.typesetter.unknown,set())
        runs=[value[0] for kind,_,_,value in box.operations if kind=='text']
        self.assertEqual(runs,list('12.5'))

    def test_a_line_draws_its_rule_on_the_side_it_stores(self):
        """`LineItem` holds only the position, so the node says which side it is.

        The arrangement must be one this frontend knows, like `number`, or a formula
        with an overline would report a frontend/backend mismatch; and the two sides
        have to land differently, which is the point of storing a position instead
        of a command name. Both arrive as `decorated`, and the marker says which
        decoration it is -- the wire no longer carries the position in `text`.
        """
        def lay(source):
            self.load(source)
            view=self.window.analysis['formulas'][0]['view']
            lines=[node for node in self.window.view_nodes(view) if node['kind']=='decorated']
            self.window.typesetter.unknown.clear()
            box=self.window.typesetter.layout(view)
            self.assertEqual(self.window.typesetter.unknown,set(),source)
            return lines,box
        self.load("$x$")
        base=self.window.typesetter.layout(self.window.analysis['formulas'][0]['view'])
        # Typst math commands carry no backslash -- `\o` would be an escape -- so
        # the source spells the command the way the writer does.
        lines,above=lay("$overline(x)$")
        self.assertEqual([node['marker'] for node in lines],['overline'])
        lines,below=lay("$underline(x)$")
        self.assertEqual([node['marker'] for node in lines],['underline'])
        # A rule above grows the box upwards and lifts the baseline; a rule below
        # grows it downwards and leaves the baseline where it was.
        self.assertGreater(above.height,base.height)
        self.assertGreater(above.baseline,base.baseline)
        self.assertGreater(below.height,base.height)
        self.assertAlmostEqual(below.baseline,base.baseline)

    def test_the_box_the_caret_is_in_is_marked_at_its_corners(self):
        """The reader has to be able to see which box the caret is editing.

        The marking follows the caret's own slices: `frac(a, b)` with the caret in
        the numerator marks exactly that cell -- not the fraction, not the
        denominator.
        """
        self.load("$frac(a, b) + c$")
        window=self.window;window.compile_timer.stop()
        window.activate(window.analysis['formulas'][0]['start'])
        numerator_stop=next(node for node in window.view_nodes(window.math_state['view'])
                            if node.get('cursor') and node['cursor']['slices']==[{'atom':0,'cell':0}])
        window.math_action('click',cursor=numerator_stop['cursor'])
        view=window.math_state['view']
        fraction=next(node for node in window.view_nodes(view) if node['kind']=='fraction')
        numerator,denominator=fraction['children'][:2]
        # The fraction's own node is marked: the caret's slice chain is
        # root cell -> fraction atom -> numerator cell, so every box on the way is a
        # box the caret is inside. What must *not* be marked is a box the caret is
        # not in -- the denominator is the control here.
        #
        # Two marks, not three: the cell that holds the fraction atom is marked when
        # the marker reaches the atom, and the fraction's box *is* what gets drawn
        # for that cell in the outer box, so they are one mark, not two.
        self.assertEqual(len(self.marked_corners(view)),2,"光标路径上的框各一个")
        self.assertEqual(numerator.get("_active"),True,"分子格被标记")
        self.assertEqual(fraction.get("_active"),True,"分式也在光标路径上")
        self.assertIsNone(denominator.get("_active"),"分母格不该被标记")
        # A move deeper into the formula moves the mark with it.
        window.math_action('key',key='ArrowDown')
        view=window.math_state['view']
        fraction=next(node for node in window.view_nodes(view) if node['kind']=='fraction')
        self.assertEqual(fraction['children'][1].get("_active"),True,"下键之后标记跟着到分母")
        self.assertIsNone(fraction['children'][0].get("_active"))

    def marked_corners(self,view):
        """The boxes whose layout carries a corner mark, as (width, height)."""
        box=self.window.typesetter.layout(view)
        return [value for kind,_,_,value in box.operations if kind=='corners']

    def service(self,route,body,client=None):
        result=[];loop=QEventLoop();timer=QTimer();timer.setSingleShot(True);timer.timeout.connect(loop.quit)
        def receive(value,error):result.append((value,error));loop.quit()
        (client or self.window.services).request(route,body,receive)
        timer.start(45000);loop.exec_();timer.stop()
        self.assertTrue(result,"native service timed out")
        self.assertIsNone(result[0][1],result[0][1]);return result[0][0]

    def test_preview_contains_real_source_positions_and_raw_svg(self):
        self.load("= Native test\nBefore $lr(a, size: #100%)$ after.")
        window=self.window;window.completion_timer.stop()
        result=self.service('/api/preview',window.body()|{'preview':True})
        self.assertEqual(len(result['pages']),1)
        self.assertTrue(result['pages'][0]['mapping'])
        for entry in result['pages'][0]['mapping']:
            self.assertLessEqual(entry['start'],len(window.source.encode('utf-8')))
        request=window.analysis['formulas'][0]['render']
        result=self.service('/api/render',window.body()|request)
        self.assertTrue(result['items'][0]['svg'].startswith('<svg'))
        self.assertGreater(result['items'][0]['base_font_size_pt'],0)

    def test_tinymist_semantic_tokens(self):
        status=self.service('/api/status',{},self.window.lsp)
        if not status.get('available'):self.skipTest('Tinymist is not installed')
        self.load('#let value = 12\nHello #value')
        result=self.service('/api/lsp',self.window.body()|{'method':'semanticTokens/full'},self.window.lsp)
        self.assertTrue(result['legend']['tokenTypes'])
        self.assertTrue(result['result']['data'])

    def test_preview_protocol_colors_and_multipage_svg_export(self):
        from unittest.mock import patch
        import xml.etree.ElementTree as ET
        self.load('#set text(fill: red)\nHello\n#pagebreak()\nWorld')
        result=self.service('/api/preview',self.window.body()|{'preview':True})
        self.assertEqual(len(result['pages']),2)
        self.assertTrue(all(item['color'] for page in result['pages'] for item in page['mapping']))
        with TemporaryDirectory() as directory:
            for kind in ('svg',):
                path=Path(directory)/('export.'+kind)
                with patch('desktop.window.QFileDialog.getSaveFileName',return_value=(str(path),'')),patch.object(self.window,'compile',side_effect=lambda callback:callback(result,None)),patch('desktop.window.QMessageBox.warning') as warning:
                    self.window.export(kind)
                    warning.assert_not_called()
                self.assertTrue(path.exists())
                root=ET.parse(path).getroot();self.assertEqual(len(root),2)
                ids=[node.attrib['id'] for node in root.iter() if 'id' in node.attrib]
                self.assertEqual(len(ids),len(set(ids)))

    def test_unchanged_preview_pages_reuse_qt_widgets(self):
        window=self.window;page={'svg':'<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>','width':10,'height':10,'mapping':[]}
        replies=[{'pages':[page,page]},{'pages':[page,page]}]
        def request(route,body,callback,key=None,dropped=None):callback(replies.pop(0),None)
        with patch.object(window.services,'request',side_effect=request):
            window.compile();first=list(window.pages);window.compile()
        self.assertEqual(window.pages,first)

    def test_shift_selection_and_arrows_enter_formula(self):
        from PyQt5.QtTest import QTest
        self.load('A $x$ Z');editor=self.window.editor
        editor.project((2,2));QTest.keyClick(editor,Qt.Key_Right,Qt.ShiftModifier)
        self.assertEqual(editor.createMimeDataFromSelection().text(),'$x$')
        self.assertIsNone(self.window.math_state)
        editor.project((2,2));QTest.keyClick(editor,Qt.Key_Right)
        self.assertIsNotNone(self.window.math_state)

    def test_native_keyboard_typing_and_backspace_are_lossless(self):
        from PyQt5.QtTest import QTest
        editor=self.window.editor
        QTest.keyClicks(editor,'Hello $x$!')
        self.assertEqual(self.window.source,'Hello $x$!')
        QTest.keyClick(editor,Qt.Key_Backspace)
        self.assertEqual(self.window.source,'Hello $x$')
        editor.expanded.clear();editor.project((9,9))
        QTest.keyClick(editor,Qt.Key_Backspace)
        self.assertEqual(self.window.source,'Hello ')

    def test_big_formula_gets_scrollbars_without_scaling_glyphs(self):
        self.load('$'+'+'.join(['x_1^2']*80)+'$')
        self.window.activate(0)
        natural=self.window.math_canvas.width()
        self.assertGreater(natural,self.window.math_scroll.width())
        self.assertLessEqual(self.window.math_scroll.width(),self.window.editor.viewport().width())
        before=self.window.math_canvas.box.height
        self.window.resize(800,600);self.window.reposition_math()
        self.assertEqual(self.window.math_canvas.width(),natural)
        self.assertEqual(self.window.math_canvas.box.height,before)

    def test_pending_formula_draft_is_not_discarded_by_source_edit(self):
        self.load('$x$');self.window.activate(0);self.window.math_action('input',text='\\')
        self.assertTrue(self.window.math_state['pending']);before=self.window.source
        self.window.replace(0,len(before),'replacement')
        self.assertEqual(self.window.source,before)
        self.window.math_action('key',key='Escape')

    def test_a_macro_fragment_is_rendered_from_its_call_site_like_any_other(self):
        """The ordinary render batch covers a fragment inside a macro template.

        The fragment's own text lives in the definition, so that is the range the
        batch asks for; the document's call to the macro is what compiles it.

        A definition body is **source**, not a formula, so the fragment reaches the
        batch through the *call site's* view -- which carries it together with the
        range in the definition it came from (`view::Projector` writes
        `definitions`/`origin`/`source_range` for exactly that).
        """
        fragment='lr(a, size: #100%)'
        self.load(f'#let fixed(x) = $#x + {fragment}$\n$fixed(y)$')
        window=self.window;window.compile_timer.stop()
        before=window.source;history=len(window.history)
        call=next(formula for formula in window.analysis['formulas'] if formula.get('view'))
        calls,request=self.fake_render()
        with patch.object(window,'visible_formula_starts',return_value={call['start']}), \
             patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_raw()
        self.assertEqual(len(calls),1)
        # The range is derived from the fragment rather than written out: an opaque
        # fixture of a different length silently changed what this asserted before.
        definition=window.source.index(fragment)
        end=definition+len(fragment)
        self.assertIn({'id':f'{definition}:{end}','start':definition,'end':end},calls[0]['raw'])
        node=next(node for node in window.view_nodes(call['view']) if node['kind']=='raw')
        self.assertIsInstance(window.typesetter.raw(node),dict,'the batch result is what this fragment draws')
        self.assertEqual(window.preview_revision,-1,'an image request must not touch the preview')
        self.assertEqual(window.source,before);self.assertEqual(len(window.history),history,'asking for an image never edits the document')

    def test_a_definition_fragment_keeps_the_call_that_renders_it(self):
        """Typst typesets a definition's fragment where the macro is called, so the
        context cut may not drop a call that is later in the document.

        The request is made from the call site -- that is the only formula the fragment
        appears in -- but its **range** lies in the definition, and that is what makes
        the whole document the compiled context.
        """
        self.load('#let fixed(x) = $#x + lr(a, size: #100%)$\n\n$ fixed(y) $')
        window=self.window;window.compile_timer.stop();window.raw_timer.stop()
        call=next(formula for formula in window.analysis['formulas'] if formula.get('view'))
        calls,request=self.fake_render()
        with patch.object(window,'visible_formula_starts',return_value={call['start']}), \
             patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_raw()
        self.assertEqual(len(calls),1)
        self.assertEqual(calls[0]['context_end'],len(window.source.encode('utf-8')),'the call must stay inside the compiled source')
        node=next(node for node in window.view_nodes(call['view']) if node['kind']=='raw')
        self.assertIsInstance(window.typesetter.raw(node),dict)

    def test_typst_controls_native_limit_placement(self):
        self.load('$ sum_1^2 $')
        view=self.window.analysis['formulas'][0]['view']
        script=next(node for node in self.window.view_nodes(view) if node['kind']=='scripts')
        result=self.service('/api/attachments',{'path':'untitled.typ','expression':script['attachment'],'definitions':'','display':True})
        self.assertEqual(result['upper'],'limits')
        key=('',script['attachment'],True);self.window.typesetter.placements[key]=result
        self.window.prepare_view(view,'',True)
        self.assertEqual(script['_placement']['lower'],'limits')

    def fake_render(self):
        calls=[]
        def request(route,body,callback,key=None,dropped=None):
            # The real `Services.request` coalesces queued requests by key and tells a
            # caller that passed `dropped` when its request was replaced. This fake
            # answers every request at once, so nothing is ever queued or replaced.
            if route=='/api/render':
                calls.append(body)
                callback({'items':[{'id':r['id']+':0:0','svg':'<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>','base_font_size_pt':1,'base_font_height_pt':1,'base_font_baseline_pt':.8} for r in body['raw']]},None)
            elif route=='/api/glyphs':
                calls.append(body)
                answers={'bold(A)':'\U0001D468','upright(A)':'A','bold(upright(a))':'\U0001D41A',
                         'bold(x)':'\U0001D499','upright(x)':'x','bold(upright(x))':'\U0001D431',
                         'bold(a)':'\U0001D482'}
                items=[{'glyphs':answers.get(query['expression'],'')} for query in body['expressions']]
                callback({'items':items},None)
            elif route=='/api/preview':callback({'pages':[]},None)
            else:callback({},None)
        return calls,request

    def test_a_font_variant_draws_the_glyphs_the_engine_substituted(self):
        """`bold(A)` has two drawings, and its glyphs come from the analysis.

        Typst applies a variant by *substituting codepoints* through a table the kernel
        cannot reach, so the kernel puts the **spellings to ask about** in the analysis and
        the window turns them into glyphs. Nothing here is asynchronous any more: a `style`
        node always has its glyphs by the time it is drawn, because the kernel only builds
        one for a body that has a glyph run (`has_glyph_run`) — a body that has none is
        drawn as the call instead, which `test_a_variant_without_a_glyph_run_*` pins.
        """
        window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):self.load('$bold(A)$')
        style=next(n for n in window.view_nodes(window.analysis['formulas'][0]['view']) if n['kind']=='style')
        self.assertEqual(style['style_name'],'bold')
        self.assertEqual(style['text'],'bold(A)','分析里带的是要问引擎的拼写')
        self.assertIn('bold(A)',window.analysis['glyphs'],'分析列出了要取的字形簇拼写')
        # The glyphs are asked for by `load` itself (see
        # `test_loading_a_document_renders_its_variants_without_help`), so this only checks
        # that the answer reached the node and the drawing.
        calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_glyphs();window.load_raw()
        # Laid out the way the page lays it out, because that is where a view is stamped:
        # `FormulaObject.box` reads the caches as they stand, so a test that calls the
        # typesetter directly would be testing a path the window does not have.
        box=window.editor.handler.box(window.analysis['formulas'][0])
        style=next(n for n in window.view_nodes(window.analysis['formulas'][0]['view']) if n['kind']=='style')
        self.assertEqual(style.get('_glyph'),'\U0001D468','字形簇盖上节点')
        drawn=[value[0] for kind,_,_,value in box.operations if kind=='text']
        self.assertIn('\U0001D468',drawn,'画的是引擎替换后的字形簇')
        self.assertFalse(any(op[0]=='svg' for op in box.operations),'样式不取图')
        # Entering the node shows the variant's *name* and the body: a variant around one
        # glyph looks exactly like the glyph, so editing one would be editing something
        # invisible.
        window.activate(window.analysis['formulas'][0]['start'])
        window.math_action('key',key='ArrowRight')
        drawn=[value[0] for kind,_,_,value in window.math_canvas.box.operations if kind=='text']
        self.assertIn('bold(',drawn,'进入后显示样式名，否则不知道在编辑什么')

    def test_upright_draws_the_plain_letter_the_engine_asked_for(self):
        """A substituted run is drawn **as the engine spelled it**, not through the editor's
        own letter mapping.

        The editor draws a math variable as an italic letter by mapping `A` to `𝐴`, because
        that is Typst's default. A font variant is a *substitution the engine already made*,
        so applying that mapping again undoes it exactly: `upright(A)` comes back from
        `/api/glyphs` as a plain `A` — u pright is the answer — and the mapping put the
        italic one back. `bold(A)` hid the bug because `𝐀` is not ASCII.
        """
        window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            self.load('$upright(A) + bold(A)$')
            window.load_glyphs();window.load_raw()
        box=window.editor.handler.box(window.analysis['formulas'][0])
        drawn=[value[0] for kind,_,_,value in box.operations if kind=='text']
        self.assertIn('A',drawn,f'引擎说的直体要原样画出来：{drawn}')
        self.assertIn('\U0001D468',drawn,f'加粗仍然画 𝐀：{drawn}')
        self.assertNotIn('\U0001D434',drawn,f'不能把直体的 A 又映射回数学斜体：{drawn}')

    def test_loading_a_document_renders_its_variants_without_help(self):
        """The whole path, driven the way the window drives it: `replace` -> `load` -> paint.

        This is the case that was broken and that the hand-called tests missed: the glyphs
        are requested when the document is loaded, the answer arrives *later*, and the node
        draws `_glyph`, which `stamp_formula` reads out of the cache when the view is laid
        out. Nothing runs between the answer arriving and the paint other than the repaint
        itself, so the answer callback has nothing to stamp: what the reader sees is whatever
        the caches hold at layout time.
        """
        from unittest.mock import patch
        window=self.window
        calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):
            window.replace(0,len(window.source),'$bold(x)$')
            window.compile_timer.stop()
            # Nothing else: no `load_glyphs`, no `load_raw`. The window must do it all.
            window.load_raw()
        self.assertIn('bold(x)',[query['expression'] for body in calls for query in body.get('expressions',[])],'加载后自己去取字形簇')
        box=window.editor.handler.box(window.analysis['formulas'][0])
        style=next(n for n in window.view_nodes(window.analysis['formulas'][0]['view']) if n['kind']=='style')
        self.assertEqual(style.get('_glyph'),'\U0001D499','取到的字形簇要盖上节点')
        drawn=[value[0] for kind,_,_,value in box.operations if kind=='text']
        self.assertIn('\U0001D499',drawn,'画出来的是替换后的字形簇，不是空白')

    def test_a_variant_renders_inside_the_formula_box_too(self):
        """The box and the page are **two different views**, and both have to be stamped.

        The page draws the view `analyze` projected; the box draws the one `activate_formula`
        returns. They are separate objects, so stamping only the analysis renders a variant
        on the page and leaves it blank the moment the reader enters the formula — which is
        exactly the complaint this pins. The drawing is the same on both sides, so both need
        the glyphs.
        """
        from unittest.mock import patch
        window=self.window;calls,request=self.fake_render()
        # The glyphs are asked for by `load` itself, so the service has to be faked before
        # it runs — otherwise the request goes to a service that is not up and the cache
        # keeps a `None` (asked, nothing came back) for the rest of the test.
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.replace(0,len(window.source),'$x + bold(x)$')
            window.compile_timer.stop()
            window.activate(window.analysis['formulas'][0]['start'])
            # No key is pressed: entering the box must already draw the variant. The box
            # gets its own view from `activate_formula`, so *this* call is what fills it.
            box=window.math_canvas.box
            drawn=[value[0] for kind,_,_,value in box.operations if kind=='text']
            self.assertIn('\U0001D499',drawn,f'进入公式框就应当画出替换后的字形簇：{drawn}')
            self.assertEqual(sum(1 for op in box.operations if op[0]=='svg'),0,'公式框里不该出现取图')
            window.math_action('key',key='End')
        drawn=[value[0] for kind,_,_,value in window.math_canvas.box.operations if kind=='text']
        self.assertIn('\U0001D499',drawn,f'移动光标后仍然画出字形簇：{drawn}')

    def test_committing_a_variant_command_draws_it_once_the_answer_arrives(self):
        """Typing `bold(x)` in the box and confirming it must render the variant.

        The glyphs of a spelling are asked for the moment it appears, and the answer comes
        back **later** — the same as a fragment's image. Confirming the command changes the
        source, so the spelling is new at that instant and the box is laid out before the
        answer is in: whatever draws the answer has to reach the box's own view, which is a
        different object from the page's.
        """
        window=self.window;asked=[]
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/glyphs':asked.append((body,callback))
            else:callback({},None)
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.replace(0,len(window.source),'$x$')
            window.compile_timer.stop()
            window.activate(window.analysis['formulas'][0]['start'])
            window.math_action('input',text='\\')
            window.math_action('input',text='bold(x)')
            window.math_action('key',key='Enter')
            self.assertIn('bold(x)',[query['expression'] for body,_ in asked for query in body['expressions']],
                          '确认命令后要问引擎取字形簇')
            # The service answers out of band, so the fake one must too: answering
            # inline would drive a path the window never takes.
            for body,callback in asked:
                callback({'items':[{'glyphs':'\U0001D499' if query['expression']=='bold(x)' else ''} for query in body['expressions']]},None)
        drawn=[value[0] for kind,_,_,value in window.math_canvas.box.operations if kind=='text']
        self.assertIn('\U0001D499',drawn,f'答案到达后公式框要画出替换后的字形簇：{drawn}')

    def test_the_page_renders_a_variant_whose_glyphs_arrive_late(self):
        """The page's own view, stamped at layout time, is the half the box does not cover.

        Both draws stamp the view they are about to lay out (`FormulaObject.box` for the
        page, `MathCanvas.refresh` for the box). The page's view is not rebuilt when an
        answer lands -- only the caches change -- so the stamp has to be read then, and the
        memoized box is dropped by `typesetter.version`. A window that stamped a view when
        it was built draws the *call* here instead of the variant.
        """
        window=self.window;asked=[]
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/glyphs':asked.append((body,callback))
            else:callback({},None)
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.replace(0,len(window.source),'$x + bold(x)$')
            window.compile_timer.stop()
            self.assertIn('bold(x)',[query['expression'] for body,_ in asked for query in body['expressions']],'新拼写要问引擎')
            # Out of band, like the real service: answering inline would test the order the
            # fake happens to have, not the one the window has.
            for body,callback in asked:callback({'items':[{'glyphs':'\U0001D499'} for _ in body['expressions']]},None)
            box=window.editor.handler.box(window.analysis['formulas'][0])
        drawn=[value[0] for kind,_,_,value in box.operations if kind=='text']
        self.assertIn('\U0001D499',drawn,f'晚到的答案要画在页面上：{drawn}')

    def test_a_variant_without_a_glyph_run_is_drawn_as_the_call(self):
        """A body that is not a run of characters makes the call a `raw_macro`.

        `bold(frac(a, b))` and `bold(hat(a))` have no single glyph run — measured, the
        engine refuses both — so they take the path that already works (one image, or the
        name and its slots once the caret enters) instead of a variant that would have to
        report "no glyphs" every time it is drawn.
        """
        for source in ['$bold(frac(a, b))$','$bold(hat(a))$','$upright(a/b)$']:
            self.load(source);window=self.window
            view=window.analysis['formulas'][0]['view']
            self.assertFalse(any(n['kind']=='style' for n in window.view_nodes(view)),
                             f'{source} 的结构化主体不该建样式节点')
            self.assertTrue(any(n['kind']=='raw_macro' for n in window.view_nodes(view)),
                            f'{source} 应当改画成调用')
            self.assertEqual(window.analysis['glyphs'],[],f'{source} 没有要取的字形簇')

    def test_every_variant_layer_gets_its_own_glyphs(self):
        """Each layer is asked for its own glyph run, and **no** layer is asked for an image.

        The engine collapses `bold(upright(x))` to one glyph `𝐱`, but each layer has a glyph
        run of its own (`upright(x)` is the upright `x`), and the reader needs it: stepping
        into `bold` would otherwise show the *italic* `x` of the bare cell, which is not what
        that layer means.

        No images at all is the part that matters here. When `style` was image-drawn as well,
        the nested pair produced the ranges `(1,17)` and `(6,16)` — the inner *inside* the
        outer — which the adapter rejects as overlapping and which cost the whole batch its
        images. A variant over a glyph run never needs an image, so the conflict is gone
        rather than special-cased.
        """
        from unittest.mock import patch
        self.load('$bold(upright(x))$');window=self.window
        self.assertEqual(sorted(window.analysis['glyphs']),['bold(upright(x))','upright(x)'],
                         '两层各要自己的字形簇')
        calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_raw()
        raws=[item for body in calls if 'raw' in body for item in body['raw']]
        self.assertEqual(raws,[],f'样式一律不取图：{raws}')

    def test_entering_a_nested_variant_shows_each_name(self):
        """Inside `bold(upright(a))` the reader has to see *both* names, not just the body.

        One variant around a single glyph is drawn identically to the glyph, so entering
        it would otherwise mean editing something invisible — and with a nested pair there
        is nothing on screen that says which levels are in play.
        """
        self.load('$bold(upright(a))$');window=self.window
        window.activate(window.analysis['formulas'][0]['start'])
        window.math_action('key',key='ArrowRight')
        outer=[value[0] for kind,_,_,value in window.math_canvas.box.operations if kind=='text' and 'bold(' in str(value[0])]
        self.assertTrue(outer,'进入外层后要显示 bold(')
        # Step into the inner variant: it names itself too.
        window.math_action('key',key='ArrowRight')
        inner=[value[0] for kind,_,_,value in window.math_canvas.box.operations if kind=='text' and 'upright(' in str(value[0])]
        self.assertTrue(inner,'进入内层后要显示 upright(')

    def test_a_collapsed_call_is_drawn_as_the_document_has_it_until_the_caret_enters(self):
        """A call the kernel will not expand is one image -- until the caret goes in.

        `raw_macro` has two drawings. Outside the node the document is what the engine
        typesets the call to, so the call's own source is asked for as a fragment and
        drawn; inside, the name and its argument slots are laid out so the arguments can
        be edited. Both come from the same view: the children are always there, and only
        the inside view uses them.
        """
        defs="#let layer0(x) = $#x$"+"".join(f"\n#let layer{i}(x) = $layer{i-1}(#x) + layer{i-1}(#x)$" for i in range(1,16))
        self.load(defs+"\n$layer15(a)$");window=self.window
        calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_raw()
        formula=window.analysis['formulas'][-1]
        collapsed=next(n for n in window.view_nodes(formula['view']) if n['kind']=='raw_macro')
        self.assertEqual(collapsed['text'],'layer15(a)','片段要的是调用本身，不是一句文案')
        start,end=(int(part) for part in collapsed['render_id'].split(':')[:2])
        self.assertIn({'id':f'{start}:{end}','start':start,'end':end},calls[0]['raw'],
                      '调用本身被当成一个片段取图')
        self.assertIsInstance(window.typesetter.raw(collapsed),dict,'取回来的图就是它的画法')
        # Nothing is asked for from *inside* the call: it is drawn as one image, so its
        # argument slots are not separate fragments until the caret enters it.
        for body in calls:
            for item in body['raw']:
                self.assertFalse(start < item['start'] and item['end'] <= end,
                                 f"调用内部的片段不该单独取图：{item}")
        # Outside the node: one svg, no argument cells.
        outside=window.typesetter.layout(formula['view'])
        self.assertTrue(any(op[0]=='svg' for op in outside.operations),'光标在外时画成一张图')
        # Inside it: the name and the slots, and no image.
        window.activate(formula['start'])
        window.math_action('key',key='ArrowRight')
        inside=[op for op in window.math_canvas.box.operations if op[0]=='svg']
        self.assertEqual(inside,[],'光标进入后改用名字与参数槽')

    def test_raw_keeps_svg_when_adjacent_text_moves_its_source_range(self):
        """A fragment that is not in an attachment renders from its own source alone.

        One image answers for it wherever it stands, so editing a sibling leaves it
        alone -- see `test_a_base_that_an_attachment_stretches_is_asked_for_again`
        for the fragments that do follow a sibling.
        """
        from unittest.mock import patch
        self.load('$lr(a, size: #100%)$');window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_raw();self.assertEqual(len(calls),1)
            raw=next(n for n in window.view_nodes(window.analysis['formulas'][0]['view']) if n['kind']=='raw')
            key=raw['_raw_key'];svg=window.typesetter.raw(raw)
            window.activate(0);window.math_action('input',text='z')
            window.background();window.load_raw()
            after=next(n for n in window.view_nodes(window.math_state['view']) if n['kind']=='raw')
            self.assertEqual(after['_raw_key'],key)
            self.assertIs(window.typesetter.raw(after),svg)
            self.assertEqual(len(calls),1,'editing a sibling must not request Raw SVG again')

    def test_a_base_that_an_attachment_stretches_is_asked_for_again(self):
        """`stretch(->)^x` takes its width from `x`, so the base cannot keep the image
        it had under another script -- in another formula, or in this one."""
        from unittest.mock import patch
        self.load('$stretch(->)^x$\n\n$stretch(->)^(1234)$');window=self.window
        calls,request=self.fake_render()
        def fragment(index):
            """The base fragment of one formula, which is drawn from a compiled image.

            It used to be a `raw`; `stretch(->)` has a purely positional argument, so it
            is a `raw_macro` now -- the whole call is the image, and the `->` inside it is
            not drawn separately (see `window.raw_fragments`).
            """
            return next(n for n in window.view_nodes(window.analysis['formulas'][index]['view'])
                        if n['kind'] in ('raw','raw_macro'))
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_raw()
            self.assertEqual(len(calls),1,'both fragments come from one batch compile')
            self.assertEqual(len(calls[0]['raw']),2,'each script asks for its own base')
            first,second=fragment(0),fragment(1)
            self.assertEqual(first['text'],second['text'])
            self.assertNotEqual(first['_context'],second['_context'],'two scripts, two contexts')
            for node in (first,second):
                start,end=(int(part) for part in node['render_id'].split(':')[:2])
                self.assertTrue(window.typesetter.raw(node)['id'].startswith(f'{start}:{end}:'),
                    'each fragment draws the image rendered at its own range')
            # The same formula: widening the script asks for its base again.
            window.activate(window.analysis['formulas'][0]['start'])
            script=next(n for n in window.view_nodes(window.math_state['view']) if n['kind']=='scripts')
            stop=next(n for n in window.view_nodes(script['children'][1]) if n.get('cursor'))
            window.math_action('click',cursor=stop['cursor']);window.math_action('input',text='57')
            calls.clear();window.load_raw()
        asked=[[body['source'].encode('utf-8')[item['start']:item['end']].decode('utf-8') for item in body['raw']] for body in calls]
        self.assertEqual(asked,[['stretch(->)']],
            'the base follows the script it is drawn under')

    def test_script_edits_invalidate_only_base_raw_when_leaving_slot(self):
        from unittest.mock import patch
        self.load('$lr(a, size: #100%)_(1)+lr(b, size: #100%)$');window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):
            window.activate(0)
            raw=[n for n in window.view_nodes(window.math_state['view']) if n['kind']=='raw']
            for n in raw:window.typesetter.cache[n['_raw_key']]={'svg':'<svg/>','base_font_size_pt':1,'base_font_height_pt':1}
            keys=[n['_raw_key'] for n in raw]
            script=next(n for n in window.view_nodes(window.math_state['view']) if n['kind']=='scripts')
            stop=next(n for n in window.view_nodes(script['children'][2]) if n.get('cursor'))
            window.math_action('click',cursor=stop['cursor']);window.math_action('input',text='2')
            self.assertIn(keys[0],window.typesetter.cache)
            window.math_action('click',cursor=window.math_canvas.box.stops[-1][3])
            self.assertNotIn(keys[0],window.typesetter.cache)
            self.assertIn(keys[1],window.typesetter.cache)
            window.load_raw();self.assertEqual(len(calls),1);self.assertEqual(len(calls[0]['raw']),1)

    def test_equal_raw_sources_share_one_render_result(self):
        self.load('$lr(a, size: #100%)$\n$lr(a, size: #100%)$');window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):window.load_raw()
        self.assertEqual(len(calls),1)
        raws=[next(n for n in window.view_nodes(formula['view']) if n['kind']=='raw') for formula in window.analysis['formulas']]
        self.assertIs(window.typesetter.raw(raws[0]),window.typesetter.raw(raws[1]))

    def test_the_experiment_switch_reuses_only_call_free_fragments(self):
        """TYPFORMULA_RAW_CACHE=plain keeps an image only for a fragment without a call.

        The switch asks what dropping the reuse of call-carrying fragments costs,
        so a fragment that already has an image is still drawn from it: the extra
        render work is the only difference. A refusal is not a reused image and
        still waits for the next edit.
        """
        import desktop.rawcache as rawcache
        self.load('$lr(a, size: #100%)$\n$partial + 1$');window=self.window;calls,request=self.fake_render()
        def pass_once():
            with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
                window.load_raw()
        pass_once();pass_once()
        self.assertEqual(len(calls),1,'every fragment is reused by default')
        call=window.source.index('lr(a, size: #100%)');plain=window.source.index('partial')
        starts=[[item['start'] for item in body['raw']] for body in calls]
        self.assertIn(call,starts[0]);self.assertIn(plain,starts[0])
        with patch.object(rawcache,'MODE','plain'):
            self.assertTrue(rawcache.reusable('partial'));self.assertFalse(rawcache.reusable('lr(a, size: #100%)'))
            self.assertFalse(rawcache.reusable('mat(1, 2)'),'a call written as text is one')
            self.assertTrue(rawcache.reusable('#f'),'a parameter reference is not a call')
            pass_once();pass_once()
        self.assertEqual([[item['start'] for item in body['raw']] for body in calls[1:]],
            [[call],[call]],'only the fragment holding a call is asked for again')
        node=next(node for node in window.view_nodes(window.analysis['formulas'][0]['view']) if node['kind']=='raw')
        self.assertEqual(node['text'],'lr(a, size: #100%)')
        self.assertIsInstance(window.typesetter.raw(node),dict,'the image in hand keeps being drawn')
        drawn=[window.typesetter.raw(node) for formula in window.analysis['formulas']
            for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        self.assertTrue(all(isinstance(item,dict) for item in drawn),'no fragment fell back to source text')
        window.typesetter.cache[raw_key(node)]=False
        before=len(calls)
        with patch.object(rawcache,'MODE','plain'):
            pass_once()
        self.assertEqual(len(calls),before,'a refused fragment is not asked for again in the same revision')

    def test_distinct_visible_raws_across_formulas_use_one_batch_compile(self):
        self.load('$lr(a, size: #100%)$\n$lr(b, size: #100%)$\n$lr(c, size: #100%)$');window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):window.load_raw()
        self.assertEqual(len(calls),1)
        self.assertEqual(len(calls[0]['raw']),3)

    def test_raw_svg_is_rasterized_once_then_painted_from_bitmap(self):
        import desktop.mathview as mathview
        settings={"font_size":12,"svg_scale":1,"font_family":"Consolas"}
        svg='<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>'
        box=mathview.Box(20,20,15,[('svg',0,0,(svg,20,20))]);image=QImage(50,50,QImage.Format_ARGB32);image.fill(Qt.white)
        actual=mathview.QSvgRenderer;renders=[]
        class CountingRenderer:
            def __init__(self,data):self.inner=actual(data)
            def render(self,*args):renders.append(1);return self.inner.render(*args)
        typesetter=Typesetter(settings)
        with patch.object(mathview,'QSvgRenderer',CountingRenderer):
            painter=QPainter(image);typesetter.paint(painter,box);typesetter.paint(painter,box);painter.end()
        self.assertEqual(len(renders),1);self.assertEqual(len(typesetter.svg),1)
        self.assertEqual(image.pixelColor(5,5).name(),'#000000')

    def test_a_cached_fragment_is_black_but_its_exported_svg_keeps_its_colour(self):
        """A document that colours its math must not colour the editor's fragments."""
        from PyQt5.QtCore import QRectF
        from .svg import qt_svg
        import desktop.mathview as mathview
        svg='<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#ffffff"/>'
        svg+='<rect width="4" height="4" fill="none"/></svg>'
        cache=mathview.BitmapCache()
        image=QImage(8,8,QImage.Format_ARGB32);image.fill(Qt.white)
        painter=QPainter(image);cache.draw(painter,svg,QRectF(0,0,8,8));painter.end()
        self.assertEqual(image.pixelColor(6,6).name(),'#000000','a white fragment would be invisible on the light page')
        self.assertIn(b'fill="none"',qt_svg(svg,True),'a stroke-only shape must stay unfilled')
        self.assertIn(b'fill="#ffffff"',qt_svg(svg),'export keeps the document colour')

    def test_arrows_exit_only_when_internal_movement_is_exhausted(self):
        for key in ('ArrowLeft','ArrowRight','ArrowUp','ArrowDown'):
            with self.subTest(key=key):
                self.load('Before\n$x$\nAfter');window=self.window;window.activate(to_byte(window.source,7))
                if key=='ArrowRight':window.math_action('click',cursor=window.math_canvas.box.stops[-1][3])
                window.math_action('key',key=key);self.assertIsNone(window.math_state)
        self.load('$a/b$');window.activate(0)
        window.math_action('key',key='ArrowRight');self.assertIsNotNone(window.math_state)
        window.math_action('key',key='ArrowDown');self.assertIsNotNone(window.math_state)

    def test_formula_command_completion_and_mode_backdrops(self):
        self.load('$x$');window=self.window;window.activate(0)
        window.math_action('input',text='\\fr')
        self.assertIn('frac',window.math_state['candidates'])
        self.assertTrue(window.math_popup.isVisible())
        modes=[op[3][2] for op in window.math_canvas.box.operations if op[0]=='mode']
        self.assertIn('command',modes)
        window.math_action('complete',name='frac')
        self.assertTrue(window.math_state['pending'])
        window.math_action('key',key='Enter');self.assertFalse(window.math_state['pending'])
        window.math_action('input',text='"')
        self.assertTrue(window.math_state['string_mode'])
        modes=[op[3][2] for op in window.math_canvas.box.operations if op[0]=='mode']
        self.assertIn('string',modes);self.assertFalse(window.math_popup.isVisible())

    def test_automatic_completion_also_triggers_inside_new_formula_source(self):
        from PyQt5.QtTest import QTest
        editor=self.window.editor;QTest.keyClicks(editor,'$abc$')
        self.assertTrue(editor.expanded)
        editor.project((3,3));QTest.keyClicks(editor,'d')
        self.assertTrue(self.window.completion_timer.isActive())

    def test_line_numbers_follow_variable_formula_line_heights(self):
        self.load('first\n$ (a+b)/(c+d) $\nthird')
        editor=self.window.editor;self.window.show();editor.resize(700,500);APPLICATION.processEvents()
        blocks=[];block=editor.document().begin();layout=editor.document().documentLayout()
        while block.isValid():blocks.append(layout.blockBoundingRect(block).height());block=block.next()
        self.assertEqual(len(blocks),3)
        self.assertGreater(blocks[1],blocks[0]*1.5)
        self.assertEqual([line[0] for line in editor.line_metrics()],[1,2,3])
        self.assertGreaterEqual(editor.line_numbers.width(),editor.line_number_width())

    def test_background_services_do_not_compile_live_preview_or_editor_raw(self):
        """The idle pass keeps attachments and highlights current, and nothing else.

        The live preview is **not** part of it: it runs only while its dock is open, and
        the render pass that would page-compile the document is the export path.
        """
        from unittest.mock import patch
        self.load('$ sum_1^2 lr(a, size: #100%) $');window=self.window;window.raw_timer.stop();calls=[]
        def request(route,body,callback,key=None,dropped=None):
            calls.append(route)
            callback({},None)
        document_revision=window.editor.document().revision()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.background()
        self.assertIn('/api/attachments',calls)
        self.assertNotIn('/api/preview',calls);self.assertNotIn('/api/preview/live',calls)
        self.assertNotIn('/api/render',calls)
        self.assertEqual(window.editor.document().revision(),document_revision)

    def test_the_live_preview_starts_only_when_it_is_switched_on(self):
        """Tinymist serves and renders the preview; switching it off must stop that.

        The preview is a running compiler, so "only render while it is on" is not a
        drawing detail: turning it on asks Tinymist to start one, and turning it off
        kills it. Nothing here renders anything — the test asserts the *requests*, and
        that the page is loaded from the address the reply named.

        The web view is a stand-in, because a real one **cannot be constructed under the
        offscreen platform** (QtWebEngine wants a GL context and the process dies with an
        access violation, not an exception). `Window.preview_widget` exists to be
        replaced for exactly this reason; the start/stop logic under test is the real one.
        """
        from unittest.mock import patch
        window=self.window;asked=[];loaded=[]
        class View:
            def load(self,url):loaded.append(url.toString())
            def setUrl(self,url):loaded.append('blank:'+url.toString())
        window.preview_view=View()
        def request(route,body,callback,key=None,dropped=None):
            asked.append((route,body.get('action')))
            if route=='/api/preview/live' and body.get('action')=='start':
                callback({'staticServerPort':38251,'dataPlanePort':38251,'isPrimary':True},None)
            else:callback({},None)
        with patch.object(window.lsp,'request',side_effect=request):
            self.assertFalse(window.preview_started,'没有开启时不该有预览在跑')
            window.set_preview(True)
        self.assertIn(('/api/preview/live','start'),asked,'开启时才向 Tinymist 要预览')
        self.assertEqual(len(loaded),1,'按回复里的地址加载预览页')
        # The page and its WebSocket share a port, so the reply's address is the whole URL.
        self.assertIn('38251',loaded[0])
        with patch.object(window.lsp,'request',side_effect=request):
            window.set_preview(False)
        self.assertIn(('/api/preview/live','kill'),asked,'关闭时必须停掉 Tinymist 的预览')
        self.assertFalse(window.preview_started,'关闭后不再有预览在跑')
        # Hiding also blanks the view, so a hidden dock cannot keep a live page running.
        self.assertTrue(loaded[-1].startswith('blank:'),f'关闭后页面要清空：{loaded}')

    def test_closing_the_window_stops_a_running_preview(self):
        """A preview left running is a compiler left running, so closing kills it."""
        from unittest.mock import patch
        window=self.window;killed=[]
        class View:
            def load(self,url):pass
            def setUrl(self,url):pass
        window.preview_view=View()
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/preview/live' and body.get('action')=='kill':killed.append(route)
            callback({'staticServerPort':1,'dataPlanePort':1,'isPrimary':True},None)
        with patch.object(window.lsp,'request',side_effect=request):
            window.set_preview(True)
            self.assertTrue(window.preview_started)
            events=type('E',(object,),{'ignore':lambda self:None,'accept':lambda self:None})()
            window.closeEvent(events)
        self.assertTrue(killed,'关窗要停掉预览')

    def test_live_preview_shares_sync_and_restarts_after_language_session_loss(self):
        window=self.window;asked=[];loaded=[];running=[None]
        class View:
            def load(self,url):loaded.append(url.toString())
            def setUrl(self,url):pass
        window.preview_view=View()
        def request(route,body,callback,key=None,dropped=None):
            asked.append((route,body.copy()))
            if route=='/api/preview/live':
                running[0]={'staticServerPort':38251,'dataPlanePort':38251} if body['action']=='start' else None
                callback(running[0] or {},None)
            else:callback({'diagnostics':[],'preview':running[0]},None)
        with patch.object(window.lsp,'request',side_effect=request),patch.object(window.services,'request') as render:
            window.set_preview(True)
            window.replace(0,len(window.source),'Hello changed')
            window.request_diagnostics()
            self.assertEqual(asked[-1][1]['source'],'Hello changed')
            self.assertEqual(len(loaded),1)
            running[0]=None
            window.request_diagnostics()
            self.assertEqual(len(loaded),2)
            self.assertEqual(asked[-1][1]['action'],'start')
            self.assertEqual(asked[-1][1]['source'],'Hello changed')
            self.assertFalse(any(call.args[0]=='/api/preview/live' for call in render.call_args_list))
            window.set_preview(False)

    def test_live_preview_ignores_late_start_and_reopens_for_another_file(self):
        window=self.window;loaded=[];requests=[]
        class View:
            def load(self,url):loaded.append(url.toString())
            def setUrl(self,url):pass
        window.preview_view=View()
        # Use files in the current workspace so both requests share the pipe.
        def request(route,body,callback,key=None,dropped=None):requests.append((route,body.copy(),callback))
        with patch.object(window.lsp,'request',side_effect=request):
            window.set_preview(True);old=requests[-1][2]
            window.set_preview(False)
            old({'staticServerPort':38251,'dataPlanePort':38251},None)
            self.assertEqual(loaded,[])
            window.set_preview(True)
            with patch.object(Path,'open',return_value=__import__('io').StringIO('New document')):
                window.load(window.workspace/'another.typ')
            starts=[body for route,body,_ in requests if route=='/api/preview/live' and body['action']=='start']
            self.assertEqual(starts[-1]['path'],'another.typ')
            self.assertEqual(starts[-1]['source'],'New document')
            window.set_preview(False)

    def test_pdf_button_uses_typst_pdf(self):
        self.assertTrue(hasattr(self.window,'preview_dock'),'实时预览有自己的 dock')
        self.assertFalse(self.window.preview_dock.isVisible(),'实时预览默认关闭')
        self.load('= PDF\n\nHello $x^2$');window=self.window
        result=self.service('/api/pdf',window.body()|{'pdf':True})
        data=base64.b64decode(result['pdf'],validate=True);self.assertTrue(data.startswith(b'%PDF'))
        with TemporaryDirectory() as directory,patch('desktop.window.tempfile.gettempdir',return_value=directory),patch('desktop.window.QDesktopServices.openUrl',return_value=True) as opened:
            with patch.object(window.services,'request',side_effect=lambda route,body,callback,key=None:callback(result,None)):
                window.compile_pdf()
            opened.assert_called_once()
            output=Path(opened.call_args.args[0].toLocalFile());self.assertTrue(output.is_file());self.assertEqual(output.read_bytes(),data)

    def test_distant_formula_projections_survive_typst_incremental_edit(self):
        source=''.join(f'paragraph {i}\n\n$lr(x_{i}, size: #100%)$\n\n' for i in range(80))
        self.load(source);window=self.window
        before=[formula['view'] for formula in window.analysis['formulas']]
        calls=[];original=window.core.call
        def observed(action,**arguments):
            calls.append(action);return original(action,**arguments)
        with patch.object(window.core,'call',side_effect=observed):window.replace(5,5,' updated')
        projected=calls.count('analyze_formula')
        self.assertIn('edit_source',calls);self.assertIn('scan',calls)
        self.assertNotIn('analyze',calls)
        self.assertLessEqual(projected,4,f'{projected} of 80 formulas were rebuilt')
        from .rawcache import signature
        self.assertEqual(signature(window.analysis['formulas'][-1]['view']),signature(before[-1]))

    def test_prose_edits_reuse_nearby_formula_views_and_boxes(self):
        window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):
            self.load('Before $bold(x)$ between $frac(a, b)$ after')
            views=[f['view'] for f in window.analysis['formulas']]
            boxes=[window.editor.handler.box(f) for f in window.analysis['formulas']]
            calls.clear()
            with patch.object(window.core,'call',wraps=window.core.call) as core:
                window.replace(0,6,'中文😀 ')
                window.replace(len(window.source),len(window.source),'!')
            self.assertFalse([c for c in core.call_args_list if c.args[0] in ('analyze','analyze_formula')])
            self.assertFalse([b for b in calls if 'expression' in b])
            for formula,view,box in zip(window.analysis['formulas'],views,boxes):
                self.assertIs(formula['view'],view)
                self.assertIs(window.editor.handler.box(formula),box)
            window.activate(window.analysis['formulas'][0]['start'])
            window.finish_formula()
            self.assertIs(window.editor.handler.box(window.analysis['formulas'][0]),boxes[0])

    def test_only_changed_formula_is_analyzed_in_shared_paragraph(self):
        self.load('Before $a$ between $b$ after');window=self.window
        second=window.analysis['formulas'][1]['view']
        at=window.source.index('$a$')+1
        with patch.object(window.core,'call',wraps=window.core.call) as calls:
            window.replace(at,at+1,'z')
        targets=[c.kwargs['start'] for c in calls.call_args_list if c.args[0]=='analyze_formula']
        self.assertEqual(targets,[window.analysis['formulas'][0]['start']])
        self.assertIs(window.analysis['formulas'][1]['view'],second)
        self.assertIn('z',[n.get('text') for n in window.view_nodes(window.analysis['formulas'][0]['view'])])

    def test_reused_raw_view_source_ranges_follow_unicode_prose_edits(self):
        self.load('Before $unknownfn(a)$ after');window=self.window
        with patch.object(window.core,'call',wraps=window.core.call) as calls:
            window.replace(0,0,'中文😀 ')
        self.assertFalse([c for c in calls.call_args_list if c.args[0]=='analyze_formula'])
        formula=window.analysis['formulas'][0]
        node=next(n for n in window.view_nodes(formula['view']) if n['kind']=='raw_macro')
        a,b=map(int,node['render_id'].split(':')[:2])
        self.assertEqual(window.source.encode()[a:b].decode(),'unknownfn(a)')
        self.assertIn({'id':f'{a}:{b}','start':a,'end':b},formula['render']['raw'])

    def test_scope_delimiter_edits_refresh_cached_macro_bindings(self):
        self.load('#[#let foo(x) = $#x+1$]\n$foo(a)$');window=self.window
        at=window.source.index(']')
        for replacement,kind in (('', 'macro'),(']', 'raw_macro')):
            window.replace(at,at+(not replacement),replacement)
            formula=window.analysis['formulas'][-1]
            kinds=[n['kind'] for n in window.view_nodes(formula['view'])]
            self.assertIn(kind,kinds)
            fresh=window.core.call('analyze_formula',start=formula['start'])
            self.assertEqual(signature(formula['view']),signature(fresh['view']))

    def test_every_variant_in_the_document_is_asked_for_in_one_request(self):
        """One request carries every spelling the analysis mentions, and each answer lands
        on its own spelling.

        Each expression used to be its own adapter process — 50ms of startup for 2ms of
        work — so a document with four variants paid that four times.
        """
        window=self.window;asked=[]
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/glyphs':asked.append((body,callback))
        with patch.object(window.services,'request',side_effect=request):
            self.load('$bold(x) + upright(y) + bold(z)$ 与 $upright(x)$')
            window.load_glyphs()
        self.assertEqual(len(asked),1,'一次请求')
        queries=asked[0][0]['expressions']
        self.assertEqual(sorted(query['expression'] for query in queries),['bold(x)','bold(z)','upright(x)','upright(y)'])
        self.assertTrue(all(query['definitions']=='' for query in queries),'字形与文档上下文无关')
        asked[0][1]({'items':[{'glyphs':f'<{query["expression"]}>'} for query in queries]},None)
        for query in queries:
            self.assertEqual(window.typesetter.glyphs[('',query['expression'],query['display'])],f'<{query["expression"]}>')
        self.assertFalse(window.glyph_pending)

    def test_styles_share_empty_context_cache_and_pending_requests(self):
        window=self.window;asked=[]
        def request(route,body,callback,key=None,dropped=None):
            if route=='/api/glyphs':asked.append((body,callback))
        with patch.object(window.services,'request',side_effect=request):
            self.load('#let value = 1\nBefore $bold(x)$ between $bold(x)$')
            window.load_glyphs();window.load_glyphs()
            self.assertEqual(len(asked),1)
            self.assertEqual([query['definitions'] for query in asked[0][0]['expressions']],[''],'字形不依赖文档上下文')
            window.replace(window.source.index('Before'),window.source.index('Before')+6,'正文 ')
            self.assertEqual(len(asked),1,'正文变化不能重复发送在途字形请求')
            asked[0][1]({'items':[{'glyphs':'\U0001D499'}]},None)
            at=window.source.index('1');window.replace(at,at+1,'2')
            self.assertEqual(len(asked),1,'宏定义变化不清空无定义上下文的字形缓存')
            for formula in window.analysis['formulas']:
                if 'view' not in formula:continue
                window.stamp_formula(formula)
                for node in window.view_nodes(formula['view']):
                    if node['kind']=='style':self.assertEqual(node['_glyph'],'\U0001D499')
            window.activate(window.analysis['formulas'][-1]['start'])
            style=next(n for n in window.view_nodes(window.math_state['view']) if n['kind']=='style')
            self.assertEqual(style['_glyph'],'\U0001D499')

    def test_style_request_ignores_broken_and_unclosed_document_prefixes(self):
        window=self.window
        for source in ('$unknownfn(a)$\n$bold(x)$','#block[$bold(x)$]'):
            asked=[];window.typesetter.glyphs.clear();window.glyph_pending.clear()
            def request(route,body,callback,key=None,dropped=None):
                if route=='/api/glyphs':asked.append((body,callback))
            with patch.object(window.services,'request',side_effect=request):self.load(source)
            self.assertEqual(len(asked),1)
            body,callback=asked[0]
            self.assertEqual([query['definitions'] for query in body['expressions']],[''])
            result=self.service('/api/glyphs',body)
            self.assertEqual(result['items'][0]['glyphs'],'\U0001D499')
            callback(result,None)
            formula=window.analysis['formulas'][-1]
            self.assertIn('\U0001D499',[op[3][0] for op in window.editor.handler.box(formula).operations if op[0]=='text'])

    def test_incremental_edit_preserves_distant_qt_text_blocks_and_formula_ids(self):
        from PyQt5.QtGui import QTextBlockUserData
        source='header\n\n'+('\n\n'.join(f'$x_{i}$' for i in range(40)))+'\n\ntail'
        self.load(source);window=self.window;editor=window.editor
        last=editor.document().lastBlock();marker=QTextBlockUserData();last.setUserData(marker)
        formula_id=window.analysis['formulas'][-1]['_object_id']
        window.replace(1,1,'!')
        self.assertIs(editor.document().lastBlock().userData(),marker)
        self.assertEqual(window.analysis['formulas'][-1]['_object_id'],formula_id)

    def test_editing_one_formula_does_not_project_the_whole_document(self):
        source='\n\n'.join(f'$x_{i}$' for i in range(60));self.load(source);window=self.window
        target=window.analysis['formulas'][30];start=from_byte(source,target['start'])+2
        calls=[];original=window.core.call
        def observed(action,**arguments):calls.append(action);return original(action,**arguments)
        with patch.object(window.core,'call',side_effect=observed):window.replace(start,start,'z')
        self.assertNotIn('analyze',calls)
        self.assertLessEqual(calls.count('analyze_formula'),5)

    def test_formula_box_is_lain_out_once_per_layout_paint_cycle(self):
        self.load('$frac(a, b) + x$');window=self.window
        formula=window.analysis['formulas'][0];handler=window.editor.handler
        first=handler.box(formula)
        self.assertIs(handler.box(formula),first,'paint and click must reuse the layout result')
        window.typesetter.touch()
        second=handler.box(formula)
        self.assertIsNot(second,first,'new Raw output must lay the formula out again')
        self.assertEqual((second.width,second.height),(first.width,first.height))
        before=handler.box(formula)
        window.change_font(1);window.compile_timer.stop()
        self.assertIsNot(handler.box(formula),before,'an editor font change must relayout')

    def test_font_metrics_are_shared_per_size(self):
        typesetter=Typesetter({'font_size':16,'svg_scale':1,'font_family':'Consolas'})
        font,metrics,em,ascent,descent=typesetter.line(1.0)
        self.assertIs(typesetter.line(1.0)[0],font)
        self.assertIs(typesetter.line(1.0)[1],metrics)
        self.assertEqual(em,metrics.height())
        self.assertEqual(ascent,metrics.ascent())
        self.assertGreater(em,0)
        self.assertIsNot(typesetter.line(.7)[1],metrics)

    def test_formula_glyphs_come_from_an_installed_math_font(self):
        from . import mathfont
        mathfont.install()
        typesetter=Typesetter({'font_size':12,'svg_scale':1,'font_family':'Consolas','math_font':'NewComputerModern Math'})
        self.assertEqual(typesetter.family(),'NewComputerModern Math')
        self.assertIn(typesetter.family(),mathfont.families())
        # A variable is the math italic letter of the range Typst typesets with.
        for letter,italic in (('a','𝑎'),('h','ℎ'),('Z','𝑍')):
            glyph,font,width=typesetter.run(letter,1.0)
            self.assertEqual(glyph,italic)
            self.assertEqual(font.family(),'NewComputerModern Math')
            self.assertGreater(width,0)
        # Text cells and symbols stay as written.
        self.assertEqual(typesetter.run('a',1.0,text_mode=True)[0],'a')
        self.assertEqual(typesetter.run('≤',1.0)[0],'≤')

    def test_an_uninstalled_math_font_is_never_handed_to_qt(self):
        """A missing family is substituted in silence; that must not reach layout."""
        from . import mathfont
        typesetter=Typesetter({'font_size':12,'svg_scale':1,'font_family':'Consolas','math_font':'No Such Math Font'})
        self.assertIn(typesetter.family(),mathfont.families())
        # The line box is taken from the editor text font, never from the math font:
        # a math font reports TeX line metrics (an ascent of several em on Windows)
        # because it has to contain four-line delimiters.
        line_font,metrics,em,ascent,descent=typesetter.line(1.0)
        self.assertEqual(line_font.family(),'Consolas')
        self.assertEqual((em,ascent),(metrics.height(),metrics.ascent()))
        self.assertLess(ascent,em)
        self.assertIsNot(metrics,typesetter.font(typesetter.family(),1.0)[1])

    def test_incremental_merge_shares_untouched_views_and_keeps_the_old_analysis(self):
        from .incremental import merge
        def formula(start,text):
            return {'start':start,'end':start+3,'editable':True,
                'view':{'kind':'cell','children':[{'kind':'raw','text':text,'render_id':f'{start}:{start+3}:0:0','source_range':[start,start+3]}]},
                'render':{'source':'old','raw':[{'id':f'{start}:{start+3}:0','start':start,'end':start+3}]}}
        previous={'formulas':[formula(0,'a'),formula(10,'b')],'styles':[]}
        syntax={'formulas':[{'start':0,'end':3,'editable':True},{'start':12,'end':15,'editable':True}],'styles':[]}
        merged,rebuild=merge(previous,syntax,'new',5,5,'zz',{'start':5,'end':5})
        self.assertEqual(rebuild,[])
        first,second=merged['formulas']
        # A formula before the edit cannot move, so its whole view subtree is shared.
        self.assertIs(first['view'],previous['formulas'][0]['view'])
        self.assertEqual(first['render']['source'],'new')
        self.assertEqual(previous['formulas'][0]['render']['source'],'old')
        # A formula after the edit gets new nodes; the previous analysis keeps its own.
        self.assertIsNot(second['view'],previous['formulas'][1]['view'])
        self.assertEqual((second['start'],second['end']),(12,15))
        self.assertEqual(second['view']['children'][0]['render_id'],'12:15:0:0')
        self.assertEqual(second['view']['children'][0]['source_range'],[12,15])
        self.assertEqual(second['render']['raw'][0]['id'],'12:15:0')
        old=previous['formulas'][1]
        self.assertEqual((old['start'],old['end']),(10,13))
        self.assertEqual(old['view']['children'][0]['render_id'],'10:13:0:0')
        self.assertEqual(old['view']['children'][0]['source_range'],[10,13])
        self.assertEqual(old['render']['raw'][0]['id'],'10:13:0')

    def test_the_math_font_covers_every_glyph_the_core_can_draw(self):
        """A missing glyph is drawn from another font, so coverage is a test.

        The letters are taken **through `mathfont.glyph`** rather than written out here, so
        the mapping and the coverage have to agree with each other. There is exactly one
        place where they can disagree, and it is `h`: U+1D455 (mathematical italic small h)
        is **unassigned in Unicode**, so no font has a glyph for it, and the engine typesets
        the italic h as Planck's constant `ℎ` U+210E instead — measured against the real
        adapter for all 52 letters, `h` is the only one that differs from the plain mapping
        (`native-adapter/src/main.rs::the_italic_default_has_one_hole_and_it_is_h`).
        """
        from PyQt5.QtGui import QFont,QRawFont
        from . import mathfont
        family=mathfont.resolve('','Consolas')
        font=QFont(family);font.setStyleStrategy(QFont.NoFontMerging)
        raw=QRawFont.fromFont(font)
        # Letters as the math alphabet (through the mapping itself), operators and Greek
        # as the symbol table writes them, the box glyphs the display tree adds, and the
        # digits the editor puts in text cells.
        needed=[mathfont.glyph(family,chr(code))[1] for code in (*range(0x61,0x7B),*range(0x41,0x5B))]
        needed+=list('0123456789+-=()[]{}|/,.:;!?<>^_')+['ℎ','−','∗','≤','≥','≠','±','∓','×','⋅','÷','𝛼','𝛽','𝛾','𝛿','𝜀','𝜃','𝜆','𝜇','𝜋','𝜌','𝜎','𝜏','𝜑','𝜓','𝜔','Γ','Δ','Θ','Σ','Ω','□','│','⌘','·','‖']
        missing=[ch for ch in needed if raw.glyphIndexesForString(ch)[0]==0]
        self.assertEqual(missing,[],f'{family} cannot draw {missing}')

    def test_a_failed_fragment_is_reported_and_entered_with_a_horizontal_key(self):
        """A Raw without an image has nothing to click, so a key has to open it."""
        window=self.window
        self.load('$ undefinedname $');window.activate(0);window.compile_timer.stop()
        text=next(node['text'] for node in window.view_nodes(window.math_state['view']) if node['kind']=='raw')
        self.assertEqual(text,'undefinedname')
        # What load_raw records when the render pass returns nothing for a fragment.
        node=next(node for node in window.view_nodes(window.math_state['view']) if node['kind']=='raw')
        window.typesetter.cache[raw_key(node)]=False
        window.report_raw_fragments(force=True)
        window.math_action('key',key='ArrowRight')
        state=window.math_state
        self.assertTrue(state['pending'],'the caret must be inside the fragment draft')
        self.assertEqual(state['view']['children'][1]['kind'],'unknown')
        # Escape restores the fragment and closes the draft.
        window.math_action('key',key='Escape')
        self.assertFalse(window.math_state['pending'])
        self.assertEqual(window.source,'$ undefinedname $')

    def test_a_fragment_without_a_source_range_is_marked_as_failed(self):
        window=self.window
        self.load('$ sum_(n=0)^oo a_n $')
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        self.assertTrue(nodes)
        for node in nodes:node.pop('render_id',None)
        window.load_raw()
        for node in nodes:self.assertIs(window.typesetter.cache[raw_key(node)],False)
        box=window.editor.handler.box(window.analysis['formulas'][0])
        marks=[kind for kind,_,_,_ in box.operations]
        self.assertIn('failed',marks,'a fragment with no image must say so where it is drawn')

    def test_a_rendered_fragment_is_not_marked_and_draws_its_image(self):
        window=self.window
        self.load('$ sum_(n=0)^oo a_n $')
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        for node in nodes:
            window.typesetter.cache[raw_key(node)]={'svg':'<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"/>',
                'base_font_size_pt':.5,'base_font_height_pt':.7,'base_font_baseline_pt':.1}
        window.typesetter.touch()
        box=window.editor.handler.box(window.analysis['formulas'][0])
        marks=[kind for kind,_,_,_ in box.operations]
        self.assertIn('svg',marks);self.assertNotIn('failed',marks)

    def test_a_failed_render_request_is_retried_after_the_next_edit(self):
        """One bad compile must not leave a viewport of source text for good."""
        window=self.window
        self.load('$ sum_(n=1)^oo frac(1, n^2) $')
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        self.assertTrue(nodes)
        requests=[]
        def failing(route,body,callback,key=None,dropped=None):
            requests.append((route,body,key))
            if len(requests)==1:callback(None,'渲染后端编译失败')
            else:callback({'items':[]},None)
        with patch.object(window.services,'request',side_effect=failing):
            window.load_raw()
            self.assertEqual(len(requests),1)
            for node in nodes:self.assertIs(window.typesetter.cache[raw_key(node)],False)
            # The same revision does not ask again: the verdict still holds.
            window.load_raw()
            self.assertEqual(len(requests),1)
            # An edit usually fixes the document, and must clear that verdict.
            window.replace(0,0,'正文 ')
            window.load_raw()
            self.assertEqual(len(requests),2)

    def test_a_fragment_the_renderer_refused_is_retried_after_the_next_edit(self):
        """A fragment left out of a salvaged batch is about the text, not the session."""
        window=self.window
        self.load('$ sum_(n=1)^oo frac(1, n^2) $')
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        self.assertTrue(nodes)
        refused=nodes[0];source_id=':'.join(refused['render_id'].split(':')[:2])
        requests=[]
        def salvaged(route,body,callback,key=None,dropped=None):
            requests.append((route,body,key))
            callback({'items':[],'failed':[source_id]},None)
        with patch.object(window.services,'request',side_effect=salvaged):
            window.load_raw()
            self.assertEqual(len(requests),1)
            self.assertIs(window.typesetter.cache[raw_key(refused)],False)
            window.load_raw()
            self.assertEqual(len(requests),1,'the same revision keeps the verdict')
            window.replace(0,0,'正文 ')
            window.load_raw()
            self.assertEqual(len(requests),2,'an edit asks for the refused fragment again')

    def test_a_fragment_followed_by_a_parenthesis_still_gets_its_image(self):
        """`cal(A)(E)`: the spliced fragment must not be read as a call on code."""
        window=self.window
        self.load('正文 $ cal(A)(E) = 0 $ 与 $ dif x $。')
        window.compile_timer.stop();window.raw_timer.stop()
        loop=QEventLoop()
        QTimer.singleShot(200,window.load_raw)
        QTimer.singleShot(3000,loop.quit)
        loop.exec_()
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        self.assertTrue(nodes)
        drawn=[node['text'] for node in nodes if isinstance(window.typesetter.raw(node),dict)]
        self.assertEqual(drawn,[node['text'] for node in nodes],'a fragment followed by `(` must keep its image')

    def test_a_command_draft_is_drawn_in_the_editor_font(self):
        """The draft is Typst source, so it reads like the text around the formula."""
        window=self.window
        self.load('$ $');window.activate(0)
        for character in "\\alph":window.math_action('input',text=character)
        drawn=[value for kind,_,_,value in window.math_canvas.box.operations if kind=='text']
        self.assertTrue(drawn)
        self.assertEqual({font.family() for _,font,_ in drawn},{window.settings['font_family']})
        self.assertNotIn(window.settings['math_font'],{font.family() for _,font,_ in drawn})

    def test_an_empty_slot_is_a_dashed_box(self):
        """`\\frac` + Enter must show where the numerator and denominator go."""
        window=self.window
        self.load('$ $');window.activate(0)
        for character in "\\frac":window.math_action('input',text=character)
        window.math_action('key',key='Enter')
        slots=[value for kind,_,_,value in window.math_canvas.box.operations if kind=='slot']
        self.assertEqual(len(slots),2,'numerator and denominator are both empty')
        self.assertTrue(all(width>3 and height>3 for width,height in slots),slots)
        # The static projection of a formula with empty slots shows them too.
        self.load('$frac("", "")$')
        static=[kind for kind,_,_,_ in window.editor.handler.box(window.analysis['formulas'][0]).operations]
        self.assertEqual(static.count('slot'),2)

    def test_a_display_formula_gets_a_centred_line(self):
        window=self.window
        self.load('正文 $ x^2 $ 后文\n\n$ y^2 $\n')
        alignments=[(block.text(),int(block.blockFormat().alignment())) for block in
                    [window.editor.document().findBlockByNumber(number) for number in range(window.editor.document().blockCount())]]
        self.assertIn(('\ufffc',int(Qt.AlignHCenter)),alignments,'a display formula alone on its line is centred')
        self.assertIn(('正文 \ufffc 后文',int(Qt.AlignLeft)),alignments,'sharing a line with text keeps it left aligned')

    def test_side_scripts_follow_the_shifts_the_compiler_uses(self):
        """A subscript sits .25 em below the base's baseline, a superscript .36 em above."""
        window=self.window
        metrics=window.typesetter.line(1.0)[1]
        em=window.typesetter.style_em(1.0,metrics)
        for formula,sign in [('$x_1$',1),('$x^2$',-1)]:
            self.load(formula);window.activate(0)
            runs=sorted(((x,y,value[0]) for kind,x,y,value in window.math_canvas.box.operations if kind=='text'))
            self.assertEqual(len(runs),2,'one base and one script')
            offset=sign*(runs[-1][1]-runs[0][1])
            expected=em*(.25 if sign>0 else .36)
            self.assertAlmostEqual(offset,expected,delta=1.0,
                                   msg='%s: %.2f px, wanted %.2f' % (formula,offset,expected))
        self.load('$ $')

    def test_an_edit_keeps_the_view_where_the_reader_scrolled_it(self):
        """A re-projection must not read the dock's caret back into the editor.

        The dock's caret stays near the top while the editor is scrolled far down.
        Setting that caret scrolls the dock, and the scroll mirror would carry that
        value to the editor -- which put every edit back at the document's start.
        """
        window=self.window
        self.load('\n\n'.join(f'第 {i} 行正文 $ x_{i} + {i} $' for i in range(1,60)))
        window.compile_timer.stop();window.raw_timer.stop()
        window.resize(900,600);window.show();APPLICATION.processEvents()
        bar=window.editor.verticalScrollBar()
        bar.setValue(bar.maximum()//2);where=bar.value()
        self.assertGreater(where,0,'the fixture has to be long enough to scroll')
        self.assertEqual(window.source_view.textCursor().position(),0,'the dock caret sits at the top')
        window.replace(0,0,'X',typed=True);window.raw_timer.stop()
        self.assertEqual(bar.value(),where,'an edit must not move the view')
        formula=window.analysis['formulas'][len(window.analysis['formulas'])//2]
        window.activate(formula['start']);window.math_action('input',text='z');window.raw_timer.stop()
        self.assertEqual(bar.value(),where,'a formula edit must not move the view')
        self.assertEqual(window.source_view.verticalScrollBar().value(),where,'the dock still follows the editor')

    def test_the_source_dock_lines_up_with_the_editor(self):
        window=self.window
        self.load('第一行\n\n$ frac(a, b) $\n\n尾部\n')
        window.source_dock.show()
        dock=window.source_view;editor=window.editor
        self.assertEqual(dock.document().defaultFont().family(),editor.document().defaultFont().family())
        offset=(editor.viewport().mapTo(editor,editor.viewport().rect().topLeft()).y()+editor.document().documentMargin()
                -dock.viewport().mapTo(dock,dock.viewport().rect().topLeft()).y())
        self.assertEqual(dock.document().documentMargin(),offset,'both panes start at the same height')
        formula=window.analysis['formulas'][0]
        line=window.source.count('\n',0,from_byte(window.source,formula['start']))
        height=dock.document().findBlockByNumber(line).blockFormat().lineHeight()
        self.assertGreater(height,dock.fontMetrics().height(),'the formula line is taller than a text line')
        self.assertGreaterEqual(height,editor.handler.box(formula).height)
        # Both panes show the same lines, whichever one is scrolled.
        editor.verticalScrollBar().setValue(0);dock.verticalScrollBar().setValue(0)
        dock.verticalScrollBar().setValue(dock.verticalScrollBar().maximum())
        self.assertEqual(editor.verticalScrollBar().value(),dock.verticalScrollBar().value())

    def test_the_caret_survives_inside_an_empty_slot(self):
        """A dashed slot must keep its stop, or the caret has nowhere to sit."""
        window=self.window
        self.load('$ $');window.activate(0)
        for character in "\\frac":window.math_action('input',text=character)
        window.math_action('key',key='Enter')
        box=window.math_canvas.box
        slots=sorted((y,x,value[0],value[1]) for kind,x,y,value in box.operations if kind=='slot')
        self.assertEqual(len(slots),2)
        inside=[stop for stop in box.stops
                if any(sx<=stop[0]<=sx+width and sy<=stop[1]<=sy+height for sy,sx,width,height in slots)]
        self.assertEqual(len(inside),2,'both empty slots keep a caret position')
        self.assertTrue(any(stop[4] for stop in inside),'the caret is drawn inside the slot it is in')
        # Clicking the lower slot moves the caret there and it stays visible.
        lower=[stop for stop in inside if stop[1]>=slots[-1][0]][0]
        window.math_action('click',cursor=lower[3])
        box=window.math_canvas.box
        active=[stop for stop in box.stops if stop[4]]
        self.assertEqual(len(active),1)
        self.assertGreaterEqual(active[0][1],slots[-1][0]-1,'the caret sits in the denominator')
        self.load('$ $')

    def test_fragments_render_even_when_the_document_has_a_later_error(self):
        """A batch is compiled against the commands and the formula it asked for, so an
        error further down the document is not compiled at all and cannot take their
        images with it."""
        window=self.window
        self.load('正文 $ sum_(n=1)^oo frac(1, n^2) $ 与 $ dif x $。\n\n#panic("坏了")\n')
        window.compile_timer.stop();window.raw_timer.stop()
        loop=QEventLoop()
        QTimer.singleShot(200,window.load_raw)
        QTimer.singleShot(3000,loop.quit)
        loop.exec_()
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        self.assertTrue(nodes)
        drawn=[node['text'] for node in nodes if isinstance(window.typesetter.raw(node),dict)]
        self.assertEqual(len(drawn),len(nodes),'fragments before a document error must still get images')
        formula=window.analysis['formulas'][0]
        # Control: the document itself does not compile, so those images are the reduced
        # context's doing and not an error-free document's.
        outcome={};loop=QEventLoop()
        def previewed(result,error):outcome['error']=error;loop.quit()
        window.services.request('/api/preview',window.body()|{'preview':True},previewed,key='preview-whole')
        QTimer.singleShot(5000,loop.quit)
        loop.exec_()
        self.assertIn('坏',outcome.get('error') or '')
        # And the same fragments render even with no cut asked for at all: `context_end` is
        # no longer what keeps the batch alive, the reduced context is.
        outcome={};loop=QEventLoop()
        def done(result,error):outcome.update(result=result,error=error);loop.quit()
        window.services.request('/api/render',window.body()|{'raw':formula['render']['raw'],'formulas':[]},done,key='raw-whole')
        QTimer.singleShot(5000,loop.quit)
        loop.exec_()
        self.assertIsNone(outcome.get('error'))
        self.assertTrue(outcome['result']['items'],'an unrelated later error cannot blank the batch')

    def test_semantic_highlights_follow_an_incremental_edit(self):
        self.load('= 标题\n\n正文 $x$\n\n尾部 $y$')
        window=self.window;window.compile_timer.stop()
        def spans(source):
            # Spans are Python source indices; the mapping turns them into Qt
            # (UTF-16) positions for each view.
            found=[]
            for text,color in [('标题','#8250a3'),('尾部','#26384a')]:
                at=source.index(text)
                found.append((at,at+len(text),color))
            return found
        def positions():
            return [[(s.cursor.anchor(),s.cursor.position()) for s in editor.extraSelections()] for editor in window.highlight_views()]
        self.assertEqual(len(positions()),1,'the source dock is hidden by default')
        window.source_dock.show()
        window.semantic_spans=spans(window.source);window.engine_spans=[]
        window.apply_highlights()
        built=positions()
        self.assertEqual(len(built),2,'opening the dock colours it too')
        for view in range(2):
            self.assertEqual(len(built[view]),2,'both spans reach every view')
            self.assertTrue(all(b>a for a,b in built[view]))
        # An edit before the spans moves every view by the inserted display width.
        window.replace(0,0,'前缀 ')
        window.compile_timer.stop()
        window.semantic_spans=spans(window.source)
        window.apply_highlights()
        shifted=positions()
        for view in range(2):
            for index in range(2):
                self.assertEqual((shifted[view][index][0]-built[view][index][0],shifted[view][index][1]-built[view][index][1]),(3,3),f'view {view} span {index}')

    def test_a_formula_the_language_service_rejects_is_drawn_like_a_failed_fragment(self):
        """Only a node the engine has to evaluate can be marked.

        It is the node that carries a located range, so a half-typed argument is never
        dressed as broken -- and the drawing is the one a fragment whose image never came
        back gets, because both mean "there is nothing here the editor can lay out".
        """
        self.load('$ x + lr(a, size: #100%) $')
        window=self.window;window.compile_timer.stop();window.diagnostic_timer.stop()
        formula=next(f for f in window.analysis['formulas'] if f.get('view'))
        marked=next(n for n in window.view_nodes(formula['view']) if n.get('render_id'))
        start,end=(int(part) for part in marked['render_id'].split(':')[:2])
        prefix=window.source[:start];line=prefix.count('\n');line_start=prefix.rfind('\n')+1
        character=u16(window.source[line_start:start]);length=u16(window.source[line_start:end])
        diagnostic={'range':{'start':{'line':line,'character':character},
                             'end':{'line':line,'character':character+length}},
                    'message':'unknown variable'}
        asked=[]
        def request(route,body,callback,key=None,dropped=None):
            asked.append((route,body.get('method'),key));callback({'diagnostics':[diagnostic]},None)
        with patch.object(window.lsp,'request',side_effect=request):
            window.request_diagnostics()
        self.assertEqual(asked,[('/api/lsp','diagnostics','diagnostics')],'诊断要真的问出去')
        self.assertEqual(marked.get('error'),'unknown variable','诊断落在哪个节点就标哪个')
        # The editable part of the same formula is untouched.
        for node in window.view_nodes(formula['view']):
            if not node.get('render_id'):
                self.assertIsNone(node.get('error'),f'可编辑节点不该被标：{node.get("kind")}')
        box=window.typesetter.layout(formula['view'])
        self.assertTrue(any(op[0]=='failed' for op in box.operations),'按失败片段的样式画')
        # An answer that clears must clear the mark, not leave the last one standing.
        with patch.object(window.lsp,'request',side_effect=lambda route,body,callback,key=None:callback({'diagnostics':[]},None)):
            window.request_diagnostics()
        self.assertIsNone(marked.get('error'))

    def test_let_edit_rebuilds_affected_following_projections(self):
        # A name of **two or more** graphemes: a one-letter name is lexed as `MathText`,
        # so `$f(a)$` is never a call and this fixture used to pass for the wrong reason
        # (the *definition's* own projection changed, not the call sites').
        source='#let dbl(x) = $#x + 1$\nBefore $dbl(a)$\nAfter $dbl(b)$'
        self.load(source);window=self.window
        # The definition body is source, so only the two call sites have a view.
        before=[signature(formula.get('view') or {}) for formula in window.analysis['formulas']]
        at=source.index('+');calls=[];original=window.core.call
        def observed(action,**arguments):calls.append(action);return original(action,**arguments)
        with patch.object(window.core,'call',side_effect=observed):window.replace(at,at+1,'-')
        after=[signature(formula.get('view') or {}) for formula in window.analysis['formulas']]
        self.assertNotEqual(after,before,'改宏定义体必须重建后续调用点的投影')
        self.assertGreaterEqual(calls.count('analyze_formula'),2)
        self.assertNotIn('analyze',calls)

    def test_code_completion_popup_accepts_enter_as_one_source_edit(self):
        from unittest.mock import patch
        from PyQt5.QtTest import QTest
        self.load('#sy');window=self.window;editor=window.editor
        window.show();editor.setFocus();APPLICATION.processEvents()
        reply={'result':[{'label':'symbol','textEdit':{'range':{'start':{'line':0,'character':1},'end':{'line':0,'character':3}},'newText':'symbol'}}]}
        with patch.object(window,'request_language',side_effect=lambda method,callback:callback(reply)):
            window.complete()
            self.assertTrue(editor.completer.popup().isVisible())
            QTest.keyClick(editor,Qt.Key_Return)
            self.assertEqual(window.source,'#symbol')
            window.undo();self.assertEqual(window.source,'#sy')

    def test_real_formula_completion_uses_isolated_command_projection(self):
        status=self.service('/api/status',{},self.window.lsp)
        if not status.get('available'):self.skipTest('Tinymist is not installed')
        self.load('$x$');self.window.activate(0);self.window.math_action('input',text='\\fr')
        command=self.window.math_state['command']
        result=self.service('/api/completion',{key:command[key] for key in ('source','start','end','caret')},self.window.lsp)
        self.assertTrue(result['items'])
        self.assertTrue(any('frac' in item['label'] for item in result['items']))

    def test_mode_frames_render_without_modifying_document(self):
        self.load('$x$');window=self.window;window.activate(0);window.math_action('input',text='\\fr')
        before=window.source;box=window.math_canvas.box
        image=QImage(int(box.width+20),int(box.height+20),QImage.Format_ARGB32);image.fill(Qt.white)
        painter=QPainter(image);window.typesetter.paint(painter,box,5,5,True);painter.end()
        self.assertEqual(window.source,before)
        self.assertTrue(any(image.pixelColor(x,y).name()=='#edf4ff' for x in range(image.width()) for y in range(image.height())))

    def test_absolute_file_open_save_and_native_snapshot(self):
        with TemporaryDirectory() as directory:
            path=Path(directory)/'中文.typ';path.write_text('= Native editor\n\n中文 $ (a+b)/(c+d) $\n\n#let value = 42\n',encoding='utf-8')
            self.window.load(path);self.window.compile_timer.stop();self.window.completion_timer.stop()
            self.window.replace(0,0,'Test\n');self.window.compile_timer.stop()
            self.assertTrue(self.window.save());self.window.compile_timer.stop()
            self.assertEqual(path.read_text('utf-8'),self.window.source)
            self.window.show();APPLICATION.processEvents()
            image=QImage(self.window.size(),QImage.Format_ARGB32);image.fill(Qt.white)
            painter=QPainter(image);self.window.render(painter);painter.end()
            from .model import ROOT
            self.assertTrue(image.save(str(ROOT/'target/desktop-test.png')))
            self.window.load();self.window.compile_timer.stop()

# A scripted child for the pipe bookkeeping below. It answers every request with
# the source it holds, the actions it has seen and its own pid, so a test can
# tell a replayed session from a fresh one. Only the first child behaves as the
# test asked; a replacement answers at once and survives, unless `all` is set.
FAKE_CORE='''
import base64, json, os, sys, time
config = json.loads(base64.b64decode(sys.argv[1]))
source = ""
seen = []
while True:
    line = sys.stdin.readline()
    if not line: break
    request = json.loads(line)
    action = request.get("action", "")
    if action == "set_source": source = request.get("source", "")
    seen.append(action)
    if action == config.get("die", ""):
        sys.stderr.write(config.get("stderr", "fake core exploded") + "\\n"); sys.stderr.flush()
        sys.exit(config.get("code", 7))
    if action == config.get("slow", "") and config.get("delay", 0.0) > 0:
        time.sleep(config["delay"]); config["delay"] = 0.0
    print(json.dumps({"result": {"source": source, "active_range": None, "action": action,
        "pid": os.getpid(), "seen": seen}}), flush=True)
'''
FAKE_SERVICES='''
import base64, json, os, sys
config = json.loads(base64.b64decode(sys.argv[1]))
dying = bool(config.get("die"))
while True:
    line = sys.stdin.readline()
    if not line: break
    request = json.loads(line)
    if dying:
        dying = False
        sys.stderr.write("fake backend exploded\\n"); sys.stderr.flush()
        sys.exit(9)
    print(json.dumps({"id": request.get("id"), "result": {"route": request.get("route"), "pid": os.getpid()}}), flush=True)
'''

class BridgeTest(unittest.TestCase):
    """One slow or dead helper must not cost the window its formula service."""
    def test_real_core_recovers_cursor_selection_draft_and_history(self):
        scenarios=[
            ([('key',{'key':'End'})],('input',{'text':'z'}),'$a + b z$'),
            ([('key',{'key':'End'}),('key',{'key':'ArrowLeft','shift':True})],('input',{'text':'z'}),'$a + z$'),
            ([('key',{'key':'End'}),('input',{'text':'\\sqrt(x)'})],('key',{'key':'Enter'}),'$a + b sqrt(x)$'),
            ([('key',{'key':'End'}),('input',{'text':'z'})],('undo',{}),'$a + b$'),
        ]
        for actions,(action,args),expected in scenarios:
            with self.subTest(expected=expected):
                core=Core()
                try:
                    core.call('set_source',source='$a + b$');core.call('activate_formula',start=0)
                    for name,arguments in actions:core.call(name,**arguments)
                    core.child.kill();core.child.waitForFinished(1000)
                    self.assertEqual(core.call(action,**args)['source'],expected)
                finally:core.close()

    def scripted(self,script,config):
        """Run a scripted child instead of the real backend; returns the launches."""
        import json,sys
        from PyQt5.QtCore import QProcess
        launches=[]
        def spawn(parent,arguments,workspace=None):
            generation=len(launches);launches.append(list(arguments))
            settings=dict(config) if (not generation or config.get("all")) else {"die":"","slow":"","stderr":""}
            child=QProcess(parent);child.setProcessChannelMode(QProcess.SeparateChannels)
            child.start(sys.executable,["-c",script,base64.b64encode(json.dumps(settings).encode()).decode()])
            self.assertTrue(child.waitForStarted(5000),child.errorString())
            return child
        patcher=patch('desktop.bridge.process',spawn);patcher.start();self.addCleanup(patcher.stop)
        return launches

    def core(self,config,budgets=None):
        launches=self.scripted(FAKE_CORE,config)
        core=Core(budgets=budgets);self.addCleanup(core.close)
        return core,launches

    def backend(self,config):
        from .bridge import Services
        launches=self.scripted(FAKE_SERVICES,config)
        services=Services(ROOT/'workspace');self.addCleanup(services.close)
        return services,launches

    def answer(self,services,route,body):
        """Wait for one service reply, whichever way it went."""
        result=[];loop=QEventLoop();timer=QTimer();timer.setSingleShot(True);timer.timeout.connect(loop.quit)
        services.request(route,body,lambda value,error:(result.append((value,error)),loop.quit()))
        if not result:
            timer.start(20000);loop.exec_();timer.stop()
        self.assertTrue(result,'scripted backend timed out')
        return result[0]

    def test_a_slow_core_is_waited_for_instead_of_killed(self):
        """Five seconds used to be the deadline for every request, and a core that
        missed it was killed along with the rest of the window's formula service."""
        from PyQt5.QtCore import QProcess
        core,launches=self.core({'slow':'analyze','delay':6.0})
        reply=core.call('analyze')
        self.assertEqual(reply['action'],'analyze')
        self.assertEqual(len(launches),1,'one slow answer must not restart the core')
        self.assertEqual(core.child.state(),QProcess.Running)

    def test_a_core_that_died_mid_request_is_replaced_with_the_document_replayed(self):
        core,launches=self.core({'die':'analyze'})
        core.call('set_source',source='$ a+b $')
        reply=core.call('analyze')
        self.assertEqual(len(launches),2)
        self.assertEqual(reply['seen'],['set_source','analyze'],'a fresh core gets the document before the retried request')

    def test_a_core_that_stopped_answering_is_replaced_instead_of_abandoned(self):
        core,launches=self.core({'slow':'analyze','delay':1.5},budgets={'analyze':0.4})
        core.call('set_source',source='$ a+b $')
        reply=core.call('analyze')
        self.assertEqual(len(launches),2)
        self.assertEqual(reply['seen'],['set_source','analyze'])

    def test_a_core_that_keeps_failing_reports_the_request_and_its_stderr(self):
        core,launches=self.core({'die':'analyze','stderr':'fake core exploded','code':7,'all':True})
        core.call('set_source',source='$ a+b $')
        with self.assertRaises(RuntimeError) as caught:core.call('analyze')
        message=str(caught.exception)
        self.assertIn('analyze',message)
        self.assertIn('退出码 7',message)
        self.assertIn('fake core exploded',message)
        self.assertEqual(len(launches),2,'one recovery attempt, then a report')

    def test_a_backend_that_exited_is_replaced_for_the_next_request(self):
        services,launches=self.backend({'die':True})
        value,error=self.answer(services,'/api/status',{})
        self.assertIsNone(value);self.assertIn('后端进程退出',error)
        value,error=self.answer(services,'/api/status',{})
        self.assertIsNone(error)
        self.assertEqual(value['route'],'/api/status')
        self.assertEqual(len(launches),2)

if __name__=="__main__":unittest.main()
