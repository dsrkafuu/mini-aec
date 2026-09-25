@echo off
setlocal

call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul
if errorlevel 1 exit /b %errorlevel%

set "LIBCLANG_PATH=%VSINSTALLDIR%VC\Tools\Llvm\x64\bin"
cargo %*
set "rustAnalyzerExitCode=%errorlevel%"
endlocal & exit /b %rustAnalyzerExitCode%
