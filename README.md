# EVE Multibox

🇬🇧 **English** · 🇩🇪 [Deutsch](README.de.md)

Free multiboxing tool for EVE Online on **Windows 11 and Linux** – written in Rust, without input broadcasting.
One codebase, two programs: `eve-multibox.exe` (Windows) and `eve-multibox` (Linux, X11/XWayland).

**Version 0.4.0** · [Download (Releases)](https://github.com/Zer0C0ol1337/eve-multibox/releases/latest) ·
[Changelog](CHANGELOG.md) · MIT License

© 2026 LeggaSchnegge · Questions or anything odd? → @[3THEL] LeggaSchnegge

![EVE Multibox – dashboard and live previews](docs/screenshot.png)

## Download & start

| System | File | Start |
|---|---|---|
| **Windows 10/11** | `eve-multibox-0.4.0-windows-x64.zip` | unzip, run `eve-multibox.exe` |
| **Linux** | `eve-multibox-0.4.0-linux-x86_64.tar.gz` | unpack, run `./eve-multibox --install`, then start it from your app menu |

EVE has to run in window mode ("Window" or "Fixed window"). Linux details below.

**Antivirus:** A single scanner (SecureAge) reported a false positive on VirusTotal (1/68) – common for new,
unsigned programs. The complete source code is public here; if you like, build the `.exe` yourself.

## Features

| Area | Features |
|---|---|
| **Layouts** | Profiles with a main slot and any number of secondary slots, multiple monitors, monitor presets (main + grid, main + sidebar), "capture current positions", borderless, apply automatically on login, hotkey to swap |
| **Live previews** | Live images of all clients (like EVE-O Preview; Windows: DWM, Linux: XComposite), left click = focus, right click = swap into main window, drag = move (position saved per character), active client highlighted |
| **Hotkeys** | Next/previous client, swap, apply layout, previews on/off, one hotkey per character – by default only active while EVE is in front |
| **CPU** | Priority and core affinity per character, throttle background clients (priority + dedicated cores), minimize inactive clients |
| **Memory** | RAM per client (working set + private), warnings per client and for the system, free memory (manually or automatically for background clients) |
| **Dashboard** | System CPU/RAM, EVE totals, table of all clients with CPU history, status and quick actions |
| **Tray & autostart** | System tray icon with menu, X hides to the tray, start with Windows (straight to the tray), a second start brings the running dashboard to the front |
| **Languages** | Deutsch, English, Français, Русский, Español, 简体中文 – automatic from the system language or fixed (Chinese uses a system font, e.g. Microsoft YaHei or Noto CJK) |
| **Hotkey recording** | "Record" button next to every hotkey field – just press the combination |
| **Design** | Custom title bar, dark EVE look with orange accents, preview titles marking the active client and the main window |

## Linux

Works with EVE via Steam/Proton or Lutris on **X11** and on **Wayland with XWayland** (default on KDE Plasma and
GNOME – EVE itself runs there as an XWayland window). Same feature set as on Windows: layouts, live previews
(XComposite), hotkeys, throttling, memory tools, dashboard, tray, autostart, all languages.

```bash
tar xzf eve-multibox-0.4.0-linux-x86_64.tar.gz && cd eve-multibox-0.4.0-linux-x86_64
./eve-multibox --install                       # menu entry + icon (no sudo), --uninstall removes everything
sudo setcap cap_sys_nice+ep ./eve-multibox     # optional: raise/restore priority + page out memory
```

- **Requirement:** glibc ≥ 2.30 (Ubuntu 20.04+, Debian 11+, Fedora, Arch, Mint 20+, SteamOS in desktop mode).
- **Permissions:** Without `cap_sys_nice`, Linux only allows *lowering* the priority – throttling works, restoring
  doesn't fully. The Performance tab shows the status and the matching `setcap` command (with a copy button).
  Set it again after every update.
- **Free memory** uses `process_madvise(MADV_PAGEOUT)` (kernel ≥ 5.10) and pages memory out to swap/zram.
- **Tray:** via StatusNotifierItem – KDE, Xfce, Cinnamon, MATE, LXQt; GNOME needs the "AppIndicator" extension.
  Without a tray, X really closes the program.
- **Pure Wayland** (without XWayland) doesn't let other programs move windows or capture previews – the settings
  then show a note.
- Configuration: `~/.config/eve-multibox/config.json`, log file next to it. Autostart: `~/.config/autostart/`.
- Commands: `--install`, `--uninstall`, `--minimized`, `--version`, `--help`.

## Rules compliance

CCP forbids input broadcasting and multiplexing: a single input must not affect more than one client.
EVE Multibox **never sends keyboard or mouse input to EVE**. It only uses operating system functions:

- Move/resize, focus and minimize windows (Windows: `SetWindowPos`, `SetForegroundWindow`; Linux: EWMH)
- Preview images like EVE-O Preview (Windows: `DwmRegisterThumbnail`; Linux: XComposite)
- Process priority and core affinity, freeing memory (Windows: `EmptyWorkingSet`; Linux: `nice`,
  `sched_setaffinity`, `process_madvise`)
- Global hotkeys (Windows: `RegisterHotKey`; Linux: `XGrabKey`): one keypress triggers exactly one window action

Every keypress in the game stays with the active client. Responsibility for following the current EULA still lies
with the player – if in doubt, ask CCP.

## Development (WSL + VS Code)

Builds run in WSL and cross-compile straight to a Windows `.exe` (target `x86_64-pc-windows-gnu`).

```bash
# once: Windows target + MinGW linker
scripts/dev.sh setup

# unit tests of the core logic (run natively on Linux)
scripts/dev.sh test        # or: cargo t

# debug build, copy to C:\Users\<you>\EveMultibox and start
scripts/dev.sh run

# release build (small, no console window) → dist/…-windows-x64.zip and C:\Users\<you>\EveMultibox
scripts/dev.sh release

# tests + clippy for both backends (the Windows backend is type-checked on Linux)
scripts/dev.sh check

# Linux: once zig + cargo-zigbuild, then build the package → dist/eve-multibox-<version>-linux-x86_64.tar.gz
scripts/dev.sh setup-linux
scripts/dev.sh linux

# try the Linux version with simulated EVE clients (WSLg on Windows 11 is enough)
scripts/dev.sh demo-linux
```

- `.cargo/config.toml` makes Windows the default target – plain `cargo build` always produces the `.exe`.
- `.vscode/settings.json` points rust-analyzer at the Windows target. Recommended extensions: *rust-analyzer*, *WSL*.
- `cargo ck` quickly type-checks without the Windows linker.
- The debug build additionally opens a console window (handy for panics), the release build doesn't.

On Windows the configuration lives in `%APPDATA%\EveMultibox\config.json` and is saved automatically on every change.
Next to it is `eve-multibox.log` (startup log, crash details).

On Windows builds, `build.rs` embeds the icon and version info (copyright LeggaSchnegge) into the `.exe` via `windres`.

**Adding a language:** add a column in `src/i18n.rs` – `cargo t` reports every missing text.

## Publishing a release

1. Bump the version in `Cargo.toml`, update `CHANGELOG.md`, `README.md` and `README.de.md`, commit and push.
2. On GitHub: **Releases → Draft a new release**, create a new tag `vX.Y.Z` (same as the version in `Cargo.toml`),
   paste the notes, **Publish release**.
3. GitHub Actions (`.github/workflows/release.yml`) builds the Windows ZIP and the Linux package and attaches them to
   the release after a few minutes. If the tag doesn't match the version, the run stops with a clear message.

## Project layout

```
src/
  lib.rs          library "evemb": platform-independent logic (testable on Linux)
  engine_api.rs   shared types between UI and engine (commands, events, snapshot)
  platform.rs     facade: startup, language, single instance, monitors, autostart – per OS
  config.rs       configuration, load/save, core masks
  hotkey.rs       parser "Ctrl+Shift+F1" → modifiers + virtual key
  i18n.rs         texts in 6 languages (tests check completeness and placeholders)
  icon.rs         app icon drawn in code (window, tray, .exe)
  layout.rs       character from title, ordering, slot assignment, presets, snapping
  main.rs         startup: DPI, language, single instance, dashboard
  diag.rs         startup log and crash report
  engine.rs       engine thread: clients, hotkeys, throttling, layouts, snapshot
  tray.rs         system tray icon with menu
  preview.rs      preview windows with DWM thumbnails, drag, resize, snapping
  procstat.rs     CPU time, memory, priority, affinity, freeing memory
  win.rs          find/focus/move windows, monitors
  ui/             egui interface (dashboard, layouts, previews, performance, hotkeys, settings, theme)
  linux/          Linux backend: engine, x11 (EWMH), preview (XComposite/XRender), label (title rendering),
                  keys (XGrabKey), procstat (/proc, nice, sched_setaffinity, process_madvise),
                  tray (StatusNotifierItem), signal (tray/signals → engine), platform (XDG, autostart, --install)
examples/
  fake_eve.rs     simulated EVE clients for testing on Linux/WSLg
```

`build.rs` selects the backend: Windows → `win_backend` (engine.rs, preview.rs, tray.rs, …), Linux → `x11_backend`
(`src/linux/`). With `EVEMB_CHECK_WINDOWS=1` the Windows backend is type-checked on Linux as well.

The engine runs in its own thread (Windows: its own Win32 message loop, Linux: poll on the X11 connection and a
wake-up eventfd). The UI sends it commands over a channel and reads the state from a shared snapshot. On exit the
engine restores the priority and affinity of all clients.

## Good to know

- **Start EVE in window mode** ("Window" or "Fixed window"). "Fixed window" is already borderless.
- **Admin rights:** If EVE runs as administrator, EVE Multibox may not change priority/affinity – the dashboard then
  shows "No rights". Fix: start both the same way.
- **Minimized clients** deliver no preview image (black) – a Windows limitation.
- **Freeing memory** moves pages to the standby list. If the client needs them again it reloads them (short stutter).
  Good for background clients, not for the active one in combat.
- **More than 64 logical cores** (processor groups) are not supported.
- **Hotkeys** are captured by the system and don't reach EVE – pick combinations that are free in the game.
