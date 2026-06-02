#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="com.github.xierongchuan.karpender"
APP_NAME="Karpender"
VERSION="$(grep -m1 '^version = ' "$ROOT_DIR/Cargo.toml" | sed -E 's/version = "([^"]+)"/\1/')"
ARCH="${ARCH:-$(uname -m)}"
APPDIR="$ROOT_DIR/target/appimage/$APP_NAME.AppDir"
DIST_DIR="$ROOT_DIR/dist"
TOOLS_DIR="$ROOT_DIR/target/appimage-tools"
OUT="$DIST_DIR/$APP_NAME-$VERSION-$ARCH.AppImage"

case "$ARCH" in
  x86_64 | amd64)
    APPIMAGE_ARCH="x86_64"
    ;;
  aarch64 | arm64)
    APPIMAGE_ARCH="aarch64"
    ;;
  *)
    APPIMAGE_ARCH="$ARCH"
    ;;
esac

APPIMAGETOOL="$TOOLS_DIR/appimagetool-$APPIMAGE_ARCH.AppImage"
APPIMAGETOOL_URL="https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-$APPIMAGE_ARCH.AppImage"

require_file() {
  if [ ! -f "$1" ]; then
    echo "missing required file: $1" >&2
    exit 1
  fi
}

require_file "$ROOT_DIR/data/$APP_ID.desktop"
require_file "$ROOT_DIR/data/$APP_ID.metainfo.xml"
require_file "$ROOT_DIR/data/icons/$APP_ID.svg"
require_file "$ROOT_DIR/packaging/appimage/AppRun"

# This script only builds a local AppDir/AppImage artifact. It does not install
# the app into /usr, register desktop files, or modify the user's system.
cargo build --release --locked --manifest-path "$ROOT_DIR/Cargo.toml"

rm -rf "$APPDIR"
mkdir -p "$APPDIR" "$DIST_DIR" "$TOOLS_DIR"

mkdir -p \
  "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/metainfo" \
  "$APPDIR/usr/share/icons/hicolor/scalable/apps"

cp "$ROOT_DIR/target/release/karpender" "$APPDIR/usr/bin/karpender"
cp "$ROOT_DIR/packaging/appimage/AppRun" "$APPDIR/AppRun"
cp "$ROOT_DIR/data/$APP_ID.desktop" "$APPDIR/usr/share/applications/$APP_ID.desktop"
cp "$ROOT_DIR/data/$APP_ID.metainfo.xml" "$APPDIR/usr/share/metainfo/$APP_ID.metainfo.xml"
cp "$ROOT_DIR/data/$APP_ID.metainfo.xml" "$APPDIR/usr/share/metainfo/$APP_ID.appdata.xml"
cp "$ROOT_DIR/data/icons/$APP_ID.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"
chmod 755 "$APPDIR/usr/bin/karpender" "$APPDIR/AppRun"
chmod 644 \
  "$APPDIR/usr/share/applications/$APP_ID.desktop" \
  "$APPDIR/usr/share/metainfo/$APP_ID.metainfo.xml" \
  "$APPDIR/usr/share/metainfo/$APP_ID.appdata.xml" \
  "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"

cp "$APPDIR/usr/share/applications/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
cp "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg" "$APPDIR/$APP_ID.svg"
ln -sfn "$APP_ID.svg" "$APPDIR/.DirIcon"

if command -v linuxdeploy >/dev/null 2>&1; then
  export OUTPUT="$OUT"
  args=(
    --appdir "$APPDIR"
    --executable "$APPDIR/usr/bin/karpender"
    --desktop-file "$APPDIR/usr/share/applications/$APP_ID.desktop"
    --icon-file "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"
    --output appimage
  )

  if command -v linuxdeploy-plugin-gtk >/dev/null 2>&1; then
    args+=(--plugin gtk)
  else
    echo "warning: linuxdeploy-plugin-gtk not found; GTK/libadwaita libraries may not be bundled" >&2
  fi

  linuxdeploy "${args[@]}"
elif command -v appimagetool >/dev/null 2>&1; then
  echo "warning: appimagetool fallback does not bundle GTK/libadwaita dependencies" >&2
  appimagetool "$APPDIR" "$OUT"
else
  if [ ! -x "$APPIMAGETOOL" ]; then
    echo "No AppImage tool found in PATH; downloading local appimagetool to:" >&2
    echo "  $APPIMAGETOOL" >&2
    if command -v curl >/dev/null 2>&1; then
      curl --fail --location --output "$APPIMAGETOOL" "$APPIMAGETOOL_URL"
    elif command -v wget >/dev/null 2>&1; then
      wget --output-document "$APPIMAGETOOL" "$APPIMAGETOOL_URL"
    else
      cat >&2 <<EOF
AppDir prepared at:
  $APPDIR

No AppImage tool found, and neither curl nor wget is available.
Put appimagetool in PATH or at:
  $APPIMAGETOOL
Then rerun:
  scripts/build-appimage.sh
EOF
      exit 2
    fi
    chmod 755 "$APPIMAGETOOL"
  fi

  echo "warning: appimagetool fallback does not bundle GTK/libadwaita dependencies" >&2
  APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" "$APPDIR" "$OUT"
fi

echo "AppImage written to: $OUT"
