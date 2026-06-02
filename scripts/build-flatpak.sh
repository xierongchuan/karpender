#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="com.github.xierongchuan.karpender"
VERSION="$(grep -m1 '^version = ' "$ROOT_DIR/Cargo.toml" | sed -E 's/version = "([^"]+)"/\1/')"
MANIFEST="$ROOT_DIR/packaging/flatpak/$APP_ID.json"
CARGO_SOURCES="$ROOT_DIR/packaging/flatpak/cargo-sources.json"
BUILD_DIR="$ROOT_DIR/target/flatpak/build"
REPO_DIR="$ROOT_DIR/target/flatpak/repo"
DIST_DIR="$ROOT_DIR/dist"
BUNDLE="$DIST_DIR/Karpender-$VERSION.flatpak"
FLATHUB_URL="https://flathub.org/repo/flathub.flatpakrepo"
SCREENSHOTS_URL="https://raw.githubusercontent.com/xierongchuan/karpender/master/data/screenshots"

ensure_user_flathub_remote() {
  if flatpak remotes --user --columns=name | grep -Fxq flathub; then
    return
  fi

  echo "Adding Flathub remote for the user Flatpak installation..."
  flatpak remote-add --user --if-not-exists flathub "$FLATHUB_URL"
}

refresh_appstream_metadata() {
  local compose_root="$ROOT_DIR/target/flatpak/appstream-compose"
  local media_root="$ROOT_DIR/target/flatpak/appstream-media"
  local appstream_root="$ROOT_DIR/target/flatpak/appstream-ref"
  local appstream2_root="$ROOT_DIR/target/flatpak/appstream2-ref"
  local catalog_xml_gz="$compose_root/share/swcatalog/xml/flatpak.xml.gz"

  rm -rf "$compose_root" "$media_root" "$appstream_root" "$appstream2_root"
  mkdir -p "$compose_root" "$media_root" "$appstream_root/icons" "$appstream2_root/icons"

  appstreamcli compose \
    --no-net \
    --prefix=/ \
    --origin=flatpak \
    --result-root="$compose_root" \
    --media-dir="$media_root" \
    --media-baseurl="$SCREENSHOTS_URL" \
    "$BUILD_DIR/files"

  if [ ! -f "$catalog_xml_gz" ]; then
    echo "missing composed AppStream catalog: $catalog_xml_gz" >&2
    exit 1
  fi

  cp "$catalog_xml_gz" "$appstream_root/appstream.xml.gz"
  gzip -dc "$catalog_xml_gz" > "$appstream2_root/appstream.xml"

  if [ -d "$compose_root/share/swcatalog/icons/flatpak" ]; then
    cp -a "$compose_root/share/swcatalog/icons/flatpak/." "$appstream_root/icons/"
    cp -a "$compose_root/share/swcatalog/icons/flatpak/." "$appstream2_root/icons/"
  fi

  ostree --repo="$REPO_DIR" commit \
    --branch=appstream/x86_64 \
    --subject="Update AppStream metadata" \
    "$appstream_root"
  ostree --repo="$REPO_DIR" commit \
    --branch=appstream2/x86_64 \
    --subject="Update AppStream 2 metadata" \
    "$appstream2_root"
}

if ! command -v flatpak-builder >/dev/null 2>&1; then
  cat >&2 <<EOF
flatpak-builder is required.

Install it first, for example on Fedora:
  sudo dnf install flatpak-builder
EOF
  exit 2
fi

ensure_user_flathub_remote

"$ROOT_DIR/scripts/generate-flatpak-cargo-sources.py" \
  "$ROOT_DIR/Cargo.lock" \
  -o "$CARGO_SOURCES"

mkdir -p "$DIST_DIR" "$REPO_DIR"

flatpak-builder \
  --force-clean \
  --user \
  --install-deps-from=flathub \
  --compose-url-policy=full \
  --mirror-screenshots-url="$SCREENSHOTS_URL" \
  --repo="$REPO_DIR" \
  "$BUILD_DIR" \
  "$MANIFEST"

refresh_appstream_metadata

flatpak build-bundle \
  --runtime-repo="$FLATHUB_URL" \
  "$REPO_DIR" \
  "$BUNDLE" \
  "$APP_ID" \
  stable

echo "Flatpak bundle written to: $BUNDLE"
