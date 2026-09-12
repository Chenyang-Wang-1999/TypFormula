@echo off
setlocal
cd /d "%~dp0"
if defined TYPFORMULA_PYTHON (
  "%TYPFORMULA_PYTHON%" tools\build_release.py %*
) else (
  python tools\build_release.py %*
)
exit /b %errorlevel%
