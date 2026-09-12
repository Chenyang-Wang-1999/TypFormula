import sys
from PyQt5.QtCore import Qt
from PyQt5.QtWidgets import QApplication, QMessageBox
from . import mathfont
from . import preview

def main():
    QApplication.setAttribute(Qt.AA_EnableHighDpiScaling,True)
    # QtWebEngine must be initialised before the application object exists, and it wants
    # shared GL contexts; both are hard errors the other way round. `preview.prepare()`
    # does the import and the attribute, and reports whether a preview can be shown at
    # all — the editor works without it (the wheel is not part of PyQt5 itself).
    preview.prepare()
    application=QApplication(sys.argv);application.setApplicationName("TypFormula")
    # LyX's FontLoader does the same: the bundled math fonts have to be registered
    # before any family name is resolved, and a missing one is never guessed at.
    mathfont.install()
    from .window import Window
    def exception(kind,value,traceback):
        import traceback as trace
        trace.print_exception(kind,value,traceback)
        QMessageBox.warning(None,"TypFormula",str(value))
    sys.excepthook=exception
    try:window=Window(sys.argv[1] if len(sys.argv)>1 else None)
    except Exception as error:QMessageBox.critical(None,"启动失败",str(error));return 1
    window.show();return application.exec_()

if __name__=="__main__":sys.exit(main())
