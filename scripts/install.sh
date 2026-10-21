#!/bin/bash
# Builds Forked in release mode, wraps it in Forked.app and installs it in
# ~/Applications, where it shows up in Launchpad, Spotlight and the Dock.
# Re-run after each change to update the installed app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${FORKED_INSTALL_DIR:-$HOME/Applications}"
APP="$ROOT/target/Forked.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"

if ! command -v cargo >/dev/null; then
    export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
fi

echo "==> Compilation (release)"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "==> Création du bundle"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/forked" "$APP/Contents/MacOS/forked"

ICONS="$ROOT/target/icon"
if [ ! -f "$ICONS/AppIcon.icns" ] || [ "$ROOT/scripts/make_icon.swift" -nt "$ICONS/AppIcon.icns" ]; then
    echo "==> Génération de l'icône"
    rm -rf "$ICONS" && mkdir -p "$ICONS/AppIcon.iconset"
    swift "$ROOT/scripts/make_icon.swift" "$ICONS/icon_1024.png"
    for s in 16 32 128 256 512; do
        sips -z $s $s "$ICONS/icon_1024.png" --out "$ICONS/AppIcon.iconset/icon_${s}x${s}.png" >/dev/null
        d=$((s * 2))
        sips -z $d $d "$ICONS/icon_1024.png" --out "$ICONS/AppIcon.iconset/icon_${s}x${s}@2x.png" >/dev/null
    done
    iconutil -c icns "$ICONS/AppIcon.iconset" -o "$ICONS/AppIcon.icns"
fi
cp "$ICONS/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Forked</string>
    <key>CFBundleDisplayName</key><string>Forked</string>
    <key>CFBundleIdentifier</key><string>local.forked.app</string>
    <key>CFBundleExecutable</key><string>forked</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signature: required on Apple Silicon, no developer account needed.
codesign --force --deep --sign - "$APP" >/dev/null

echo "==> Installation dans $DEST"
mkdir -p "$DEST"
if pgrep -xq forked; then
    osascript -e 'quit app "Forked"' >/dev/null 2>&1 || true
    sleep 1
fi
rm -rf "$DEST/Forked.app"
cp -R "$APP" "$DEST/Forked.app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$DEST/Forked.app"

echo "✔ Forked installé : $DEST/Forked.app"
echo "  Lancez-le depuis Spotlight (⌘ Espace → Forked) ou le Launchpad."
