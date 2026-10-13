#!/usr/bin/env bash
# Hilfsskript für die Entwicklung unter WSL (Windows 11) bzw. Linux.
#
#   scripts/dev.sh setup        – einmalig: Windows-Target und MinGW-Linker installieren
#   scripts/dev.sh setup-linux  – einmalig: Fenster-Bibliotheken, zig + cargo-zigbuild (Linux-Build)
#   scripts/dev.sh test         – Unit-Tests (Bibliothek + Linux-Backend)
#   scripts/dev.sh check        – Tests + Prüfung beider Backends (Windows und Linux) inkl. Clippy
#   scripts/dev.sh run          – Windows-Debug-Build, nach C:\Users\<du>\EveMultibox kopieren und starten
#   scripts/dev.sh release      – Windows-Release: dist/eve-multibox-<version>-windows-x64.zip + C:\Users\<du>\EveMultibox
#   scripts/dev.sh windows      – nur Windows-ZIP nach dist/ (ohne Kopie nach C:, z. B. für GitHub Actions)
#   scripts/dev.sh package-windows – ZIP aus vorhandener dist/eve-multibox.exe (nach dem Signieren)
#   scripts/dev.sh linux        – Linux-Release-Paket nach dist/eve-multibox-<version>-linux-x86_64.tar.gz
#   scripts/dev.sh demo-linux   – Linux-Version mit simulierten EVE-Clients starten (WSLg oder Linux-Desktop)
set -euo pipefail
cd "$(dirname "$0")/.."

# Windows-Ziel: GNU (MinGW, WSL) – in GitHub Actions per WIN_TARGET=x86_64-pc-windows-msvc überschreibbar.
WIN_TARGET=${WIN_TARGET:-x86_64-pc-windows-gnu}
LINUX_TARGET=x86_64-unknown-linux-gnu
EXE=eve-multibox.exe
# Python heißt unter Windows (Git Bash) oft nur „python“.
PYTHON=$(command -v python3 || command -v python)
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

# Windows-Benutzername ermitteln, um auf das Windows-Laufwerk zu kopieren.
win_dest() {
    local user
    user=$(cmd.exe /c "echo %USERNAME%" 2>/dev/null | tr -d '\r')
    echo "/mnt/c/Users/${user}/EveMultibox"
}

# Baut die Windows-.exe im angegebenen Profil und kopiert sie auf das Windows-Laufwerk.
build_and_copy() {
    local profile=$1 dir=debug
    if [[ $profile == release ]]; then
        cargo build --release
        dir=release
    else
        cargo build
    fi
    local dest
    dest=$(win_dest)
    mkdir -p "$dest"
    # Läuft das Programm noch, ist die .exe gesperrt – dann vorher beenden.
    cp "target/$WIN_TARGET/$dir/$EXE" "$dest/$EXE"
    echo "→ $dest/$EXE"
}

# Baut die Linux-Version (Release). Mit cargo-zigbuild gegen glibc 2.31, damit sie auch auf
# älteren Distributionen (Ubuntu 20.04+, Debian 11+, Mint 20+, SteamOS) läuft.
build_linux_release() {
    if command -v cargo-zigbuild >/dev/null 2>&1; then
        cargo zigbuild --release --target "$LINUX_TARGET.2.31"
    else
        echo "Hinweis: cargo-zigbuild fehlt (scripts/dev.sh setup-linux) – Build läuft nur auf gleich neuen oder neueren Distributionen."
        cargo build --release --target "$LINUX_TARGET"
    fi
}

# Schreibt die Kurzanleitung für das Windows-Paket.
windows_readme() {
    cat <<EOF
EVE Multibox $VERSION für Windows / for Windows · © 2026 LeggaSchnegge
======================================================================

DEUTSCH
-------
1. ZIP entpacken (z. B. nach C:\\Spiele\\EveMultibox) und eve-multibox.exe starten.
   Windows SmartScreen: „Weitere Informationen“ → „Trotzdem ausführen“ (Programm ist nicht signiert).
2. EVE im Fenstermodus starten („Fenster“ oder „Fenster (fest)“).
3. Autostart, Infobereich und Sprache findest du im Tab „Einstellungen“.
   Konfiguration: %APPDATA%\\EveMultibox\\config.json

ENGLISH
-------
1. Unzip (e.g. to C:\\Games\\EveMultibox) and run eve-multibox.exe.
   Windows SmartScreen: "More info" → "Run anyway" (the program is not code-signed).
2. Run EVE in window mode ("Window" or "Fixed window").
3. Autostart, tray and language are in the "Settings" tab.
   Configuration: %APPDATA%\\EveMultibox\\config.json

No input broadcasting – one keypress = one action, nothing is ever sent to your clients.
Source code: https://github.com/Zer0C0ol1337/eve-multibox
Questions or anything odd → just contact @[3THEL] LeggaSchnegge
EOF
}

# Schreibt die Kurzanleitung für das Linux-Paket.
linux_readme() {
    cat <<EOF
EVE Multibox $VERSION für Linux / for Linux · © 2026 LeggaSchnegge
====================================================================

DEUTSCH
-------
1. Entpacken und starten:
     chmod +x eve-multibox
     ./eve-multibox --install      (Eintrag im Startmenü + Icon, ohne sudo)
   Danach „EVE Multibox“ über das Startmenü starten.
2. EVE (Steam/Proton oder Lutris) im Fenstermodus starten („Fenster“ oder „Fenster (fest)“).
3. Voraussetzung: X11 oder Wayland mit XWayland (Standard bei KDE Plasma und GNOME).
   Steam Deck: im Desktop-Modus.
4. Tray-Symbol: KDE, Xfce, Cinnamon, MATE, LXQt. GNOME braucht die Erweiterung „AppIndicator“.
5. Optional für Priorität und Speicher-Freigabe (einmalig, nach jedem Update erneut):
     sudo setcap cap_sys_nice+ep ./eve-multibox
6. Entfernen: ./eve-multibox --uninstall

ENGLISH
-------
1. Unpack and run:
     chmod +x eve-multibox
     ./eve-multibox --install      (menu entry + icon, no sudo)
   Then start "EVE Multibox" from your application menu.
2. Run EVE (Steam/Proton or Lutris) in window mode ("Window" or "Fixed window").
3. Requires X11 or Wayland with XWayland (default on KDE Plasma and GNOME).
   Steam Deck: desktop mode.
4. Tray icon: KDE, Xfce, Cinnamon, MATE, LXQt. GNOME needs the "AppIndicator" extension.
5. Optional, for priority and memory freeing (once, again after every update):
     sudo setcap cap_sys_nice+ep ./eve-multibox
6. Remove: ./eve-multibox --uninstall

No input broadcasting – one keypress = one action, nothing is ever sent to your clients.
Source code: https://github.com/Zer0C0ol1337/eve-multibox
Questions or anything odd → just contact @[3THEL] LeggaSchnegge
EOF
}

case "${1:-}" in
    setup)
        rustup target add "$WIN_TARGET"
        sudo apt-get update && sudo apt-get install -y mingw-w64
        ;;
    setup-linux)
        # Laufzeit-Bibliotheken für das Fenster (fehlen in WSL/Minimal-Installationen) + Build-Werkzeuge.
        if command -v apt-get >/dev/null 2>&1; then
            sudo apt-get update
            sudo apt-get install -y libx11-6 libx11-xcb1 libxcursor1 libxi6 libxkbcommon0 \
                libxkbcommon-x11-0 libegl1 libgl1-mesa-dri python3-pip
        fi
        python3 -m pip install --user --break-system-packages ziglang
        cargo install cargo-zigbuild
        ;;
    test)
        cargo test --lib --target "$LINUX_TARGET"
        cargo test --bin eve-multibox --target "$LINUX_TARGET"
        ;;
    check)
        cargo test --lib --target "$LINUX_TARGET"
        cargo test --bin eve-multibox --target "$LINUX_TARGET"
        cargo clippy --all-targets --target "$LINUX_TARGET"
        # Windows-Backend unter Linux typprüfen (eigener Zielordner, damit nichts doppelt baut).
        EVEMB_CHECK_WINDOWS=1 CARGO_TARGET_DIR=target/check-win cargo clippy --target "$LINUX_TARGET"
        echo "✔ Tests und beide Backends geprüft"
        ;;
    run)
        build_and_copy debug
        dest=$(win_dest)
        # Von Windows aus starten (Arbeitsverzeichnis auf C:, nicht \\wsl$).
        (cd "$dest" && cmd.exe /c start "" "$EXE")
        ;;
    release)
        build_and_copy release
        bash scripts/dev.sh windows
        ;;
    windows)
        cargo build --release --target "$WIN_TARGET"
        mkdir -p dist
        cp "target/$WIN_TARGET/release/$EXE" dist/
        bash scripts/dev.sh package-windows
        ;;
    package-windows)
        # ZIP aus dist/eve-multibox.exe + Kurzanleitung (Windows-Zeilenenden) – getrennt vom Bauen,
        # damit GitHub Actions die .exe vorher signieren lassen kann. python statt „zip“ (fehlt in WSL oft).
        windows_readme | sed 's/$/\r/' > dist/LIESMICH-README.txt
        zipname="dist/eve-multibox-$VERSION-windows-x64.zip"
        rm -f "$zipname"
        "$PYTHON" - "$zipname" "dist/$EXE" dist/LIESMICH-README.txt <<'PY'
import os, sys, zipfile
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    for f in sys.argv[2:]:
        z.write(f, os.path.basename(f))
PY
        rm dist/LIESMICH-README.txt
        echo "→ $zipname"
        ;;
    linux)
        build_linux_release
        pkg="eve-multibox-$VERSION-linux-x86_64"
        rm -rf "dist/$pkg"
        mkdir -p "dist/$pkg"
        cp "target/$LINUX_TARGET/release/eve-multibox" "dist/$pkg/"
        linux_readme > "dist/$pkg/LIESMICH-README.txt"
        tar -C dist -czf "dist/$pkg.tar.gz" "$pkg"
        echo "→ dist/$pkg.tar.gz"
        ;;
    demo-linux)
        # Unter WSLg (Windows 11) oder einem Linux-Desktop: drei simulierte Clients + EVE Multibox.
        cargo build --target "$LINUX_TARGET" --example fake_eve
        cargo build --target "$LINUX_TARGET"
        "target/$LINUX_TARGET/debug/examples/fake_eve" "Pilot One" "Pilot Two" LOGIN &
        fake=$!
        trap 'kill $fake 2>/dev/null' EXIT
        "target/$LINUX_TARGET/debug/eve-multibox"
        ;;
    *)
        sed -n '2,14p' "$0"
        exit 1
        ;;
esac
