#!/usr/bin/env bash
# ╔══════════════════════════════════════════════════════════════════════╗
# ║  Stratum Terminal — Cross-Platform Installer                        ║
# ║  Installs stratum and nos-shell binaries to your system PATH.       ║
# ║                                                                     ║
# ║  Usage:                                                             ║
# ║    curl -fsSL https://nexarats.com/install.sh | bash                ║
# ║    OR                                                               ║
# ║    ./scripts/install.sh                                             ║
# ╚══════════════════════════════════════════════════════════════════════╝

set -euo pipefail

# --- Configuration ---
APP_NAME="stratum"
VERSION="0.1.0-alpha.1"
INSTALL_DIR="${STRATUM_INSTALL_DIR:-$HOME/.stratum}"
BIN_DIR="$INSTALL_DIR/bin"
CONFIG_DIR="$INSTALL_DIR/config"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

info()  { echo -e "${CYAN}▸${NC} $1"; }
ok()    { echo -e "${GREEN}✓${NC} $1"; }
warn()  { echo -e "${YELLOW}⚠${NC} $1"; }
error() { echo -e "${RED}✗${NC} $1"; exit 1; }

# --- OS Detection ---
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="macos" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
        *) error "Unsupported OS: $(uname -s)" ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) error "Unsupported architecture: $(uname -m)" ;;
    esac

    echo "${os}-${arch}"
}

# --- Build from source ---
build_from_source() {
    info "Building Stratum from source..."

    # Check Rust toolchain
    if ! command -v cargo &> /dev/null; then
        warn "Rust not found. Installing via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi

    local rust_version
    rust_version=$(rustc --version | grep -oP '\d+\.\d+')
    info "Rust version: $rust_version"

    # Find the project root (script is at stratum/scripts/install.sh)
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local project_root
    project_root="$(cd "$script_dir/.." && pwd)"

    if [ ! -f "$project_root/Cargo.toml" ]; then
        error "Cannot find Cargo.toml. Run this script from the stratum directory."
    fi

    # Build release
    info "Compiling stratum (release mode)..."
    cd "$project_root"
    cargo build --release 2>&1 | tail -5

    # Also build nos-shell if it exists as a sibling
    local nos_root="$(cd "$project_root/.." && pwd)/nos-shell"
    if [ -d "$nos_root" ] && [ -f "$nos_root/Cargo.toml" ]; then
        info "Compiling nos-shell (release mode)..."
        cd "$nos_root"
        cargo build --release 2>&1 | tail -5
    fi

    cd "$project_root"
}

# --- Install binaries ---
install_binaries() {
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local project_root
    project_root="$(cd "$script_dir/.." && pwd)"

    mkdir -p "$BIN_DIR"
    mkdir -p "$CONFIG_DIR"

    # Copy stratum binary
    local stratum_bin="$project_root/target/release/$APP_NAME"
    if [ -f "$stratum_bin" ]; then
        cp "$stratum_bin" "$BIN_DIR/"
        chmod +x "$BIN_DIR/$APP_NAME"
        ok "Installed stratum → $BIN_DIR/$APP_NAME"
    elif [ -f "${stratum_bin}.exe" ]; then
        cp "${stratum_bin}.exe" "$BIN_DIR/"
        ok "Installed stratum → $BIN_DIR/${APP_NAME}.exe"
    else
        error "stratum binary not found at $stratum_bin. Build first with 'cargo build --release'."
    fi

    # Copy nos-shell if available
    local nos_bin="$(cd "$project_root/.." && pwd)/nos-shell/target/release/nos-shell"
    if [ -f "$nos_bin" ]; then
        cp "$nos_bin" "$BIN_DIR/"
        chmod +x "$BIN_DIR/nos-shell"
        ok "Installed nos-shell → $BIN_DIR/nos-shell"
    elif [ -f "${nos_bin}.exe" ]; then
        cp "${nos_bin}.exe" "$BIN_DIR/"
        ok "Installed nos-shell → $BIN_DIR/nos-shell.exe"
    else
        warn "nos-shell not found — stratum will use system shell"
    fi

    # Create default config if it doesn't exist
    if [ ! -f "$CONFIG_DIR/stratum.toml" ]; then
        cat > "$CONFIG_DIR/stratum.toml" << 'EOF'
# Stratum Terminal Configuration
# https://nexarats.com/stratum

[terminal]
# Shell to use (leave empty for auto-detect: nos-shell > system default)
# shell = "/bin/bash"
font_size = 14.0

[appearance]
# Theme: "dark" | "light" | "monokai" | "dracula" | "nord"
theme = "dark"
# Background opacity (0.0 - 1.0)
opacity = 0.95

[ai]
# Default AI provider (set API key with: /ai-set-key <provider> <key>)
# provider = "openai"
# model = "gpt-4o-mini"

[keybindings]
# Custom keybindings (Ctrl+Shift prefix)
# new_tab = "T"
# close_pane = "W"
# split_vertical = "E"
# split_horizontal = "O"
# copy = "C"
# paste = "V"
EOF
        ok "Created default config → $CONFIG_DIR/stratum.toml"
    fi
}

# --- PATH setup ---
setup_path() {
    local shell_rc=""
    local current_shell
    current_shell="$(basename "$SHELL" 2>/dev/null || echo "bash")"

    case "$current_shell" in
        zsh)  shell_rc="$HOME/.zshrc" ;;
        bash) shell_rc="$HOME/.bashrc" ;;
        fish) shell_rc="$HOME/.config/fish/config.fish" ;;
        *)    shell_rc="$HOME/.profile" ;;
    esac

    # Check if already in PATH
    if echo "$PATH" | grep -q "$BIN_DIR"; then
        ok "PATH already contains $BIN_DIR"
        return
    fi

    if [ "$current_shell" = "fish" ]; then
        echo "set -gx PATH $BIN_DIR \$PATH" >> "$shell_rc"
    else
        echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$shell_rc"
    fi

    ok "Added $BIN_DIR to PATH in $shell_rc"
    warn "Restart your shell or run: source $shell_rc"
}

# --- Desktop entry (Linux) ---
create_desktop_entry() {
    if [ "$(uname -s)" != "Linux" ]; then
        return
    fi

    local desktop_dir="$HOME/.local/share/applications"
    mkdir -p "$desktop_dir"

    cat > "$desktop_dir/stratum.desktop" << EOF
[Desktop Entry]
Name=Stratum Terminal
Comment=The terminal that understands what you're doing
Exec=$BIN_DIR/stratum
Icon=utilities-terminal
Type=Application
Categories=System;TerminalEmulator;
Terminal=false
StartupNotify=true
Keywords=terminal;shell;console;command;
EOF

    ok "Created desktop entry → $desktop_dir/stratum.desktop"
}

# --- macOS app bundle ---
create_macos_app() {
    if [ "$(uname -s)" != "Darwin" ]; then
        return
    fi

    local app_dir="$HOME/Applications/Stratum.app"
    local contents_dir="$app_dir/Contents"
    local macos_dir="$contents_dir/MacOS"

    mkdir -p "$macos_dir"

    # Info.plist
    cat > "$contents_dir/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Stratum</string>
    <key>CFBundleDisplayName</key>
    <string>Stratum Terminal</string>
    <key>CFBundleIdentifier</key>
    <string>com.nexarats.stratum</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleExecutable</key>
    <string>stratum</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

    # Symlink the binary
    ln -sf "$BIN_DIR/stratum" "$macos_dir/stratum"

    ok "Created macOS app → $app_dir"
}

# --- Main ---
main() {
    echo ""
    echo -e "${BOLD}╔══════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║  ${CYAN}Stratum Terminal${NC} ${BOLD}Installer  v$VERSION  ║${NC}"
    echo -e "${BOLD}╚══════════════════════════════════════════╝${NC}"
    echo ""

    local platform
    platform=$(detect_platform)
    info "Platform: $platform"
    info "Install directory: $INSTALL_DIR"
    echo ""

    # Build
    build_from_source

    # Install
    echo ""
    info "Installing..."
    install_binaries

    # PATH
    echo ""
    setup_path

    # Platform-specific
    create_desktop_entry
    create_macos_app

    # Summary
    echo ""
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════${NC}"
    echo -e "${GREEN}${BOLD}  Stratum installed successfully! 🚀${NC}"
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════${NC}"
    echo ""
    echo -e "  Binary:  ${CYAN}$BIN_DIR/stratum${NC}"
    echo -e "  Config:  ${CYAN}$CONFIG_DIR/stratum.toml${NC}"
    echo ""
    echo -e "  Run:     ${BOLD}stratum${NC}"
    echo -e "  Help:    ${BOLD}stratum --help${NC}"
    echo ""
}

main "$@"
