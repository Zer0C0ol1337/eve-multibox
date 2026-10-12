# Changelog

Alle nennenswerten Änderungen an EVE Multibox. Versionsnummern nach [SemVer](https://semver.org/lang/de/);
die Version steht einheitlich in `Cargo.toml`, im Git-Tag (`vX.Y.Z`), im Release und in der Programmanzeige.

## [0.4.0] – 2026-10-13

### Neu
- **Linux-Version** aus demselben Quellcode: X11 und Wayland mit XWayland (KDE Plasma, GNOME, Xfce, Cinnamon,
  Mint, Steam Deck im Desktop-Modus), EVE über Steam/Proton oder Lutris.
  - Live-Vorschau per XComposite/XRender, Hotkeys per XGrabKey, Layouts über EWMH.
  - CPU-Werkzeuge über `nice` und `sched_setaffinity`, Speicher freigeben über `process_madvise`.
  - Tray-Symbol über StatusNotifierItem, Autostart über `~/.config/autostart`.
  - `--install` / `--uninstall` (Startmenü-Eintrag und Icon ohne sudo), `--minimized`, `--version`, `--help`.
  - Läuft ab glibc 2.30 (Ubuntu 20.04+, Debian 11+, Mint 20+, Fedora, Arch, SteamOS).
- **Neues Design:** eigene Titelleiste statt Systemrahmen, dunkles EVE-Design mit Orange als Akzent,
  Kennzahlen-Karten, Tabs mit Unterstrich, farbige Status-Plaketten.
- **Neue Vorschau-Titelzeilen:** geglättete Schrift, Akzentpunkt und -rahmen für den aktiven Client,
  Raute für das Hauptfenster.
- Verständliche Meldung mit Installationsbefehl, wenn unter Linux System-Bibliotheken fehlen.

### Behoben
- „Im Infobereich starten“ öffnete das Dashboard trotzdem.
- Ein zweiter Programmstart leerte das Protokoll der laufenden Instanz.
- Login-Vorschauen sprangen nach dem Verschieben an ihre alte Stelle zurück.
- Einige Pfeil-Symbole wurden als Kästchen angezeigt.
- Unter WSLg/Weston wurden keine Clients gefunden (Fenstermanager ohne Fensterliste).

### Intern
- Plattform-Fassade (`platform.rs`) und gemeinsame Engine-Typen (`engine_api.rs`); Backend-Auswahl in `build.rs`.
- `scripts/dev.sh`: neue Befehle `check`, `setup-linux`, `linux`, `demo-linux`.
- Testwerkzeug `examples/fake_eve.rs` (simulierte EVE-Clients).
- GitHub Actions baut beim Veröffentlichen eines Releases die Windows-ZIP und das Linux-Paket und hängt sie an;
  vorher wird geprüft, dass der Tag zur Version in `Cargo.toml` passt.

## Frühere Versionen (0.1–0.3, nur Windows)

- Fenster-Layouts pro Profil mit Haupt- und Nebenfenstern, Monitor-Vorlagen und „gestapeltem“ Vollbild-Layout.
- Live-Vorschau mit DWM-Thumbnails: frei verschieb- und skalierbar, Einrasten an anderen Vorschauen.
- Hotkeys (Zyklus, Tauschen, Layout, Vorschau, je Charakter) mit Aufnahme per Tastendruck.
- CPU-Werkzeuge (Priorität, Kern-Affinität, Drosseln), Speicher-Werkzeuge (Überwachung, Warnungen, Freigeben).
- Dashboard, Tray-Symbol, Autostart, sechs Sprachen (DE, EN, FR, RU, ES, ZH), Copyright-Wasserzeichen.
