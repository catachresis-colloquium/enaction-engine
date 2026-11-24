#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# setup.sh — Universal setup script for enaction-engine
#
# Detects your shell, platform, and installs prerequisites.
# Then hands off to `just setup` for project-specific configuration.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/metadatastician/enaction-engine/main/setup.sh -o setup.sh && sh setup.sh
#   # or after cloning:
#   ./setup.sh
#
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath)

set -eu

# ── Colours (safe — uses symbols too per ADJUST contractile) ──
if [ -t 1 ] && command -v tput >/dev/null 2>&1; then
    RED=$(tput setaf 1 2>/dev/null || true)
    GREEN=$(tput setaf 2 2>/dev/null || true)
    YELLOW=$(tput setaf 3 2>/dev/null || true)
    CYAN=$(tput setaf 6 2>/dev/null || true)
    BOLD=$(tput bold 2>/dev/null || true)
    RESET=$(tput sgr0 2>/dev/null || true)
else
    RED="" GREEN="" YELLOW="" CYAN="" BOLD="" RESET=""
fi

ok()   { printf "  %s[OK]%s   %s\n" "$GREEN" "$RESET" "$1"; }
fail() { printf "  %s[FAIL]%s %s\n" "$RED" "$RESET" "$1"; }
warn() { printf "  %s[WARN]%s %s\n" "$YELLOW" "$RESET" "$1"; }
info() { printf "  %s[INFO]%s %s\n" "$CYAN" "$RESET" "$1"; }

# ── Shell Detection ──
detect_shell() {
    # Check the actual running shell, not just $SHELL
    CURRENT_SHELL="unknown"

    if [ -n "${BASH_VERSION:-}" ]; then CURRENT_SHELL="bash"
    elif [ -n "${ZSH_VERSION:-}" ]; then CURRENT_SHELL="zsh"
    elif [ -n "${FISH_VERSION:-}" ]; then CURRENT_SHELL="fish"
    elif [ -n "${KSH_VERSION:-}" ]; then CURRENT_SHELL="ksh"
    # Check by process name for shells that don't set version vars
    elif command -v ps >/dev/null 2>&1; then
        SHELL_PROC=$(ps -p $$ -o comm= 2>/dev/null || echo "unknown")
        case "$SHELL_PROC" in
            *dash*)    CURRENT_SHELL="dash" ;;
            *tcsh*)    CURRENT_SHELL="tcsh" ;;
            *csh*)     CURRENT_SHELL="csh" ;;
            *elvish*)  CURRENT_SHELL="elvish" ;;
            *nu*)      CURRENT_SHELL="nushell" ;;
            *oil*|*osh*) CURRENT_SHELL="oil" ;;
            *xonsh*)   CURRENT_SHELL="xonsh" ;;
            *murex*)   CURRENT_SHELL="murex" ;;
            *ion*)     CURRENT_SHELL="ion" ;;
            *hilbish*) CURRENT_SHELL="hilbish" ;;
            *oh*)      CURRENT_SHELL="oh" ;;
            *vsh*)     CURRENT_SHELL="vsh" ;;
            *pwsh*|*powershell*) CURRENT_SHELL="powershell" ;;
        esac
    fi

    # Fallback: check $SHELL env var
    if [ "$CURRENT_SHELL" = "unknown" ] && [ -n "${SHELL:-}" ]; then
        case "$SHELL" in
            */bash)  CURRENT_SHELL="bash" ;;
            */zsh)   CURRENT_SHELL="zsh" ;;
            */fish)  CURRENT_SHELL="fish" ;;
            */dash)  CURRENT_SHELL="dash" ;;
            */ksh*)  CURRENT_SHELL="ksh" ;;
            */tcsh)  CURRENT_SHELL="tcsh" ;;
            */csh)   CURRENT_SHELL="csh" ;;
            */vsh)   CURRENT_SHELL="vsh" ;;
        esac
    fi

    printf "%s" "$CURRENT_SHELL"
}

# ── Platform Detection ──
detect_platform() {
    OS="unknown"
    DISTRO="unknown"
    PKG_MGR="unknown"
    ARCH=$(uname -m 2>/dev/null || echo "unknown")

    case "$(uname -s 2>/dev/null)" in
        Linux*)
            OS="linux"
            if [ -f /etc/os-release ]; then
                DISTRO=$(. /etc/os-release && echo "$ID")
            elif [ -f /etc/redhat-release ]; then
                DISTRO="rhel"
            elif [ -f /etc/debian_version ]; then
                DISTRO="debian"
            fi
            # Detect package manager
            if command -v dnf >/dev/null 2>&1; then PKG_MGR="dnf"
            elif command -v apt-get >/dev/null 2>&1; then PKG_MGR="apt"
            elif command -v pacman >/dev/null 2>&1; then PKG_MGR="pacman"
            elif command -v apk >/dev/null 2>&1; then PKG_MGR="apk"
            elif command -v zypper >/dev/null 2>&1; then PKG_MGR="zypper"
            elif command -v rpm-ostree >/dev/null 2>&1; then PKG_MGR="rpm-ostree"
            elif command -v guix >/dev/null 2>&1; then PKG_MGR="guix"
            fi
            ;;
        Darwin*)
            OS="macos"
            DISTRO="macos"
            if command -v brew >/dev/null 2>&1; then PKG_MGR="brew"
            elif command -v port >/dev/null 2>&1; then PKG_MGR="macports"
            fi
            ;;
        CYGWIN*|MINGW*|MSYS*)
            OS="windows"
            DISTRO="msys"
            if command -v winget >/dev/null 2>&1; then PKG_MGR="winget"
            elif command -v scoop >/dev/null 2>&1; then PKG_MGR="scoop"
            elif command -v choco >/dev/null 2>&1; then PKG_MGR="choco"
            fi
            ;;
        FreeBSD*)
            OS="freebsd"
            DISTRO="freebsd"
            PKG_MGR="pkg"
            ;;
    esac
}

# ── Install just ──
# Use versioned upstream binaries with release-published digests. Unsupported
# platforms fail closed rather than executing a mutable installer script.
install_just_verified() {
    _ji_version="1.58.0"

    case "$OS:$ARCH" in
        linux:x86_64|linux:amd64)
            _ji_target="x86_64-unknown-linux-musl"
            _ji_sha256="4a5cc2f53e6f0f8c59092a6cc38291eb729d46a7dd95d3ae582008881b84931d"
            ;;
        linux:aarch64|linux:arm64)
            _ji_target="aarch64-unknown-linux-musl"
            _ji_sha256="748237128c4c40cbdabc65e841d05ceba13cc23a91eaba395495894c1d9764df"
            ;;
        macos:x86_64|macos:amd64)
            _ji_target="x86_64-apple-darwin"
            _ji_sha256="9a09cfef66aaa79da58203970103a0684307716caaabd3e9844cacc4dc0f4023"
            ;;
        macos:aarch64|macos:arm64)
            _ji_target="aarch64-apple-darwin"
            _ji_sha256="50ae3e996c974a0bf32ea7d10f495070df33f1b43e0616b2769e3d4821ed8f48"
            ;;
        *)
            fail "No checksum-verified just artifact for $OS/$ARCH."
            return 1
            ;;
    esac

    _ji_tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/just-install.XXXXXX") || {
        fail "Could not create a private temporary directory."
        return 1
    }
    _ji_archive="$_ji_tmp_dir/just.tar.gz"
    _ji_url="https://github.com/casey/just/releases/download/${_ji_version}/just-${_ji_version}-${_ji_target}.tar.gz"

    if ! curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
        --fail --show-error --location --output "$_ji_archive" "$_ji_url"; then
        fail "Could not download the pinned just ${_ji_version} archive."
        rm -f "$_ji_archive"
        rmdir "$_ji_tmp_dir"
        return 1
    fi

    if command -v sha256sum >/dev/null 2>&1; then
        _ji_actual=$(sha256sum "$_ji_archive" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        _ji_actual=$(shasum -a 256 "$_ji_archive" | awk '{print $1}')
    else
        fail "No SHA-256 verifier is available; refusing to install just."
        rm -f "$_ji_archive"
        rmdir "$_ji_tmp_dir"
        return 1
    fi

    if [ "$_ji_actual" != "$_ji_sha256" ]; then
        fail "just ${_ji_version} archive checksum mismatch."
        rm -f "$_ji_archive"
        rmdir "$_ji_tmp_dir"
        return 1
    fi

    if ! tar -xzf "$_ji_archive" -C "$_ji_tmp_dir" just; then
        fail "Could not extract the verified just archive."
        rm -f "$_ji_archive" "$_ji_tmp_dir/just"
        rmdir "$_ji_tmp_dir"
        return 1
    fi

    if [ -w /usr/local/bin ]; then
        if install -m 0755 "$_ji_tmp_dir/just" /usr/local/bin/just; then
            _ji_rc=0
        else
            _ji_rc=$?
        fi
    elif command -v sudo >/dev/null 2>&1; then
        if sudo install -m 0755 "$_ji_tmp_dir/just" /usr/local/bin/just; then
            _ji_rc=0
        else
            _ji_rc=$?
        fi
    else
        fail "/usr/local/bin is not writable and sudo is unavailable."
        _ji_rc=1
    fi

    rm -f "$_ji_archive" "$_ji_tmp_dir/just"
    rmdir "$_ji_tmp_dir"
    return "$_ji_rc"
}

install_just() {
    if command -v just >/dev/null 2>&1; then
        ok "just already installed: $(just --version 2>/dev/null | head -1)"
        return 0
    fi

    info "Installing just (task runner)..."

    case "$PKG_MGR" in
        dnf)        sudo dnf install -y just ;;
        apt)        sudo apt-get install -y just 2>/dev/null || install_just_verified ;;
        pacman)     sudo pacman -S --noconfirm just ;;
        apk)        sudo apk add just ;;
        brew)       brew install just ;;
        scoop)      scoop install just ;;
        winget)     winget install Casey.Just ;;
        rpm-ostree) sudo rpm-ostree install just ;;
        guix)       guix install just ;;
        *)
            info "Using checksum-verified just release artifact..."
            install_just_verified
            ;;
    esac

    if command -v just >/dev/null 2>&1; then
        ok "just installed: $(just --version 2>/dev/null | head -1)"
    else
        fail "Could not install just. Install manually: https://just.systems/"
        return 1
    fi
}

# ── Main ──
main() {
    printf "%s=== enaction-engine Setup ===%s\n\n" "$BOLD" "$RESET"

    # Detect environment
    SHELL_NAME=$(detect_shell)
    detect_platform

    info "Shell:    $SHELL_NAME"
    info "Platform: $OS ($DISTRO)"
    info "Arch:     $ARCH"
    info "Packages: $PKG_MGR"
    printf "\n"

    # Warn about exotic shells
    case "$SHELL_NAME" in
        vsh)
            info "Valence Shell detected — experimental support"
            info "Falling back to POSIX sh for setup, vsh for post-setup"
            ;;
        nushell|elvish|murex|ion|hilbish|oil|xonsh|oh)
            info "$SHELL_NAME detected — using POSIX sh for setup"
            ;;
    esac

    # Step 1: Install just
    printf "%sStep 1: Install task runner%s\n" "$BOLD" "$RESET"
    install_just || { fail "Cannot proceed without just"; exit 1; }
    printf "\n"

    # Step 2: Check if we're in the repo directory
    if [ ! -f "Justfile" ] && [ ! -f "justfile" ]; then
        warn "Not in a repo directory (no Justfile found)"
        info "Clone first: git clone https://github.com/metadatastician/enaction-engine.git"
        info "Then: cd enaction-engine && ./setup.sh"
        exit 1
    fi

    # Step 3: Run just setup
    printf "%sStep 2: Project setup%s\n" "$BOLD" "$RESET"
    if just --list 2>/dev/null | grep -q "^setup "; then
        just setup
    elif just --list 2>/dev/null | grep -q "^setup-dev "; then
        just setup-dev
    else
        warn "No 'setup' recipe in Justfile — running 'just doctor' instead"
        just doctor 2>/dev/null || true
    fi
    printf "\n"

    # Step 4: Post-install security snapshot
    printf "%sStep 3: Security snapshot%s\n" "$BOLD" "$RESET"
    if command -v firewall-cmd >/dev/null 2>&1; then
        if firewall-cmd --state 2>/dev/null | grep -q running; then
            ok "Firewall: firewalld active"
        else
            warn "Firewall: firewalld installed but not running"
            info "  Enable: sudo systemctl enable --now firewalld"
        fi
    elif command -v ufw >/dev/null 2>&1; then
        if ufw status 2>/dev/null | grep -q "Status: active"; then
            ok "Firewall: ufw active"
        else
            warn "Firewall: ufw installed but not active"
            info "  Enable: sudo ufw enable"
        fi
    else
        warn "Firewall: none detected"
        case "$PKG_MGR" in
            dnf|rpm-ostree) info "  Install: sudo dnf install firewalld && sudo systemctl enable --now firewalld" ;;
            apt) info "  Install: sudo apt install ufw && sudo ufw enable" ;;
            *) info "  Install a firewall for your platform" ;;
        esac
    fi

    if command -v getenforce >/dev/null 2>&1; then
        SE_STATUS=$(getenforce 2>/dev/null || echo "unknown")
        case "$SE_STATUS" in
            Enforcing) ok "SELinux: Enforcing" ;;
            Permissive) warn "SELinux: Permissive (recommend Enforcing: sudo setenforce 1)" ;;
            *) warn "SELinux: $SE_STATUS" ;;
        esac
    fi

    # Write report
    REPORT_FILE="INSTALL-SECURITY-REPORT.adoc"
    {
        printf "// SPDX-License-Identifier: MPL-2.0\n"
        printf "= Install Security Report\n"
        printf ":date: %s\n\n" "$(date -Iseconds 2>/dev/null || date)"
        printf "== Platform\n"
        printf "* OS: %s (%s)\n" "$OS" "$DISTRO"
        printf "* Arch: %s\n" "$ARCH"
        printf "* Package manager: %s\n" "$PKG_MGR"
        printf "* Shell: %s\n\n" "$SHELL_NAME"
        printf "== Security Status\n"
        printf "Run \`just doctor\` for full diagnostic.\n"
    } > "$REPORT_FILE"
    info "Security report: $REPORT_FILE"
    printf "\n"

    # Done
    printf "%s=== Setup Complete ===%s\n\n" "${BOLD}${GREEN}" "$RESET"
    printf "Next steps:\n"
    printf "  just doctor     — verify everything works\n"
    printf "  just tour       — guided tour of the project\n"
    printf "  just build      — build the project\n"
    printf "  just help-me    — get help if stuck\n"
}

main "$@"
