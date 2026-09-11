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
        self.assertEqual(len(self.window.editor.object_data),1)
        self.assertIn("$lr(#x, size: #100%)$",self.window.editor.toPlainText())

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

    def test_a_macro_fragment_is_rendered_from_its_call_site_like_any_other(self):
        """The ordinary render batch covers a fragment inside a macro template.

        The fragment's own text lives in the definition, so that is the range the
        batch asks for; the document's call to the macro is what compiles it.
        """
        fragment='lr(a, size: #100%)'
        self.load(f'#let fixed(x) = $#x + {fragment}$\n$fixed(y)$')
        window=self.window;window.compile_timer.stop()
        before=window.source;history=len(window.history)
        calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.load_raw()
        self.assertEqual(len(calls),1)
        # The range is derived from the fragment rather than written out: an opaque
        # fixture of a different length silently changed what this asserted before.
        definition=window.source.index(fragment)
        end=definition+len(fragment)
        self.assertIn({'id':f'{definition}:{end}','start':definition,'end':end},calls[0]['raw'])
        node=next(node for node in window.view_nodes(window.analysis['formulas'][0]['view']) if node['kind']=='raw')
        self.assertIsInstance(window.typesetter.raw(node),dict,'the batch result is what this fragment draws')
        self.assertEqual(window.preview_revision,-1,'an image request must not touch the preview')
        self.assertEqual(window.source,before);self.assertEqual(len(window.history),history,'asking for an image never edits the document')

    def test_a_definition_fragment_keeps_the_call_that_renders_it(self):
        """Typst typesets a definition's fragment where the macro is called, so the
        context cut may not drop a call that is later in the document."""
        self.load('#let fixed(x) = $#x + lr(a, size: #100%)$\n\n$ fixed(y) $')
        window=self.window;window.compile_timer.stop();window.raw_timer.stop()
        # Only the definition's own formula is on screen; its call sits below it.
        with patch.object(window,'visible_formula_starts',return_value={window.source.index('$#x')}):
            calls,request=self.fake_render()
            with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
                window.load_raw()
        self.assertEqual(len(calls),1)
        self.assertEqual(calls[0]['context_end'],len(window.source.encode('utf-8')),'the call must stay inside the compiled source')
        node=next(node for node in window.view_nodes(window.analysis['formulas'][0]['view']) if node['kind']=='raw')
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
        def request(route,body,callback,key=None):
            if route=='/api/render':
                calls.append(body)
                callback({'items':[{'id':r['id']+':0:0','svg':'<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>','base_font_size_pt':1,'base_font_height_pt':1,'base_font_baseline_pt':.8} for r in body['raw']]},None)
            elif route=='/api/glyphs':
                calls.append(body)
                answers={'bold(A)':'\U0001D468','upright(A)':'A','bold(upright(a))':'\U0001D41A',
                         'bold(x)':'\U0001D499','upright(x)':'x','bold(upright(x))':'\U0001D431',
                         'bold(a)':'\U0001D482'}
                callback({'glyphs':answers.get(body['expression'],'')},None)
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
        self.load('$bold(A)$');window=self.window
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
        self.load('$upright(A) + bold(A)$');window=self.window
        calls,request=self.fake_render()
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
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
        self.assertIn('bold(x)',[body.get('expression') for body in calls],'加载后自己去取字形簇')
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
        def request(route,body,callback,key=None):
            if route=='/api/glyphs':asked.append((body,callback))
            else:callback({},None)
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.replace(0,len(window.source),'$x$')
            window.compile_timer.stop()
            window.activate(window.analysis['formulas'][0]['start'])
            window.math_action('input',text='\\')
            window.math_action('input',text='bold(x)')
            window.math_action('key',key='Enter')
            self.assertIn('bold(x)',[body['expression'] for body,_ in asked],
                          '确认命令后要问引擎取字形簇')
            # The service answers out of band, so the fake one must too: answering
            # inline would drive a path the window never takes.
            for body,callback in asked:
                callback({'glyphs':'\U0001D499' if body['expression']=='bold(x)' else ''},None)
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
        def request(route,body,callback,key=None):
            if route=='/api/glyphs':asked.append((body,callback))
            else:callback({},None)
        with patch.object(window.services,'request',side_effect=request),patch.object(window,'semantic_highlight'):
            window.replace(0,len(window.source),'$x + bold(x)$')
            window.compile_timer.stop()
            self.assertIn('bold(x)',[body['expression'] for body,_ in asked],'新拼写要问引擎')
            # Out of band, like the real service: answering inline would test the order the
            # fake happens to have, not the one the window has.
            for body,callback in asked:callback({'glyphs':'\U0001D499'},None)
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
        """VISUAL_TYPST_RAW_CACHE=plain keeps an image only for a fragment without a call.

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
        def request(route,body,callback,key=None):
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
        def request(route,body,callback,key=None):
            asked.append((route,body.get('action')))
            if route=='/api/preview/live' and body.get('action')=='start':
                callback({'staticServerPort':38251,'dataPlanePort':38251,'isPrimary':True},None)
            else:callback({},None)
        with patch.object(window.services,'request',side_effect=request):
            self.assertFalse(window.preview_started,'没有开启时不该有预览在跑')
            window.set_preview(True)
        self.assertIn(('/api/preview/live','start'),asked,'开启时才向 Tinymist 要预览')
        self.assertEqual(len(loaded),1,'按回复里的地址加载预览页')
        # The page and its WebSocket share a port, so the reply's address is the whole URL.
        self.assertIn('38251',loaded[0])
        with patch.object(window.services,'request',side_effect=request):
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
        def request(route,body,callback,key=None):
            if route=='/api/preview/live' and body.get('action')=='kill':killed.append(route)
            callback({'staticServerPort':1,'dataPlanePort':1,'isPrimary':True},None)
        with patch.object(window.services,'request',side_effect=request):
            window.set_preview(True)
            self.assertTrue(window.preview_started)
            events=type('E',(object,),{'ignore':lambda self:None,'accept':lambda self:None})()
            window.closeEvent(events)
        self.assertTrue(killed,'关窗要停掉预览')

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
        def failing(route,body,callback,key=None):
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
        def salvaged(route,body,callback,key=None):
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
