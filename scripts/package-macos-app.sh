#!/usr/bin/env bash
set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

mkdir -p release

APP_DIR="release/Open MusicFlow Studio.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

cat > "$APP_DIR/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>Open MusicFlow Studio (Rust)</string>
  <key>CFBundleExecutable</key>
  <string>OpenMusicFlowStudio</string>
  <key>CFBundleIdentifier</key>
  <string>org.openmusicflow.studio.rust</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Open MusicFlow Studio</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>1.1.0</string>
  <key>CFBundleVersion</key>
  <string>1.1.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

if [ -f "target/universal/release/open-musicflow-rs" ]; then
  cp "target/universal/release/open-musicflow-rs" "$APP_DIR/Contents/MacOS/OpenMusicFlowStudio"
elif [ -f "target/release/open-musicflow-rs" ]; then
  cp "target/release/open-musicflow-rs" "$APP_DIR/Contents/MacOS/OpenMusicFlowStudio"
fi
chmod +x "$APP_DIR/Contents/MacOS/OpenMusicFlowStudio"

cat > "release/Install-to-Applications.command" <<'EOF'
#!/usr/bin/env bash
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
APP_NAME="Open MusicFlow Studio.app"
TARGET_DIR="/Applications"
if [ ! -w "$TARGET_DIR" ]; then
  TARGET_DIR="$HOME/Applications"
  mkdir -p "$TARGET_DIR"
fi

echo "Installing native Rust '$APP_NAME' to $TARGET_DIR ..."
xattr -cr "$DIR/$APP_NAME" 2>/dev/null || true
rm -rf "$TARGET_DIR/$APP_NAME"
cp -R "$DIR/$APP_NAME" "$TARGET_DIR/$APP_NAME"
xattr -cr "$TARGET_DIR/$APP_NAME" 2>/dev/null || true

echo "✅ Installed '$TARGET_DIR/$APP_NAME'!"
open "$TARGET_DIR/$APP_NAME"
EOF
chmod +x "release/Install-to-Applications.command"

cat > "release/Debug-Open-MusicFlow.command" <<'EOF'
#!/usr/bin/env bash
DIR="$(cd "$(dirname "$0")" && pwd)"
APP_BIN="$DIR/Open MusicFlow Studio.app/Contents/MacOS/OpenMusicFlowStudio"
if [ ! -x "$APP_BIN" ] && [ -x "/Applications/Open MusicFlow Studio.app/Contents/MacOS/OpenMusicFlowStudio" ]; then
  APP_BIN="/Applications/Open MusicFlow Studio.app/Contents/MacOS/OpenMusicFlowStudio"
fi
echo "=================================================================="
echo " Open MusicFlow Studio — Live Debug Console"
echo " Debug log files:"
echo "   - /tmp/open-musicflow-debug.log"
echo "   - ~/Library/Logs/OpenMusicFlowStudio/debug.log"
echo "   - http://127.0.0.1:4741/api/debug-log"
echo "=================================================================="
xattr -cr "$DIR/Open MusicFlow Studio.app" 2>/dev/null || true
"$APP_BIN"
EOF
chmod +x "release/Debug-Open-MusicFlow.command"

echo "✅ Packaged native Rust macOS bundle at: $APP_DIR"

