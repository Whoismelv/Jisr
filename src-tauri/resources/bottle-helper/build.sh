#!/usr/bin/env bash
# build.sh — build the bottle helper as a macOS .app bundle
#
# The helper is installed inside a bottle and registers itself as the handler
# for Apple Events (Open/Print/URL) so Windows applications can hand files and
# links to the macOS side.
#
# Requirements: Xcode Command Line Tools (or full Xcode) on the Mac.
# Build:        bash build.sh
# Output:       dist/Jisr Helper.app
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${HERE}/dist"
APP="${OUT}/Jisr Helper.app"

log() { printf '\033[1;34m[helper]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[helper]\033[0m %s\n' "$*" >&2; exit 1; }

command -v swiftc >/dev/null 2>&1 || die "Xcode CLT required (xcode-select --install)"

rm -rf "${APP}"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"

log "compiling…"
swiftc -O \
    -o "${APP}/Contents/MacOS/Helper" \
    "${HERE}/BottleHelper.swift"

cat > "${APP}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Jisr Helper</string>
  <key>CFBundleDisplayName</key><string>Jisr Helper</string>
  <key>CFBundleExecutable</key><string>Helper</string>
  <key>CFBundleIdentifier</key><string>com.jisr.bottlehelper.default</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1.0</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>LSBackgroundOnly</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSAppleEventsUsageDescription</key><string>يعيد توجيه أحداث فتح/طباعة الملفات إلى جسر.</string>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSItemContentTypes</key><array><string>public.item</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST

codesign --force --sign - "${APP}" >/dev/null 2>&1 || log "warning: ad-hoc signing failed (fine without tooling)"

log "done: ${APP}"
log "install into a bottle: copy it to <prefix>/Applications/ as Jisr Helper (<tag>).app"
