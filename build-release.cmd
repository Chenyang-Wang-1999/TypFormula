@echo off
setlocal
cd /d "%~dp0"
@REM Set your python path by $env:TYPFORMULA_PYTHON = (Resolve-Path your/venv/path/python.exe).Path
if defined TYPFORMULA_PYTHON (
  "%TYPFORMULA_PYTHON%" tools\build_release.py %*
) else (
  python tools\build_release.py %*
)
exit /b %errorlevel%
