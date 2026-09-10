"""Private pipes to the native backends. Qt owns child lifetime."""
import json
import os
import time
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
    from PyQt5.QtCore import QProcessEnvironment
    environment = QProcessEnvironment.systemEnvironment()
    adapter=ROOT/"target/adapter/release/visual-typst-layout.exe"
    if not adapter.is_file():adapter=ROOT/"target/adapter/debug/visual-typst-layout.exe"
    environment.insert("VISUAL_TYPST_ADAPTER", str(adapter))
    child = QProcess(parent)
    child.setProcessEnvironment(environment)
    # A backend that dies says why on stderr. Forwarded to the terminal it never
    # reaches the window, which is how a refused request ended up reported as the
    # bare string "Unknown error"; keep the tail for the message instead.
    child.setProcessChannelMode(QProcess.SeparateChannels)
    if workspace:
        child.setWorkingDirectory(str(workspace))
    child.start(backend(), arguments)
    if not child.waitForStarted(5000):
        raise RuntimeError(child.errorString())
    return child

class Stderr:
    """The tail of a helper process's stderr, for the messages that report it."""
    LIMIT = 4000
    def __init__(self, child, limit=LIMIT):
        self.child=child;self.limit=limit;self.text=""
        child.readyReadStandardError.connect(self.read)
    def read(self):
        chunk=bytes(self.child.readAllStandardError()).decode("utf-8","replace")
        if chunk:self.text=(self.text+chunk)[-self.limit:]
    def detail(self):
        self.read()
        lines=[line.strip() for line in self.text.splitlines() if line.strip()]
        return " / ".join(lines[-3:])[:400]

# `set_source` and `analyze` parse the whole document before they answer: 1500
# formulas took 2.8 s alone and 3.9 s while a second window was loading beside
# them, so one five second deadline for every action killed healthy cores. A
# killed core then took that window's formula service down for the rest of the
# session, which is what "第二个 desktop 无法唤起公式服务" was.
QUICK = 10.0
SLOW = 60.0
BUDGETS = {"set_source":SLOW,"analyze":SLOW,"analyze_formula":SLOW,
    "activate_formula":SLOW,"preview_results":SLOW}

class Core(QObject):
    """One document and one formula session, in one child process.

    A window keeps its formula service for its whole life: a core that died, or
    that stopped answering, is replaced and put back into the session mirrored
    from the replies, so the cost is one request instead of every later one.
    """
    def __init__(self, parent=None, budgets=None):
        super().__init__(parent)
        self.budgets=dict(BUDGETS);self.budgets.update(budgets or {})
        self.document=None      # the source the child confirmed last
        self.active=None        # start of the formula session the child is in
        self.problem=""         # why the last exchange failed
        self.last_detail=""     # what the replaced child said on stderr
        self.closed=False
        self.child=None;self.errors=None
        self.spawn()

    def spawn(self):
        self.child=process(self,["--desktop-core"]);self.errors=Stderr(self.child)

    def dispose(self):
        child=self.child
        if child is None:return
        if self.errors:
            text=self.errors.detail()
            if text:self.last_detail=text
        child.blockSignals(True)
        if child.state()!=QProcess.NotRunning:
            child.kill();child.waitForFinished(1000)
        child.deleteLater()
        self.child=None;self.errors=None

    def budget(self, action):return self.budgets.get(action,QUICK)
    def running(self):return self.child is not None and self.child.state()==QProcess.Running

    def exit_detail(self):
        if self.child.exitStatus()==QProcess.NormalExit:return f"（退出码 {self.child.exitCode()}）"
        return "（被强制结束）"

    def exchange(self, action, arguments, budget):
        """One request and its answer. Returns None and sets `problem` on failure."""
        self.problem=""
        self.child.write((json.dumps({"action":action,**arguments},ensure_ascii=False)+"\n").encode())
        deadline=time.monotonic()+budget
        while not self.child.canReadLine():
            if self.child.state()!=QProcess.Running:
                self.problem="公式核心已退出"+self.exit_detail();return None
            remaining=deadline-time.monotonic()
            if remaining<=0:
                self.problem=f"公式核心没有响应（{action} 已等待 {budget:.0f} 秒）";return None
            # Short slices, so a child that dies is noticed at once instead of
            # after the whole budget has passed.
            self.child.waitForReadyRead(min(200,max(1,int(remaining*1000))))
        try:reply=json.loads(bytes(self.child.readLine()))
        except ValueError as error:
            self.problem=f"公式核心返回了无法解析的响应：{error}";return None
        if not isinstance(reply,dict):
            self.problem="公式核心返回了无法解析的响应";return None
        return reply

    def mirror(self, reply):
        """Remember the source and the session the child just confirmed."""
        result=reply.get("result")
        if not isinstance(result,dict):return
        if isinstance(result.get("source"),str):self.document=result["source"]
        # `analyze` and `scan` do not report the session, so their replies must
        # not read as "no formula is open".
        if "active_range" in result:
            bounds=result["active_range"]
            self.active=bounds.get("start") if isinstance(bounds,dict) else None

    def call(self, action, **arguments):
        if self.closed:raise RuntimeError("公式核心已关闭")
        budget=self.budget(action)
        for attempt in (0,1):
            if self.running():
                reply=self.exchange(action,arguments,budget)
                if reply is not None:
                    self.mirror(reply)
                    if "error" in reply:raise ValueError(reply["error"])
                    return reply["result"]
            elif not self.problem:self.problem="公式核心已退出"
            if attempt:break
            # One recovery. The request cannot have been applied -- the child died
            # with it or never answered it -- and the session is rebuilt below, so
            # asking again is exactly what the user meant by it.
            self.dispose();self.restore()
        raise RuntimeError(self.describe(action))

    def restore(self):
        """A fresh child, put back into the session this window was in.

        The document is the source the child confirmed last and every change
        since was mirrored from a reply, so replaying `set_source` restores it
        exactly; a formula session is re-entered afterwards when it was open.
        """
        self.spawn()
        if self.document is None:return
        if self.exchange("set_source",{"source":self.document},self.budget("set_source")) is None:return
        if self.active is not None:self.exchange("activate_formula",{"start":self.active},self.budget("activate_formula"))

    def reason(self):
        live=self.errors.detail() if self.errors else ""
        text=" / ".join(detail for detail in (self.last_detail,live) if detail)
        return ("。核心错误输出："+text) if text else ""

    def describe(self, action):
        return f"{self.problem or '公式核心没有响应'}；请求 {action} 在重建公式核心后仍未成功{self.reason()}"

    def close(self):
        self.closed=True;self.dispose()

class Services(QObject):
    """Tinymist and the Typst compiler behind one private pipe.

    Every request carries its own payload, so a child that exited is simply
    replaced: without that, one crash left the window without previews, formula
    images and language services until it was reopened.
    """
    def __init__(self, workspace, parent=None):
        super().__init__(parent)
        self.workspace=workspace
        self.queue = []
        self.active = None
        self.sequence = 0
        self.buffer = b""
        self.closed = False
        self.last_detail = ""
        self.child=None;self.errors=None
        self.timer = QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.timeout.connect(self.timeout)
        self.spawn()

    def spawn(self):
        self.buffer=b""
        self.child = process(self, ["--stdio", str(self.workspace)], self.workspace)
        self.errors = Stderr(self.child)
        self.child.readyReadStandardOutput.connect(self.receive)
        self.child.finished.connect(self.exited)

    def detail(self):
        text=self.errors.detail() if self.errors else ""
        if not text:text=self.last_detail
        return ("；后端错误输出："+text) if text else ""

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
        if self.child.state()!=QProcess.Running:
            if self.errors:
                text=self.errors.detail()
                if text:self.last_detail=text
            self.child.blockSignals(True);self.child.deleteLater();self.spawn()
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
            item[2](None, error+self.detail())

    def timeout(self):
        self.child.kill()
        self.fail("后端请求超时；源码已保留。后端会在下一次请求时重启。")

    def exited(self):
        # The next request starts a fresh child; `advance` does that. Failing here
        # only reports what the dead one was carrying.
        self.fail("后端进程退出；源码已保留，下一次请求会重启后端。")

    def close(self):
        self.closed = True
        self.timer.stop()
        self.queue = []
        self.active = None
        self.child.closeWriteChannel()
        if not self.child.waitForFinished(1000):
            self.child.kill()
            self.child.waitForFinished(1000)
