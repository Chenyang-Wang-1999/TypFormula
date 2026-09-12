@echo off
setlocal
cd /d "%~dp0"
if defined TYPFORMULA_PYTHON (
  "%TYPFORMULA_PYTHON%" -m desktop %*
) else (
  pythonw -m desktop %*
)
