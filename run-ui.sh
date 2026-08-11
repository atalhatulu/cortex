#!/bin/bash
ACTION=$1
shift
TARGETS=("$@")

if [ -f "/home/teha/Documents/GitHub/cortex/ui/src-tauri/target/release/app" ]; then
    APP_BIN="/home/teha/Documents/GitHub/cortex/ui/src-tauri/target/release/app"
elif [ -f "/home/teha/Documents/GitHub/cortex/ui/src-tauri/target/debug/app" ]; then
    APP_BIN="/home/teha/Documents/GitHub/cortex/ui/src-tauri/target/debug/app"
else
    # Fallback to dev mode if no binary exists (unlikely if they have run it once)
    cd /home/teha/Documents/GitHub/cortex/ui
    npm run tauri dev -- "$ACTION" "${TARGETS[@]}"
    exit 0
fi

"$APP_BIN" "$ACTION" "${TARGETS[@]}"
