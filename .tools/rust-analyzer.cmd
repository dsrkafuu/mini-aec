@echo off
setlocal

call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul
if errorlevel 1 exit /b %errorlevel%

set "PYTHONPATH=%~dp0python"
set "LIBCLANG_PATH=%~dp0python\clang\native"
set "PATH=%~dp0bin;%~dp0python\bin;%PATH%;C:\PortableSDKs\PortableGit\usr\bin"

cargo %*
set "rustAnalyzerExitCode=%errorlevel%"
endlocal & exit /b %rustAnalyzerExitCode%
