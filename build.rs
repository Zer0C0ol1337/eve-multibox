// Build-Skript: wählt das Plattform-Backend und bettet unter Windows Icon und
// Versionsinfos (inkl. Copyright) in die .exe ein.
//
// Backends (als cfg gesetzt):
//   win_backend – Win32 (Ziel Windows)
//   x11_backend – X11/XWayland (Ziel Linux)
// Mit EVEMB_CHECK_WINDOWS=1 wird unter Linux das Windows-Backend gewählt – nur zum
// Typprüfen per `cargo check` (scripts/dev.sh check), nicht zum Bauen.
//
// Ressourcen gibt es nur beim Bauen für Windows:
//   GNU-Ziel (WSL, scripts/dev.sh) – `windres` aus mingw-w64. Fehlt es, wird ohne Ressourcen
//                                    gebaut und nur gewarnt.
//   MSVC-Ziel (GitHub Actions)     – Ressourcen-Compiler `rc.exe` über das Crate embed-resource.

use std::path::PathBuf;
use std::process::Command;

include!("src/icon.rs");

/// Einstiegspunkt des Build-Skripts.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon.rs");
    select_backend();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR fehlt"));
    let ico = out.join("eve-multibox.ico");
    std::fs::write(&ico, icon_ico(&[16, 20, 24, 32, 40, 48, 64, 256])).expect("Icon schreiben");

    let rc = out.join("resources.rc");
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("gnu") => {
            std::fs::write(&rc, resource_script("eve-multibox.ico")).expect("RC-Datei schreiben");
            embed_with_windres(&rc, &out);
        }
        Ok("msvc") => {
            // Absoluter Pfad, damit rc.exe das Icon unabhängig vom Arbeitsverzeichnis findet.
            let ico_path = ico.display().to_string().replace('\\', "\\\\");
            std::fs::write(&rc, resource_script(&ico_path)).expect("RC-Datei schreiben");
            embed_with_rc(&rc);
        }
        _ => println!("cargo:warning=Unbekanntes Windows-Ziel – .exe wird ohne Icon/Versionsinfo gebaut."),
    }
}

/// GNU-Ziel: Ressourcen mit `windres` übersetzen und dem Linker übergeben.
fn embed_with_windres(rc: &std::path::Path, out: &std::path::Path) {
    let obj = out.join("resources.o");
    let candidates = [
        std::env::var("WINDRES").unwrap_or_default(),
        "x86_64-w64-mingw32-windres".to_string(),
        "windres".to_string(),
    ];
    for windres in candidates.iter().filter(|c| !c.is_empty()) {
        let status = Command::new(windres)
            .arg("--input")
            .arg(rc)
            .arg("--output")
            .arg(&obj)
            .arg("--output-format=coff")
            .arg("--include-dir")
            .arg(out)
            .status();
        if let Ok(s) = status
            && s.success()
        {
            println!("cargo:rustc-link-arg-bins={}", obj.display());
            return;
        }
    }
    println!(
        "cargo:warning=windres nicht gefunden – .exe wird ohne Icon/Versionsinfo gebaut (sudo apt install mingw-w64)."
    );
}

/// MSVC-Ziel (Bau unter Windows): Ressourcen mit `rc.exe` über embed-resource einbetten.
#[cfg(windows)]
fn embed_with_rc(rc: &std::path::Path) {
    if let Err(e) = embed_resource::compile(rc, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=Ressourcen konnten nicht eingebettet werden: {e}");
    }
}

/// MSVC-Ziel von einem Nicht-Windows-Rechner aus: kein rc.exe verfügbar.
#[cfg(not(windows))]
fn embed_with_rc(_rc: &std::path::Path) {
    println!("cargo:warning=MSVC-Ressourcen nur beim Bauen unter Windows – .exe ohne Icon/Versionsinfo.");
}

/// Setzt `win_backend` oder `x11_backend` je nach Zielsystem (siehe Kopfkommentar).
fn select_backend() {
    println!("cargo:rustc-check-cfg=cfg(win_backend)");
    println!("cargo:rustc-check-cfg=cfg(x11_backend)");
    println!("cargo:rerun-if-env-changed=EVEMB_CHECK_WINDOWS");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let force_windows = std::env::var_os("EVEMB_CHECK_WINDOWS").is_some();
    if os == "windows" || force_windows {
        println!("cargo:rustc-cfg=win_backend");
    } else if os == "linux" {
        println!("cargo:rustc-cfg=x11_backend");
    } else {
        panic!("EVE Multibox unterstützt nur Windows und Linux");
    }
}

/// Ressourcen-Skript: Icon (Pfad `icon`) und Versionsinformationen (Datei → Eigenschaften → Details).
fn resource_script(icon: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mut parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    parts.resize(4, 0);
    let numeric = format!("{},{},{},{}", parts[0], parts[1], parts[2], parts[3]);
    format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "LeggaSchnegge"
      VALUE "FileDescription", "EVE Multibox"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "eve-multibox"
      VALUE "LegalCopyright", "Copyright (C) 2026 LeggaSchnegge"
      VALUE "OriginalFilename", "eve-multibox.exe"
      VALUE "ProductName", "EVE Multibox"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}
