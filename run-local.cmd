@echo off
cd /d D:\Python\projects\lnc-newsman
rem Local demo key only: server is restricted to loopback, never use for deployment.
set MANAGEMENT_KEY=dev-only-key
set LNC_API_KEYS=dev-only-api-key
set BIND_ADDRESS=127.0.0.1
set COOKIE_SECURE=false
set DATA_DIR=./.local-data
set PORT=8000
set RUST_LOG=lnc_newsman=info,tower_http=info
"C:\Users\ngocl\.cargo\bin\cargo.exe" run --quiet
