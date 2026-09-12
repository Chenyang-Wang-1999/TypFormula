"""Frozen application self-test: offscreen Qt, private pipes, no WebEngine window."""
import json
import os
import sys
import tempfile
import traceback
from pathlib import Path


def main(destination):
    report = {"ok": False, "webengine_page_tested": False}
    destination = Path(destination).resolve()
    try:
        with tempfile.TemporaryDirectory(prefix="typformula-smoke-") as temporary:
            # Do not pick up the developer's config, tools, extensions, or package cache.
            for name in ("TYPFORMULA_BIN", "TYPFORMULA_ADAPTER", "TINYMIST_BIN", "TYPFORMULA_WORKSPACE"):
                os.environ.pop(name, None)
            for name in ("APPDATA", "LOCALAPPDATA", "USERPROFILE", "HOME"):
                os.environ[name] = temporary
            os.environ["TYPFORMULA_CONFIG"] = str(Path(temporary) / "settings.json")
            os.environ["TYPST_PACKAGE_CACHE_PATH"] = str(Path(temporary) / "packages")
            os.environ["QT_QPA_PLATFORM"] = "offscreen"
            system = Path(os.environ.get("SystemRoot", r"C:\Windows"))
            os.environ["PATH"] = os.pathsep.join(map(str, (system / "System32", system)))
            from PyQt5.QtWidgets import QApplication
            from PyQt5.QtCore import QEventLoop, QTimer
            from PyQt5.QtSvg import QSvgRenderer
            from . import mathfont, preview
            from .svg import qt_svg
            from .runtime import resource_root, helper, bundled_tinymist
            # Import WebEngine before QApplication; never construct QWebEngineView here.
            if not preview.prepare():
                raise RuntimeError(preview.available()[1])
            application = QApplication([])
            from .window import Window
            if not mathfont.install():
                raise RuntimeError("Bundled math fonts could not be loaded")
            window = None
            qt_errors = []
            previous_hook = sys.excepthook
            sys.excepthook = lambda kind, error, tb: qt_errors.append(error)
            try:
                window = Window()
                window.compile_timer.stop()
                source = '中文 $ frac(a, b) + bold(x) $ 后面正文 $ y $'
                window.replace(0, len(window.source), source)
                window.compile_timer.stop()
                if not window.analysis.get("formulas"):
                    raise RuntimeError("Document analysis returned no formulas")
                # Exercise synchronous Qt relayout with later formula byte offsets.
                # This is still offscreen; no native desktop or WebEngine page opens.
                window.show(); application.processEvents()
                window.activate(window.analysis['formulas'][0]['start'])
                if not window.math_state:
                    raise RuntimeError("Formula editor did not activate")
                window.math_action('input', text='\\dots')
                window.math_action('key', key='Enter')
                application.processEvents()
                if qt_errors:
                    raise RuntimeError(f"Qt formula relayout failed: {qt_errors}")
                if 'dots' not in window.source or window.math_state.get('pending'):
                    raise RuntimeError("Unicode-context command commit failed")
                report["formula_width"] = window.math_canvas.box.width
                report["unicode_command_edit"] = True

                def ask(route, body, service=None):
                    loop = QEventLoop()
                    timer = QTimer(); timer.setSingleShot(True)
                    answer = []
                    def done(value, error):
                        answer.append((value, error)); loop.quit()
                    timer.timeout.connect(loop.quit)
                    (service or window.services).request(route, body, done)
                    if not answer:
                        timer.start(60000); loop.exec_()
                    timer.stop()
                    if not answer:
                        raise RuntimeError(route + " timed out")
                    value, error = answer[0]
                    if error:
                        raise RuntimeError(route + ": " + error)
                    return value

                glyphs = ask('/api/glyphs', {"path": "main.typ", "definitions": "", "expression": "bold(x)", "display": True})
                if not glyphs.get("glyphs"):
                    raise RuntimeError("Glyph rendering returned an empty result")
                body = {"path": "main.typ", "source": source, "raw": []}
                svg = ask('/api/preview', dict(body, preview=True))
                pages = svg.get("pages", [])
                if not pages or not QSvgRenderer(qt_svg(pages[0]["svg"])).isValid():
                    raise RuntimeError("SVG rendering failed")
                pdf = ask('/api/pdf', dict(body, pdf=True))
                import base64
                if not base64.b64decode(pdf.get("pdf", "")).startswith(b"%PDF-"):
                    raise RuntimeError("PDF rendering failed")
                tinymist = bundled_tinymist()
                if tinymist:
                    answer = ask('/api/lsp', dict(body, method="hover", position={"line": 0, "character": 3}), window.lsp)
                    if not answer.get("uri"):
                        raise RuntimeError("Tinymist did not open the document")
                report.update(ok=True, resources=str(resource_root()), fonts=mathfont.install(),
                              backend=str(helper("typformula.exe", "TYPFORMULA_BIN", "server")),
                              adapter=str(helper("typformula-layout.exe", "TYPFORMULA_ADAPTER", "adapter")),
                              tinymist=str(tinymist) if tinymist else None, glyphs=glyphs["glyphs"],
                              svg_pages=len(pages), pdf=True, workspace=str(window.workspace))
            finally:
                if window is not None:
                    window.math_state = None; window.saved = window.source
                    window.close()
                application.processEvents()
                application.quit()
                sys.excepthook = previous_hook
    except Exception:
        report["error"] = traceback.format_exc()
    destination.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if report["ok"] else 1
