"""The live preview: Tinymist's own preview page, shown in a web view.

The editor does not render this. Tinymist serves a self-contained page plus a WebSocket
that pushes **incremental** renderings (a full `new` scene, then `diff-v1` deltas), and
this module's whole job is to load that page and to tear it down again. Everything the
preview does — partial rendering, scroll sync, dark-mode inversion — comes from Tinymist.

Two facts constrain the shape of this file, both measured rather than assumed:

* QtWebEngine must be imported, and `Qt.AA_ShareOpenGLContexts` set, **before** a
  QApplication exists. Importing it later raises, and constructing the view without the
  attribute makes Qt warn and misbehave. `prepare()` is therefore called from `__main__`
  before the application object is built.
* The web view is a **separate wheel** from PyQt5 (`PyQtWebEngine`). A machine that
  installed only `desktop/requirements.txt` from an older revision has no preview, so
  everything here degrades to "unavailable with a reason" instead of failing to import.
"""
from PyQt5.QtCore import Qt, QUrl

# `None` until `prepare()` runs; `False` when the wheel is missing.
_available = None
_reason = "尚未初始化"


def prepare():
    """Import the web engine and set its attribute; report whether it works.

    Must be called before the QApplication is constructed. Idempotent.
    """
    global _available, _reason
    if _available is not None:
        return _available
    try:
        from PyQt5.QtCore import QCoreApplication
        # The attribute has to be set before any QGuiApplication exists, which is why
        # this cannot live in the window: by then it is too late and Qt only warns.
        QCoreApplication.setAttribute(Qt.AA_ShareOpenGLContexts)
        from PyQt5.QtWebEngineWidgets import QWebEngineView  # noqa: F401
        _available, _reason = True, ""
    except Exception as error:  # missing wheel, or a Qt built without it
        _available, _reason = False, f"实时预览需要 PyQtWebEngine（{error}）"
    return _available


def available():
    """Whether a preview can be shown, and why not when it cannot."""
    return bool(_available), _reason


def view():
    """A new web view, or None when the web engine is not available."""
    if not prepare():
        return None
    from PyQt5.QtWebEngineWidgets import QWebEngineView
    widget = QWebEngineView()
    # The preview page is a local service, not a browser: it must never be reachable
    # from anything but this view, and it has no business opening windows or asking for
    # permissions.
    widget.page().setBackgroundColor(Qt.white)
    settings = widget.settings()
    try:
        from PyQt5.QtWebEngineWidgets import QWebEngineSettings
        settings.setAttribute(QWebEngineSettings.JavascriptCanOpenWindows, False)
        settings.setAttribute(QWebEngineSettings.LocalContentCanAccessRemoteUrls, True)
        settings.setAttribute(QWebEngineSettings.ShowScrollBars, False)
    except Exception:
        pass
    return widget


def url(reply):
    """The page URL of a `doStartPreview` reply, or None if it did not name one.

    Tinymist answers with `staticServerPort` (and `staticServerAddr`, which its own VS
    Code client does not read). The static and data planes share one port by default, so
    the page and the WebSocket it opens come from the same address.
    """
    if not isinstance(reply, dict):
        return None
    address = reply.get("staticServerAddr")
    if isinstance(address, str) and address:
        return QUrl(f"http://{address}/")
    port = reply.get("staticServerPort")
    if isinstance(port, int) and port > 0:
        return QUrl(f"http://127.0.0.1:{port}/")
    return None
