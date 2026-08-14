#!/usr/bin/env bash
#
# CORTEX Archiver — tek komutla kurulum
#
#   curl -fsSL https://raw.githubusercontent.com/atalhatulu/cortex/main/install.sh | sh
#
# Veya:
#   wget -qO- https://raw.githubusercontent.com/atalhatulu/cortex/main/install.sh | sh
#
# Ne yapar:
#   1. Kaynak koddan `cortex` CLI'ını derler (cargo gerekir) ve ~/.local/bin'e koyar.
#   2. Tauri masaüstü GUI'sine masaüstü kısayolu (.desktop) oluşturur.
#   3. PATH'e ~/.local/bin ekler (bashrc/zshrc).
#
# İsteğe bağlı: Sürüm etiketli release binary indirmek için (GitHub Actions üretiyorsa):
#   CORTEX_VERSION=v0.1.0-beta.1 curl -fsSL ... | sh

set -euo pipefail

REPO="atalhatulu/cortex"
BRANCH="${CORTEX_BRANCH:-main}"
VERSION="${CORTEX_VERSION:-source}"   # "source" => kaynaktan derle; aksi halde binary etiketi

PREFIX="${CORTEX_PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
APPDIR="$PREFIX/share/applications"

info()  { printf '\033[1;36m[cortex]\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m[cortex!]\033[0m %s\n' "$*"; }
die()   { printf '\033[1;31m[cortex!]\033[0m %s\n' "$*" >&2; exit 1; }

has()   { command -v "$1" >/dev/null 2>&1; }

# ---------------------------------------------------------------------------
say_headers() {
  cat <<'EOF'
   ____ ___  ____ _________  ____
  / ___/ _ \|  _ \___ \__  |/ ___|  CORTEX Archiver
 | |  | | | | |_) |  \ /  |/ __|   (BWT) lossless BWT archiver
 | |__| |_| |  _ <  / __/  |\__ \   Balanced · Max ratio · Max speed
  \____\___/|_| \_\____/  |____/  __/
EOF
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "eksik bagimlilik: $1 (once \`sudo pacman -S $1\` kur)"
}

# ---------------------------------------------------------------------------
build_cortex() {
  local src
  src="$(mktemp -d)"
  info "kaynak koddan derleniyor (branch: $BRANCH)..."
  git clone --depth 1 --branch "$BRANCH" "https://github.com/${REPO}.git" "$src"
  (cd "$src/core" && cargo build --release)
  cp "$src/core/target/release/cortex" "$BINDIR/cortex"
  rm -rf "$src"
}

fetch_release_binary() {
  need curl
  local url="https://github.com/${REPO}/releases/download/${VERSION}/cortex-x86_64-unknown-linux-gnu"
  info "release binary indiriliyor: $VERSION ..."
  curl -fsSL "$url" -o "$BINDIR/cortex"
}

# ---------------------------------------------------------------------------
mkdir -p "$BINDIR" "$APPDIR"

say_headers

if has cargo; then
  build_cortex
else
  if [ "$VERSION" = "source" ]; then
    warn "cargo bulunamadi; release binary'si deneniyor (Girdigi deger: $VERSION)."
    VERSION="latest"
  fi
  fetch_release_binary
fi

chmod +x "$BINDIR/cortex"
info "CLI kuruldu: $BINDIR/cortex"
"$BINDIR/cortex" --version 2>/dev/null || true

# ---------------------------------------------------------------------------
# PATH desteği
rc_updated=0
case ":$PATH:" in
  *":$BINDIR:"*) ;;
  *)
    for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
      if [ -f "$rc" ] && ! grep -qs "$BINDIR" "$rc"; then
        printf '\nexport PATH="$PATH:%s"\n' "$BINDIR" >> "$rc"
        info "PATH eklendi ($rc)"
        rc_updated=1
      fi
    done
    if [ "$rc_updated" -eq 1 ]; then
      info "Yeni shell acinda PATH aktif olacak: $BINDIR"
    else
      warn "PATH'e $BINDIR eklenemedi; manuel: export PATH=\"\$PATH:$BINDIR\""
    fi
    ;;
esac

# ---------------------------------------------------------------------------
# Masaüstü kısayolu (GUI, varsa)
GUI_BIN=""
for c in "$BINDIR/cortex-archiver" "$BINDIR/cortex-gui"; do
  [ -x "$c" ] && GUI_BIN="$c" && break
done

if [ -n "$GUI_BIN" ]; then
  cat > "$APPDIR/cortex-archiver.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Cortex Archiver
Comment=Lossless BWT archiver (Balanced / Max ratio / Max speed)
Exec=$GUI_BIN
Terminal=false
Categories=Utility;Archiving;Compression;
EOF
  chmod +x "$APPDIR/cortex-archiver.desktop"
  info "GUI kisa yolu: $APPDIR/cortex-archiver.desktop"
  if has update-desktop-database; then
    update-desktop-database "$APPDIR" || true
  fi
else
  info "GUI binary'si bulunamadi; CLI kurulum tamamlandi."
  info "GUI icin ayrica: cd ui && npm install && npm run tauri build"
fi

info "Tamam. Kilavuz: https://github.com/${REPO}#readme"
