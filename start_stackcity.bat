@echo off
chcp 65001 >nul
cd /d "%~dp0"
if exist "target\release\stackcity-rt.exe" (
    echo 正在啟動《StackCity RT（像素疊界）》...
    start "" "target\release\stackcity-rt.exe" --data-dir ".\worlds"
) else (
    echo 找不到已編譯的執行檔，正在透過 Cargo 建置並啟動...
    cargo run --release -- --data-dir ".\worlds"
)
