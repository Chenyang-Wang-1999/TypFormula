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
from .mathview import Typesetter
from .window import Window,initial_window_geometry
from .rawcache import signature

APPLICATION=QApplication.instance() or QApplication([])
from .model import ROOT
for font in [ROOT/'web/fonts/NewCMMath-Regular.otf',Path('C:/Windows/Fonts/segoeui.ttf'),Path('C:/Windows/Fonts/msyh.ttc'),Path('C:/Windows/Fonts/consola.ttf')]:
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

    def test_native_objects_copy_and_edit_undo(self):
        self.load("中文😀 $ a/b $ 末尾")
        window=self.window;editor=window.editor
        self.assertEqual(len(editor.object_data),1)
        editor.selectAll();self.assertEqual(editor.createMimeDataFromSelection().text(),window.source)
        before=window.source;window.replace(0,2,"正文")
        window.undo();self.assertEqual(window.source,before)
        window.redo();self.assertTrue(window.source.startswith("正文"))

    def test_opaque_macro_stays_code_and_loaded_formula_folds(self):
        self.load("#let opaque(x) = $cancel(#x)$\n$opaque(y)$")
        self.assertEqual(len(self.window.editor.object_data),1)
        self.assertIn("$cancel(#x)$",self.window.editor.toPlainText())

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

    def service(self,route,body,client=None):
        result=[];loop=QEventLoop();timer=QTimer();timer.setSingleShot(True);timer.timeout.connect(loop.quit)
        def receive(value,error):result.append((value,error));loop.quit()
        (client or self.window.services).request(route,body,receive)
        timer.start(45000);loop.exec_();timer.stop()
        self.assertTrue(result,"native service timed out")
        self.assertIsNone(result[0][1],result[0][1]);return result[0][0]

    def test_preview_contains_real_source_positions_and_raw_svg(self):
        self.load("= Native test\nBefore $cancel(a)$ after.")
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
        def request(route,body,callback,key=None):callback(replies.pop(0),None)
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

    def test_macro_background_warmup_updates_native_cache_without_source_edits(self):
        self.load('#let fixed(x) = $#x + cancel(a)$\n$fixed(y)$')
        before=self.window.source;history=list(self.window.history)
        self.window.background()
        loop=QEventLoop();poll=QTimer();deadline=QTimer();deadline.setSingleShot(True)
        def ready():
            if not self.window.services.active and not self.window.services.queue and not self.window.lsp.active and not self.window.lsp.queue:loop.quit()
        poll.timeout.connect(ready);poll.start(20);deadline.timeout.connect(loop.quit);deadline.start(45000)
        loop.exec_();poll.stop();deadline.stop()
        self.assertTrue(self.window.typesetter.cache)
        self.assertEqual(self.window.source,before);self.assertEqual(self.window.history,history)
        self.assertEqual(self.window.preview_revision,-1,'macro warmup must not compile a live page preview')

    def test_typst_controls_native_limit_placement(self):
        self.load('$ sum_1^2 $')
        view=self.window.analysis['formulas'][0]['view']
        script=next(node for node in self.window.view_nodes(view) if node['kind']=='script')
        result=self.service('/api/attachments',{'path':'untitled.typ','expression':script['attachment'],'definitions':'','display':True})
        self.assertEqual(result['upper'],'limits')
        key=('',script['attachment'],True);self.window.typesetter.placements[key]=result
        self.window.prepare_view(view,'',True)
        self.assertEqual(script['_placement']['lower'],'limits')

    def fake_render(self):
        calls=[]
        def request(route,body,callback,key=None):
            if route=='/api/render':
                calls.append(body)
                callback({'items':[{'id':r['id']+':0:0','svg':'<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>','base_font_size_pt':1,'base_font_height_pt':1,'base_font_baseline_pt':.8} for r in body['raw']]},None)
            elif route=='/api/prewarm':callback({'results':[]},None)
            elif route=='/api/preview':callback({'pages':[]},None)
            else:callback({},None)
        return calls,request

    def test_raw_keeps_svg_when_adjacent_text_moves_its_source_range(self):
        from unittest.mock import patch
        self.load('$cancel(a)$');window=self.window;calls,request=self.fake_render()
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

    def test_script_edits_invalidate_only_base_raw_when_leaving_slot(self):
        from unittest.mock import patch
        self.load('$cancel(a)_(1)+cancel(b)$');window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):
            window.activate(0)
            raw=[n for n in window.view_nodes(window.math_state['view']) if n['kind']=='raw']
            for n in raw:window.typesetter.cache[n['_raw_key']]={'svg':'<svg/>','base_font_size_pt':1,'base_font_height_pt':1}
            keys=[n['_raw_key'] for n in raw]
            script=next(n for n in window.view_nodes(window.math_state['view']) if n['kind']=='script')
            stop=next(n for n in window.view_nodes(script['children'][2]) if n.get('cursor'))
            window.math_action('click',cursor=stop['cursor']);window.math_action('input',text='2')
            self.assertIn(keys[0],window.typesetter.cache)
            window.math_action('click',cursor=window.math_canvas.box.stops[-1][3])
            self.assertNotIn(keys[0],window.typesetter.cache)
            self.assertIn(keys[1],window.typesetter.cache)
            window.load_raw();self.assertEqual(len(calls),1);self.assertEqual(len(calls[0]['raw']),1)

    def test_equal_raw_sources_share_one_render_result(self):
        self.load('$cancel(a)$\n$cancel(a)$');window=self.window;calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request):window.load_raw()
        self.assertEqual(len(calls),1)
        raws=[next(n for n in window.view_nodes(formula['view']) if n['kind']=='raw') for formula in window.analysis['formulas']]
        self.assertIs(window.typesetter.raw(raws[0]),window.typesetter.raw(raws[1]))

    def test_distinct_visible_raws_across_formulas_use_one_batch_compile(self):
        self.load('$cancel(a)$\n$cancel(b)$\n$cancel(c)$');window=self.window;calls,request=self.fake_render()
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
        from unittest.mock import patch
        self.load('$cancel(a)$');window=self.window;window.raw_timer.stop();calls=[]
        def request(route,body,callback,key=None):
            calls.append(route)
            if route=='/api/prewarm':callback({'results':[]},None)
            else:callback({},None)
        document_revision=window.editor.document().revision()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.background()
        self.assertNotIn('/api/preview',calls);self.assertIn('/api/prewarm',calls)
        self.assertNotIn('/api/render',calls)
        self.assertEqual(window.editor.document().revision(),document_revision)

    def test_window_has_no_live_preview_pane_and_pdf_button_uses_typst_pdf(self):
        self.assertFalse(hasattr(self.window,'preview_dock'))
        self.load('= PDF\n\nHello $x^2$');window=self.window
        result=self.service('/api/pdf',window.body()|{'pdf':True})
        data=base64.b64decode(result['pdf'],validate=True);self.assertTrue(data.startswith(b'%PDF'))
        with TemporaryDirectory() as directory,patch('desktop.window.tempfile.gettempdir',return_value=directory),patch('desktop.window.QDesktopServices.openUrl',return_value=True) as opened:
            with patch.object(window.services,'request',side_effect=lambda route,body,callback,key=None:callback(result,None)):
                window.compile_pdf()
            opened.assert_called_once()
            output=Path(opened.call_args.args[0].toLocalFile());self.assertTrue(output.is_file());self.assertEqual(output.read_bytes(),data)

    def test_distant_formula_projections_survive_typst_incremental_edit(self):
        source=''.join(f'paragraph {i}\n\n$cancel(x_{i})$\n\n' for i in range(80))
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
                'view':{'kind':'cell','children':[{'kind':'raw','text':text,'render_id':f'{start}:{start+3}:0:0','warmup_range':[start,start+3]}]},
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
        self.assertEqual(second['view']['children'][0]['warmup_range'],[12,15])
        self.assertEqual(second['render']['raw'][0]['id'],'12:15:0')
        old=previous['formulas'][1]
        self.assertEqual((old['start'],old['end']),(10,13))
        self.assertEqual(old['view']['children'][0]['render_id'],'10:13:0:0')
        self.assertEqual(old['view']['children'][0]['warmup_range'],[10,13])
        self.assertEqual(old['render']['raw'][0]['id'],'10:13:0')

    def test_the_math_font_covers_every_glyph_the_core_can_draw(self):
        """A missing glyph is drawn from another font, so coverage is a test."""
        from PyQt5.QtGui import QFont,QRawFont
        from . import mathfont
        family=mathfont.resolve('','Consolas')
        font=QFont(family);font.setStyleStrategy(QFont.NoFontMerging)
        raw=QRawFont.fromFont(font)
        # Letters as the math alphabet, operators and Greek as the symbol table
        # writes them, the box glyphs the display tree adds, and the digits the
        # editor puts in text cells.
        # U+1D455 is unassigned; glyph() maps h to ℎ, which is required below.
        needed=[chr(0x1D44E+i) for i in range(26) if i!=7]+[chr(0x1D434+i) for i in range(26)]
        needed+=list('0123456789+-=()[]{}|/,.:;!?<>^_')+['ℎ','−','∗','≤','≥','≠','±','∓','×','⋅','÷','𝛼','𝛽','𝛾','𝛿','𝜀','𝜃','𝜆','𝜇','𝜋','𝜌','𝜎','𝜏','𝜑','𝜓','𝜔','Γ','Δ','Θ','Σ','Ω','□','│','⌘','·','‖']
        missing=[ch for ch in needed if raw.glyphIndexesForString(ch)[0]==0]
        self.assertEqual(missing,[],f'{family} cannot draw {missing}')

    def test_a_failed_fragment_is_reported_and_entered_with_a_horizontal_key(self):
        """A Raw without an image has nothing to click, so a key has to open it."""
        window=self.window
        self.load('$ undefinedfunc(α) $');window.activate(0);window.compile_timer.stop()
        text=next(node['text'] for node in window.view_nodes(window.math_state['view']) if node['kind']=='raw')
        self.assertEqual(text,'undefinedfunc(α)')
        # What load_raw records when the render pass returns nothing for a fragment.
        window.typesetter.cache[('raw',text)]=False
        window.report_raw_fragments(force=True)
        window.math_action('key',key='ArrowRight')
        state=window.math_state
        self.assertTrue(state['pending'],'the caret must be inside the fragment draft')
        self.assertEqual(state['view']['children'][1]['kind'],'unknown')
        # Escape restores the fragment and closes the draft.
        window.math_action('key',key='Escape')
        self.assertFalse(window.math_state['pending'])
        self.assertEqual(window.source,'$ undefinedfunc(α) $')

    def test_a_fragment_without_a_source_range_is_marked_as_failed(self):
        window=self.window
        self.load('$ sum_(n=0)^oo a_n $')
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        self.assertTrue(nodes)
        for node in nodes:node.pop('render_id',None)
        window.load_raw()
        for node in nodes:self.assertIs(window.typesetter.cache[('raw',node['text'])],False)
        box=window.editor.handler.box(window.analysis['formulas'][0])
        marks=[kind for kind,_,_,_ in box.operations]
        self.assertIn('failed',marks,'a fragment with no image must say so where it is drawn')

    def test_a_rendered_fragment_is_not_marked_and_draws_its_image(self):
        window=self.window
        self.load('$ sum_(n=0)^oo a_n $')
        nodes=[node for formula in window.analysis['formulas'] for node in window.view_nodes(formula['view']) if node['kind']=='raw']
        for node in nodes:
            window.typesetter.cache[('raw',node['text'])]={'svg':'<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"/>',
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
        def failing(route,body,callback,key=None):
            requests.append((route,body,key))
            if len(requests)==1:callback(None,'渲染后端编译失败')
            else:callback({'items':[]},None)
        with patch.object(window.services,'request',side_effect=failing):
            window.load_raw()
            self.assertEqual(len(requests),1)
            for node in nodes:self.assertIs(window.typesetter.cache[('raw',node['text'])],False)
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
        def salvaged(route,body,callback,key=None):
            requests.append((route,body,key))
            callback({'items':[],'failed':[source_id]},None)
        with patch.object(window.services,'request',side_effect=salvaged):
            window.load_raw()
            self.assertEqual(len(requests),1)
            self.assertIs(window.typesetter.cache[('raw',refused['text'])],False)
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
        """The request compiles only as far as the fragments reach, so an error
        further down the document cannot take their images with it."""
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
        # The same fragments asked for without the context cannot compile.
        formula=window.analysis['formulas'][0]
        outcome={};loop=QEventLoop()
        def done(result,error):outcome['error']=error;loop.quit()
        window.services.request('/api/render',window.body()|{'raw':formula['render']['raw'],'formulas':[]},done,key='raw-whole')
        QTimer.singleShot(5000,loop.quit)
        loop.exec_()
        self.assertIn('坏',outcome.get('error') or '')

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

    def test_let_edit_rebuilds_affected_following_projections(self):
        source='#let f(x) = $#x + 1$\nBefore $f(a)$\nAfter $f(b)$'
        self.load(source);window=self.window
        before=[signature(formula['view']) for formula in window.analysis['formulas']]
        at=source.index('+');calls=[];original=window.core.call
        def observed(action,**arguments):calls.append(action);return original(action,**arguments)
        with patch.object(window.core,'call',side_effect=observed):window.replace(at,at+1,'-')
        after=[signature(formula['view']) for formula in window.analysis['formulas']]
        self.assertNotEqual(after,before)
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
