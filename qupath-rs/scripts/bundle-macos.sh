#!/usr/bin/env bash
# Builds a release and wraps it in dist/QuPath-rs.app (macOS).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --bin qupath-rs
APP=dist/QuPath-rs.app
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/qupath-rs "$APP/Contents/MacOS/qupath-rs"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>QuPath-rs</string>
  <key>CFBundleDisplayName</key><string>QuPath-rs</string>
  <key>CFBundleIdentifier</key><string>org.qupath-rs.qupath-rs</string>
  <key>CFBundleExecutable</key><string>qupath-rs</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
echo "Built $APP"
