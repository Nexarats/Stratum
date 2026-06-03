#!/usr/bin/env bash
# Stratum Terminal — Installation Script
# https://github.com/nexarats/stratum
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/nexarats/stratum/main/install.sh | bash
#
# This script downloads the latest Stratum release binary and
# installs it to ~/.local/bin (or /usr/local/bin with sudo).

set -euo pipefail

REPO="nexarats/stratum"
INSTALL_DIR="${STRATUM_INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="stratum"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${CYAN}▸${NC} $1"; }
ok()    { echo -e "${GREEN}✓${NC} $1"; }
warn()  { echo -e "${YELLOW}⚠${NC} $1"; }
err()   { echo -e "${RED}✗${NC} $1" >&2; exit 1; }

echo -e "${BOLD}"
echo "  ╔═══════════════════════════════════════════╗"
echo "  ║  Stratum Terminal Installer               ║"
echo "  ║  The terminal that understands what       ║"
echo "  ║  you're doing.                            ║"
echo "  ╚═══════════════════════════════════════════╝"
echo -e "${NC}"

# Detect platform
detect_platform() {
    local os arch target

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)   os="unknown-linux-gnu" ;;
        Darwin)  os="apple-darwin" ;;
        *)       err "Unsupported OS: $os. Use Windows releases from GitHub." ;;
    esac

    case "$arch" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)            err "Unsupported architecture: $arch" ;;
    esac

    target="${arch}-${os}"
    echo "$target"
}

# Get latest release tag
get_latest_version() {
    local version
    version=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

    if [ -z "$version" ]; then
        err "Could not determine latest version. Check https://github.com/${REPO}/releases"
    fi
    echo "$version"
}

# Download and install
install() {
    local target version url tmp_dir

    target=$(detect_platform)
    info "Detected platform: ${BOLD}${target}${NC}"

    info "Fetching latest release..."
    version=$(get_latest_version)
    ok "Latest version: ${BOLD}${version}${NC}"

    url="https://github.com/${REPO}/releases/download/${version}/stratum-${target}.tar.gz"
    info "Downloading ${url}..."

    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    if ! curl -fsSL "$url" -o "${tmp_dir}/stratum.tar.gz"; then
        err "Download failed. Check the release exists at: https://github.com/${REPO}/releases"
    fi

    # Extract
    tar xzf "${tmp_dir}/stratum.tar.gz" -C "$tmp_dir"

    # Install
    mkdir -p "$INSTALL_DIR"
    mv "${tmp_dir}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    ok "Installed to ${BOLD}${INSTALL_DIR}/${BINARY_NAME}${NC}"

    # Check PATH
    if ! echo "$PATH" | tr ':' '\n' | grep -q "^${INSTALL_DIR}$"; then
        warn "${INSTALL_DIR} is not in your PATH."
        echo ""
        echo "  Add it with:"
        echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
        echo ""
        echo "  Or add to your shell config:"
        echo "    echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc"
        echo ""
    fi

    # Verify
    if command -v "$BINARY_NAME" &>/dev/null; then
        echo ""
        ok "Stratum ${version} installed successfully!"
        echo ""
        echo -e "  Run ${BOLD}stratum${NC} to start."
        echo -e "  Run ${BOLD}stratum --help${NC} for options."
        echo ""
    else
        echo ""
        ok "Binary installed. Add ${INSTALL_DIR} to PATH, then run ${BOLD}stratum${NC}."
    fi
}

install
