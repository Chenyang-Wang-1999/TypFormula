"""Private pipes; no web server, WebView or WASM. Qt owns child lifetime."""
import json
import os
from pathlib import Path
from PyQt5.QtCore import QObject, QProcess, QTimer
from .model import ROOT

def backend():
    default=ROOT/"target/server/release/visual-typst.exe"
    if not default.is_file():default=ROOT/"target/server/debug/visual-typst.exe"
    path = Path(os.environ.get("VISUAL_TYPST_BIN", default))
    if not path.is_file():
        raise RuntimeError("请先运行 build-desktop.cmd：缺少 " + str(path))
    return str(path)

def process(parent, arguments, workspace=None):
    child = QProcess(parent)
    environment = child.processEnvironment()
    from PyQt5.QtCore import QProcessEnvironment
    environment = QProcessEnvironment.systemEnvironment()
    adapter=ROOT/"target/adapter/release/visual-typst-layout.exe"
    if not adapter.is_file():adapter=ROOT/"target/adapter/debug/visual-typst-layout.exe"
    environment.insert("VISUAL_TYPST_ADAPTER", str(adapter))
    child.setProcessEnvironment(environment)
    if workspace:
        child.setWorkingDirectory(str(workspace))
    child.start(backend(), arguments)
    if not child.waitForStarted(5000):
        raise RuntimeError(child.errorString())
    return child

class Core(QObject):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.child = process(self, ["--desktop-core"])

    def call(self, action, **arguments):
        self.child.write((json.dumps({"action": action, **arguments}, ensure_ascii=False) + "\n").encode())
        while not self.child.canReadLine():
            if not self.child.waitForReadyRead(5000):
                self.child.kill();self.child.waitForFinished(1000)
                raise RuntimeError("公式核心没有响应：" + self.child.errorString())
        result = json.loads(bytes(self.child.readLine()))
        if "error" in result:
            raise ValueError(result["error"])
        return result["result"]

    def close(self):
        self.child.closeWriteChannel()
        if not self.child.waitForFinished(1000):
            self.child.kill()
            self.child.waitForFinished(1000)

class Services(QObject):
    def __init__(self, workspace, parent=None):
        super().__init__(parent)
        self.child = process(self, ["--stdio", str(workspace)], workspace)
        self.queue = []
        self.active = None
        self.sequence = 0
        self.buffer = b""
        self.closed = False
        self.child.readyReadStandardOutput.connect(self.receive)
        self.child.finished.connect(self.exited)
        self.timer = QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.timeout.connect(self.timeout)

    def request(self, route, body, callback, key=None):
        if self.closed:
            callback(None, "后端已关闭")
            return
        # Coalesce pending preview/LSP work; never drop file/export actions.
        if key:
            self.queue = [item for item in self.queue if item[3] != key]
        self.queue.append((route, body, callback, key))
        self.advance()

    def advance(self):
        if self.active or not self.queue or self.closed:
            return
        self.active = self.queue.pop(0)
        self.sequence += 1
        route, body, _, _ = self.active
        self.child.write((json.dumps({"id": self.sequence, "route": route, "body": body}, ensure_ascii=False) + "\n").encode())
        self.timer.start(65000)

    def receive(self):
        self.buffer += bytes(self.child.readAllStandardOutput())
        while b"\n" in self.buffer:
            line, self.buffer = self.buffer.split(b"\n", 1)
            if not self.active:
                continue
            try:
                reply = json.loads(line)
                if reply.get("id") != self.sequence:
                    continue
            except Exception as error:
                self.fail(str(error))
                continue
            callback = self.active[2]
            self.active = None
            self.timer.stop()
            callback(reply.get("result"), reply.get("error"))
            self.advance()

    def fail(self, error):
        callbacks = ([self.active] if self.active else []) + self.queue
        self.active = None
        self.queue = []
        self.timer.stop()
        for item in callbacks:
            item[2](None, error)

    def timeout(self):
        self.closed = True
        self.child.kill()
        self.fail("后端请求超时；源码已保留。重新打开窗口可重启后端。")

    def exited(self):
        if not self.closed:
            self.closed = True
            self.fail("后端进程退出；源码已保留。")

    def close(self):
        self.closed = True
        self.timer.stop()
        self.queue = []
        self.active = None
        self.child.closeWriteChannel()
        if not self.child.waitForFinished(1000):
            self.child.kill()
            self.child.waitForFinished(1000)
