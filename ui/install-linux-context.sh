#!/bin/bash
set -euo pipefail

echo "Installing Cortex Archiver Linux Desktop & KDE Integration..."

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
APP_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
NAUTILUS_DIR="$HOME/.local/share/nautilus/scripts"
MIME_DIR="$HOME/.local/share/mime/packages"
KDE_SERVICES_DIR="$HOME/.local/share/kio/servicemenus"

mkdir -p "$APP_DIR" "$DESKTOP_DIR" "$NAUTILUS_DIR" "$MIME_DIR" "$KDE_SERVICES_DIR"

# ---------------------------------------------------------------------------
# 1. GUI binary. Tauri's default release output is "app"; other toolchains
#    may name it cortex-ui or cortex-archiver. `cortex-ui` in PATH is the GUI;
#    the CLI keeps its own `cortex` name and is never shadowed by this script.
# ---------------------------------------------------------------------------
BINARY_SRC=""
for src in "src-tauri/target/release/app" "src-tauri/target/release/cortex-ui" "src-tauri/target/release/cortex-archiver" "src-tauri/target/release/cortex_archiver"; do
    if [ -f "$src" ]; then
        BINARY_SRC="$src"
        break
    fi
done

if [ -n "$BINARY_SRC" ]; then
    echo "Copying cortex-ui binary ($BINARY_SRC) to $APP_DIR..."
    cp -f "$BINARY_SRC" "$APP_DIR/cortex-ui"
else
    echo "Warning: cortex-ui binary not found. Please run 'npm run tauri build' first."
fi

# ---------------------------------------------------------------------------
# 2. MIME type registration (application/x-cortex).
#    Content sniffing via magic bytes: CTX6 (current), plus CTX5/CTX4 which
#    the decompressor still accepts. CTX3 is deliberately absent — the reader
#    rejects it, so we must not advertise it. Split volumes (*.crx.NNN) are
#    matched with a numeric glob instead of a hard-coded .001.
# ---------------------------------------------------------------------------
echo "Installing Cortex MIME type..."
cat << EOF > "$MIME_DIR/cortex.xml"
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-cortex">
    <comment>Cortex Compressed Archive</comment>
    <glob pattern="*.crx"/>
    <glob pattern="*.crx.[0-9][0-9][0-9]"/>
    <magic priority="50">
      <match type="string" value="CTX6" offset="0"/>
      <match type="string" value="CTX5" offset="0"/>
      <match type="string" value="CTX4" offset="0"/>
    </magic>
  </mime-type>
</mime-info>
EOF

command -v update-mime-database &>/dev/null && update-mime-database "$HOME/.local/share/mime" || true

# ---------------------------------------------------------------------------
# 3. Main application .desktop file.
#    Scoped to application/x-cortex only. The previous application/octet-stream
#    claim made Cortex offer itself for every unknown binary in "Open with…",
#    which together with the ServiceMenu action produced duplicate
#    "Open with Cortex" entries. Contextual Compress/Extract live in the
#    ServiceMenus below, so this file has no [Desktop Action] sections.
# ---------------------------------------------------------------------------
echo "Installing Main .desktop file..."
cat << 'EOF' > "$DESKTOP_DIR/cortex-ui.desktop"
[Desktop Entry]
Name=Cortex
Comment=Experimental extremely fast lossless data compressor
Exec=cortex-ui %F
Icon=archive
Terminal=false
Type=Application
MimeType=application/x-cortex;
Categories=Utility;Archiving;Compression;
EOF

command -v update-desktop-database &>/dev/null && update-desktop-database "$DESKTOP_DIR" || true

# ---------------------------------------------------------------------------
# 4. KDE ServiceMenus — one file per action, each with a distinct label so the
#    context menu never shows two identical "Open with Cortex" lines.
# ---------------------------------------------------------------------------
echo "Installing KDE ServiceMenus..."

rm -f "$KDE_SERVICES_DIR/cortex-compress.desktop"
rm -f "$KDE_SERVICES_DIR/cortex-extract.desktop"

# Compress: any file or folder that is not already a Cortex archive.
cat << EOF > "$KDE_SERVICES_DIR/cortex-compress.desktop"
[Desktop Entry]
Type=Service
Name=Cortex Compress
X-KDE-ServiceTypes=KonqPopupMenu/Plugin
MimeType=all/allfiles;inode/directory;
ExcludeMimeType=application/x-cortex;
Actions=compress;

[Desktop Action compress]
Name=Compress with Cortex
Icon=archive
Exec=$APP_DIR/cortex-ui compress %F
EOF

# Extract: only Cortex archives (base file and split volumes).
cat << EOF > "$KDE_SERVICES_DIR/cortex-extract.desktop"
[Desktop Entry]
Type=Service
Name=Cortex Extract
X-KDE-ServiceTypes=KonqPopupMenu/Plugin
MimeType=application/x-cortex;
Actions=extract;

[Desktop Action extract]
Name=Extract with Cortex
Icon=archive
Exec=$APP_DIR/cortex-ui extract %F
EOF

chmod +x "$KDE_SERVICES_DIR/cortex-compress.desktop"
chmod +x "$KDE_SERVICES_DIR/cortex-extract.desktop"

# ---------------------------------------------------------------------------
# 5. Nautilus (GNOME) integration.
#    NAUTILUS_SCRIPT_SELECTED_FILE_PATHS is a NEWLINE-separated list; splitting
#    it on newlines into an argv array passes each file as its own argument.
#    The old script passed the whole list as one quoted string, which reached
#    the app as a single bogus path.
# ---------------------------------------------------------------------------
echo "Installing Nautilus scripts..."

rm -f "$NAUTILUS_DIR/Cortex Compress"
rm -f "$NAUTILUS_DIR/Cortex Extract"

cat << EOF > "$NAUTILUS_DIR/Open with Cortex"
#!/bin/bash
files=()
while IFS= read -r f; do
    [ -n "\$f" ] && files+=("\$f")
done <<< "\$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS"
exec $APP_DIR/cortex-ui open "\${files[@]}"
EOF
chmod +x "$NAUTILUS_DIR/Open with Cortex"

# ---------------------------------------------------------------------------
# 6. Refresh caches so Dolphin/Konqueror pick up the new menus immediately.
# ---------------------------------------------------------------------------
if command -v kbuildsycoca6 &>/dev/null; then
    kbuildsycoca6 >/dev/null 2>&1 || true
fi

echo "Installation complete!"
echo "NOTE: You may need to restart your file manager (e.g. 'killall dolphin' or 'nautilus -q') for changes to take effect."