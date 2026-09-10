import sys
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QFontDatabase
from PyQt5.QtWidgets import QApplication, QMessageBox
from .model import ROOT

def main():
    QApplication.setAttribute(Qt.AA_EnableHighDpiScaling,True)
    application=QApplication(sys.argv);application.setApplicationName("Visual Typst")
    QFontDatabase.addApplicationFont(str(ROOT/"web/fonts/NewCMMath-Regular.otf"))
    QFontDatabase.addApplicationFont(str(ROOT/"web/fonts/NewCM10-Italic.otf"))
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
