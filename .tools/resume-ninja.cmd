@echo off
setlocal

rem Cargo and Meson resume the current native build automatically. Keep this
rem helper independent from repository paths and Cargo build-directory hashes.
call "%~dp0cargo-webrtc.cmd" check -p mini-aec-engine
