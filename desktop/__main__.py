import sys
from PyQt5.QtCore import Qt
from PyQt5.QtWidgets import QApplication, QMessageBox
from . import mathfont

def main():
    QApplication.setAttribute(Qt.AA_EnableHighDpiScaling,True)
    application=QApplication(sys.argv);application.setApplicationName("Visual Typst")
    # LyX's FontLoader does the same: the bundled math fonts have to be registered
    # before any family name is resolved, and a missing one is never guessed at.
    mathfont.install()
    from .window import Window
    def exception(kind,value,traceback):
        import traceback as trace
        trace.print_exception(kind,value,traceback)
        QMessageBox.warning(None,"Visual Typst",str(value))
    sys.excepthook=exception
    try:window=Window(sys.argv[1] if len(sys.argv)>1 else None)
    except Exception as error:QMessageBox.critical(None,"启动失败",str(error));return 1
    window.show();return application.exec_()

if __name__=="__main__":sys.exit(main())
