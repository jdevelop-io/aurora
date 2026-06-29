#!/bin/sh
# Aurora installer.
#
#   curl -fsSL https://raw.githubusercontent.com/jdevelop-io/aurora/main/install.sh | sh
#
# Recognized environment variables:
#   AURORA_VERSION       version to install (e.g. v0.2.0). Default: latest release.
#   AURORA_INSTALL_DIR   install directory. Default: $HOME/.local/bin.
#   AURORA_SKIP_CHECKSUM set to 1 to skip SHA-256 verification (not recommended;
#                        needed only for releases published before checksums).
set -eu

REPO="jdevelop-io/aurora"
BIN="aurora"

INSTALL_DIR="${AURORA_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${AURORA_VERSION:-latest}"

# --- output ----------------------------------------------------------------
info() { printf '  %s\n' "$1"; }
warn() { printf '\033[33m!\033[0m %s\n' "$1" >&2; }
err()  { printf '\033[31mError:\033[0m %s\n' "$1" >&2; exit 1; }

# --- download helpers (curl or wget) ---------------------------------------
http_get() { # url -> stdout
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$1"
  else
    err "curl or wget is required."
  fi
}

http_download() { # url dest
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$2" "$1"
  else
    err "curl or wget is required."
  fi
}

# --- platform detection ----------------------------------------------------
detect_platform() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)  os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    *) err "Unsupported OS for this installer: $os. Download an archive from https://github.com/$REPO/releases" ;;
  esac

  case "$arch" in
    x86_64 | amd64)   arch_part="x86_64" ;;
    aarch64 | arm64)  arch_part="aarch64" ;;
    *) err "Unsupported architecture: $arch" ;;
  esac

  TARGET="${arch_part}-${os_part}"
}

# --- version resolution ----------------------------------------------------
resolve_version() {
  [ "$VERSION" = "latest" ] || return 0
  info "Resolving the latest version..."
  VERSION="$(http_get "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | head -1 \
    | sed -E 's/.*"tag_name"[ ]*:[ ]*"([^"]+)".*/\1/')"
  [ -n "$VERSION" ] || err "could not determine the latest version (GitHub rate limit?). Set AURORA_VERSION."
}

# --- integrity -------------------------------------------------------------
sha256_of() { # file -> hex digest
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    err "no sha256 tool (sha256sum or shasum) available to verify the download."
  fi
}

verify_checksum() { # file sum_url
  if [ "${AURORA_SKIP_CHECKSUM:-0}" = "1" ]; then
    warn "Checksum verification skipped (AURORA_SKIP_CHECKSUM=1)."
    return 0
  fi
  info "Verifying checksum..."
  expected="$(http_get "$2" 2>/dev/null | awk '{print $1}' | head -1)"
  [ -n "$expected" ] || err "could not download the checksum ($2). This release may predate checksums; re-run with AURORA_SKIP_CHECKSUM=1 to bypass at your own risk."
  actual="$(sha256_of "$1")"
  [ "$expected" = "$actual" ] || err "checksum mismatch (expected $expected, got $actual). Aborting."
  info "Checksum OK."
}

# --- installation ----------------------------------------------------------
install_binary() {
  asset="${BIN}-${VERSION}-${TARGET}.tar.gz"
  url="https://github.com/$REPO/releases/download/$VERSION/$asset"

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  info "Downloading $asset ($VERSION)..."
  http_download "$url" "$tmp/$asset" || err "download failed: $url"

  verify_checksum "$tmp/$asset" "${url}.sha256"

  tar -xzf "$tmp/$asset" -C "$tmp" || err "failed to extract the archive."

  binpath="$(find "$tmp" -type f -name "$BIN" 2>/dev/null | head -1)"
  [ -n "$binpath" ] || err "binary '$BIN' not found in the archive."

  mkdir -p "$INSTALL_DIR"
  cp "$binpath" "$INSTALL_DIR/$BIN"
  chmod 755 "$INSTALL_DIR/$BIN"

  info "Installed: $INSTALL_DIR/$BIN"
}

# --- post-install hints ----------------------------------------------------
post_install() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*)
      printf '\n\033[32maurora %s is ready.\033[0m Run: %s --help\n' "$VERSION" "$BIN"
      ;;
    *)
      printf '\n\033[32maurora %s is installed.\033[0m\n' "$VERSION"
      warn "$INSTALL_DIR is not on your PATH."
      info "Add this line to your ~/.bashrc or ~/.zshrc:"
      info "  export PATH=\"$INSTALL_DIR:\$PATH\""
      ;;
  esac
}

main() {
  detect_platform
  resolve_version
  install_binary
  post_install
}

main
