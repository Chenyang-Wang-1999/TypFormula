@echo off
setlocal
set "demoFile=%~f1"
cd /d "%~dp0"
call build.cmd
if errorlevel 1 exit /b 1
if not "%~1"=="" (
  target\debug\multi-formula-demo.exe 4321 "%demoFile%"
  exit /b
)
target\debug\multi-formula-demo.exe 4321
