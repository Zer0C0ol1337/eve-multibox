# Changelog

All notable changes to EVE Multibox. Version numbers follow [SemVer](https://semver.org/);
the version is kept identical in `Cargo.toml`, the Git tag (`vX.Y.Z`), the GitHub release and the in-app display.

## [0.4.1] – 2026-10-13

### Changed
- The Windows release is now built on Windows with the Microsoft compiler (MSVC) instead of MinGW – such builds tend to be
  flagged less often by antivirus heuristics. Icon and version info are embedded as before.
- Release workflow prepared for free code signing via the SignPath Foundation; it signs the `.exe` automatically as
  soon as signing is configured (see `docs/SIGNING.md`).

### Added
- README: uninstall instructions, code signing policy and privacy statement (English and German).

### Internal
- `scripts/dev.sh`: new command `package-windows`; Windows target and Python interpreter are configurable.
- `build.rs`: resources for the MSVC target via `embed-resource`.

## [0.4.0] – 2026-10-13

### Added
- **Linux version** built from the same source code: X11 and Wayland with XWayland (KDE Plasma, GNOME, Xfce,
  Cinnamon, Mint, Steam Deck in desktop mode), EVE via Steam/Proton or Lutris.
  - Live previews via XComposite/XRender, hotkeys via XGrabKey, layouts via EWMH.
  - CPU tools via `nice` and `sched_setaffinity`, freeing memory via `process_madvise`.
  - Tray icon via StatusNotifierItem, autostart via `~/.config/autostart`.
  - `--install` / `--uninstall` (menu entry and icon without sudo), `--minimized`, `--version`, `--help`.
  - Runs on glibc 2.30 and newer (Ubuntu 20.04+, Debian 11+, Mint 20+, Fedora, Arch, SteamOS).
- **New design:** custom title bar instead of the system frame, dark EVE look with orange accents,
  stat cards, underlined tabs, colored status badges.
- **New preview title bars:** smooth fonts, accent dot and frame for the active client,
  diamond marker for the main window.
- Clear message with the install command when system libraries are missing on Linux.

### Fixed
- "Start in tray" still opened the dashboard.
- A second program start cleared the log of the running instance.
- Login previews jumped back to their old position after being moved.
- Some arrow symbols were shown as boxes.
- No clients were found under WSLg/Weston (window manager without a window list).

### Internal
- Platform facade (`platform.rs`) and shared engine types (`engine_api.rs`); backend selection in `build.rs`.
- `scripts/dev.sh`: new commands `check`, `setup-linux`, `linux`, `windows`, `demo-linux`.
- Test tool `examples/fake_eve.rs` (simulated EVE clients).
- GitHub Actions builds the Windows ZIP and the Linux package when a release is published and attaches them;
  beforehand it checks that the tag matches the version in `Cargo.toml`.
- README in English (`README.md`) and German (`README.de.md`).

## Earlier versions (0.1–0.3, Windows only)

- Window layouts per profile with main and secondary slots, monitor presets and a "stacked" fullscreen layout.
- Live previews with DWM thumbnails: freely movable and resizable, snapping to other previews.
- Hotkeys (cycle, swap, layout, previews, per character) with recording by keypress.
- CPU tools (priority, core affinity, throttling), memory tools (monitoring, warnings, freeing).
- Dashboard, tray icon, autostart, six languages (DE, EN, FR, RU, ES, ZH), copyright watermark.
