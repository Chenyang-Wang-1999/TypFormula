"""Resource and helper locations shared by source and frozen builds."""
import os
import sys
from pathlib import Path


def frozen():
    return bool(getattr(sys, "frozen", False))


def resource_root():
    if frozen():
        return Path(sys._MEIPASS)
    return Path(__file__).resolve().parent.parent


def helper(name, variable, target):
    override = os.environ.get(variable)
    if override:
        return Path(override)
    root = resource_root()
    if frozen():
        return root / "bin" / name
    release = root / "target" / target / "release" / name
    return release if release.is_file() else root / "target" / target / "debug" / name


def bundled_tinymist():
    path = resource_root() / "bin" / "tinymist.exe"
    return path if frozen() and path.is_file() else None


def default_workspace():
    if frozen():
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / ".local" / "share"))
        return base / "TypFormula" / "workspace"
    return resource_root() / "workspace"
