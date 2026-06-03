# ╔══════════════════════════════════════════════════════════════════════╗
# ║  Stratum Terminal — Windows Installer (PowerShell)                  ║
# ║  Installs stratum and nos-shell binaries to your system PATH.       ║
# ║                                                                     ║
# ║  Usage (admin not required):                                        ║
# ║    .\scripts\install.ps1                                            ║
# ║    OR (remote):                                                     ║
# ║    iwr -useb https://nexarats.com/install.ps1 | iex                 ║
# ╚══════════════════════════════════════════════════════════════════════╝

$ErrorActionPreference = "Stop"

# --- Configuration ---
$AppName     = "stratum"
$Version     = "0.1.0-alpha.1"
$InstallDir  = if ($env:STRATUM_INSTALL_DIR) { $env:STRATUM_INSTALL_DIR } else { "$env:USERPROFILE\.stratum" }
$BinDir      = "$InstallDir\bin"
$ConfigDir   = "$InstallDir\config"

function Write-Info  { param($msg) Write-Host "  ▸ " -ForegroundColor Cyan -NoNewline; Write-Host $msg }
function Write-Ok    { param($msg) Write-Host "  ✓ " -ForegroundColor Green -NoNewline; Write-Host $msg }
function Write-Warn  { param($msg) Write-Host "  ⚠ " -ForegroundColor Yellow -NoNewline; Write-Host $msg }
function Write-Err   { param($msg) Write-Host "  ✗ " -ForegroundColor Red -NoNewline; Write-Host $msg; exit 1 }

# --- Locate project root ---
function Get-ProjectRoot {
    $scriptDir = Split-Path -Parent $MyInvocation.ScriptName
    if (-not $scriptDir) {
        $scriptDir = Get-Location
    }
    $root = Resolve-Path (Join-Path $scriptDir "..")
    if (-not (Test-Path (Join-Path $root "Cargo.toml"))) {
        Write-Err "Cannot find Cargo.toml. Run this script from the stratum directory."
    }
    return $root.Path
}

# --- Build from source ---
function Build-FromSource {
    Write-Info "Building Stratum from source..."

    # Check Rust
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        Write-Warn "Rust not found. Please install from https://rustup.rs"
        Write-Err "cargo is required to build Stratum."
    }

    $rustVersion = (rustc --version) -replace "rustc ", ""
    Write-Info "Rust version: $rustVersion"

    $root = Get-ProjectRoot

    # Build stratum
    Write-Info "Compiling stratum (release mode)..."
    Push-Location $root
    cargo build --release
    if ($LASTEXITCODE -ne 0) { Write-Err "Build failed!" }
    Pop-Location

    # Build nos-shell if sibling exists
    $nosRoot = Join-Path (Split-Path $root -Parent) "nos-shell"
    if (Test-Path (Join-Path $nosRoot "Cargo.toml")) {
        Write-Info "Compiling nos-shell (release mode)..."
        Push-Location $nosRoot
        cargo build --release
        if ($LASTEXITCODE -ne 0) { Write-Warn "nos-shell build failed (optional)" }
        Pop-Location
    }
}

# --- Install binaries ---
function Install-Binaries {
    $root = Get-ProjectRoot

    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null

    # Copy stratum.exe
    $stratumBin = Join-Path $root "target\release\$AppName.exe"
    if (Test-Path $stratumBin) {
        Copy-Item $stratumBin -Destination $BinDir -Force
        Write-Ok "Installed stratum → $BinDir\$AppName.exe"
    } else {
        Write-Err "stratum.exe not found at $stratumBin. Build first."
    }

    # Copy nos-shell.exe if available
    $nosRoot = Join-Path (Split-Path $root -Parent) "nos-shell"
    $nosBin = Join-Path $nosRoot "target\release\nos-shell.exe"
    if (Test-Path $nosBin) {
        Copy-Item $nosBin -Destination $BinDir -Force
        Write-Ok "Installed nos-shell → $BinDir\nos-shell.exe"
    } else {
        Write-Warn "nos-shell.exe not found — stratum will use PowerShell"
    }

    # Create default config
    $configFile = Join-Path $ConfigDir "stratum.toml"
    if (-not (Test-Path $configFile)) {
        @"
# Stratum Terminal Configuration
# https://nexarats.com/stratum

[terminal]
# Shell to use (leave empty for auto-detect: nos-shell > PowerShell)
# shell = "powershell.exe"
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
"@ | Set-Content $configFile -Encoding UTF8
        Write-Ok "Created default config → $configFile"
    }
}

# --- Add to User PATH ---
function Set-UserPath {
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")

    if ($currentPath -split ";" | Where-Object { $_ -eq $BinDir }) {
        Write-Ok "PATH already contains $BinDir"
        return
    }

    $newPath = "$BinDir;$currentPath"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    $env:Path = "$BinDir;$env:Path"

    Write-Ok "Added $BinDir to User PATH"
    Write-Warn "Restart your terminal for PATH changes to take effect."
}

# --- Create Start Menu shortcut ---
function New-StartMenuShortcut {
    $startMenu = [Environment]::GetFolderPath("StartMenu")
    $shortcutPath = Join-Path $startMenu "Programs\Stratum Terminal.lnk"

    try {
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = Join-Path $BinDir "$AppName.exe"
        $shortcut.WorkingDirectory = $env:USERPROFILE
        $shortcut.Description = "The terminal that understands what you're doing"
        $shortcut.Save()
        Write-Ok "Created Start Menu shortcut"
    } catch {
        Write-Warn "Could not create Start Menu shortcut: $_"
    }
}

# --- Main ---
function Main {
    Write-Host ""
    Write-Host "  ╔══════════════════════════════════════════╗" -ForegroundColor White
    Write-Host "  ║  " -ForegroundColor White -NoNewline
    Write-Host "Stratum Terminal" -ForegroundColor Cyan -NoNewline
    Write-Host " Installer  v$Version  ║" -ForegroundColor White
    Write-Host "  ╚══════════════════════════════════════════╝" -ForegroundColor White
    Write-Host ""

    Write-Info "Platform: Windows $([System.Environment]::Is64BitOperatingSystem ? 'x64' : 'x86')"
    Write-Info "Install directory: $InstallDir"
    Write-Host ""

    # Build
    Build-FromSource

    # Install
    Write-Host ""
    Write-Info "Installing..."
    Install-Binaries

    # PATH
    Write-Host ""
    Set-UserPath

    # Start Menu
    New-StartMenuShortcut

    # Summary
    Write-Host ""
    Write-Host "  ═══════════════════════════════════════════" -ForegroundColor Green
    Write-Host "    Stratum installed successfully! 🚀" -ForegroundColor Green
    Write-Host "  ═══════════════════════════════════════════" -ForegroundColor Green
    Write-Host ""
    Write-Host "    Binary:  " -NoNewline; Write-Host "$BinDir\stratum.exe" -ForegroundColor Cyan
    Write-Host "    Config:  " -NoNewline; Write-Host "$ConfigDir\stratum.toml" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "    Run:     " -NoNewline; Write-Host "stratum" -ForegroundColor White
    Write-Host "    Help:    " -NoNewline; Write-Host "stratum --help" -ForegroundColor White
    Write-Host ""
}

Main
