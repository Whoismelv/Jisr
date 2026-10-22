//! Bottle templates — the 13 Windows personalities a bottle can wear.
//!
//! A bottle is not just a Wine prefix: it is a prefix plus a declared
//! personality (`win98` … `win11_64`) that pins the registry version, the
//! architecture, and the Direct3D component set the prefix starts with.
//! All 13 templates come from one generator with three variables
//! (`template`, `windows_version`, `winearch`). Creating a bottle means:
//!
//! 1. write `jisr.conf` (bottle id, timestamp, encoding, template, arch),
//! 2. create the prefix skeleton (`system.reg`, `user.reg`, `dosdevices`, …),
//! 3. run the INF installer sections through `rundll32 InstallHinfSection`
//!    — **twice** when the bottle is 64-bit, so the Wow64 side gets them too,
//! 4. restore `dosdevices/y:` (the INF pass uses `y:` as a temp drive),
//! 5. `wineboot` to finish, then the bottle hooks.
//!
//! The INF below is ours, generated here, and carries nothing we do not ship.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Windows version a bottle pretends to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsVersion {
    Win98,
    Win2000,
    WinXp,
    WinVista,
    Win7,
    Win8,
    Win10,
    Win11,
}

impl WindowsVersion {
    /// Registry value for `Software\Wine\Windows`.
    pub fn registry_value(self) -> &'static str {
        match self {
            WindowsVersion::Win98 => "win98",
            WindowsVersion::Win2000 => "win2000",
            WindowsVersion::WinXp => "winxp",
            WindowsVersion::WinVista => "winvista",
            WindowsVersion::Win7 => "win7",
            WindowsVersion::Win8 => "win8",
            WindowsVersion::Win10 => "win10",
            WindowsVersion::Win11 => "win11",
        }
    }

    /// Minimum Windows build number Wine expects for this version.
    pub fn build(self) -> u32 {
        match self {
            WindowsVersion::Win98 => 520,
            WindowsVersion::Win2000 => 2195,
            WindowsVersion::WinXp => 2600,
            WindowsVersion::WinVista => 6000,
            WindowsVersion::Win7 => 7601,
            WindowsVersion::Win8 => 7601,
            WindowsVersion::Win10 => 10240,
            WindowsVersion::Win11 => 22000,
        }
    }
}

/// Wine architecture of the bottle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WineArch {
    Win32,
    Win64,
}

impl WineArch {
    pub fn as_str(self) -> &'static str {
        match self {
            WineArch::Win32 => "win32",
            WineArch::Win64 => "win64",
        }
    }
}

/// One bottle template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BottleTemplate {
    /// Stable key used in configuration and on disk.
    pub key: &'static str,
    /// Arabic label for the UI.
    pub label_ar: &'static str,
    pub windows: WindowsVersion,
    pub arch: WineArch,
    /// Does the template need the 32-bit side as well (Wow64 bottle)?
    pub wow64: bool,
    /// Direct3D versions the template's INF configures.
    pub d3d_levels: &'static [&'static str],
}

impl BottleTemplate {
    /// Key of the 32-bit sibling, when there is one.
    pub fn sibling32(self) -> Option<&'static str> {
        match self.key {
            "winxp_64" => Some("winxp"),
            "winvista_64" => Some("winvista"),
            "win7_64" => Some("win7"),
            "win8_64" => Some("win8"),
            "win10_64" => Some("win10"),
            "win11_64" => Some("win10"),
            _ => None,
        }
    }
}

/// Every template we ship, in menu order.
pub const TEMPLATES: &[BottleTemplate] = &[
    BottleTemplate {
        key: "win98",
        label_ar: "ويندوز 98",
        windows: WindowsVersion::Win98,
        arch: WineArch::Win32,
        wow64: false,
        d3d_levels: &["9"],
    },
    BottleTemplate {
        key: "win2000",
        label_ar: "ويندوز 2000",
        windows: WindowsVersion::Win2000,
        arch: WineArch::Win32,
        wow64: false,
        d3d_levels: &["9"],
    },
    BottleTemplate {
        key: "winxp",
        label_ar: "ويندوز XP",
        windows: WindowsVersion::WinXp,
        arch: WineArch::Win32,
        wow64: false,
        d3d_levels: &["9"],
    },
    BottleTemplate {
        key: "winvista",
        label_ar: "ويندوز فيستا",
        windows: WindowsVersion::WinVista,
        arch: WineArch::Win32,
        wow64: false,
        d3d_levels: &["9", "10"],
    },
    BottleTemplate {
        key: "win7",
        label_ar: "ويندوز 7",
        windows: WindowsVersion::Win7,
        arch: WineArch::Win32,
        wow64: false,
        d3d_levels: &["9", "10", "11"],
    },
    BottleTemplate {
        key: "win8",
        label_ar: "ويندوز 8",
        windows: WindowsVersion::Win8,
        arch: WineArch::Win32,
        wow64: false,
        d3d_levels: &["9", "10", "11"],
    },
    BottleTemplate {
        key: "win10",
        label_ar: "ويندوز 10",
        windows: WindowsVersion::Win10,
        arch: WineArch::Win32,
        wow64: false,
        d3d_levels: &["9", "10", "11", "12"],
    },
    BottleTemplate {
        key: "winxp_64",
        label_ar: "ويندوز XP (64-بت)",
        windows: WindowsVersion::WinXp,
        arch: WineArch::Win64,
        wow64: true,
        d3d_levels: &["9"],
    },
    BottleTemplate {
        key: "winvista_64",
        label_ar: "ويندوز فيستا (64-بت)",
        windows: WindowsVersion::WinVista,
        arch: WineArch::Win64,
        wow64: true,
        d3d_levels: &["9", "10"],
    },
    BottleTemplate {
        key: "win7_64",
        label_ar: "ويندوز 7 (64-بت)",
        windows: WindowsVersion::Win7,
        arch: WineArch::Win64,
        wow64: true,
        d3d_levels: &["9", "10", "11"],
    },
    BottleTemplate {
        key: "win8_64",
        label_ar: "ويندوز 8 (64-بت)",
        windows: WindowsVersion::Win8,
        arch: WineArch::Win64,
        wow64: true,
        d3d_levels: &["9", "10", "11"],
    },
    BottleTemplate {
        key: "win10_64",
        label_ar: "ويندوز 10 (64-بت)",
        windows: WindowsVersion::Win10,
        arch: WineArch::Win64,
        wow64: true,
        d3d_levels: &["9", "10", "11", "12"],
    },
    BottleTemplate {
        key: "win11_64",
        label_ar: "ويندوز 11 (64-بت)",
        windows: WindowsVersion::Win11,
        arch: WineArch::Win64,
        wow64: true,
        d3d_levels: &["9", "10", "11", "12"],
    },
];

/// Look a template up by key.
pub fn get(key: &str) -> Option<&'static BottleTemplate> {
    let needle = key.trim().to_ascii_lowercase();
    TEMPLATES.iter().find(|t| t.key == needle)
}

/// The steps that create a bottle, in order.
///
/// Each step is a small description so the UI can show progress and the
/// runner can execute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateStep {
    /// Write the bottle's own configuration file.
    WriteBottleConfig,
    /// Create the prefix skeleton (registry files, `dosdevices`, `drive_c`).
    CreatePrefix,
    /// Run the INF install sections (once for 32-bit bottles, twice for Wow64).
    RunInf { pass: u8 },
    /// Restore the `y:` drive the installer may have remapped.
    RestoreDosDevices,
    /// Run `wineboot` to finish the prefix.
    Wineboot,
}

/// Build the creation plan for a template.
pub fn create_plan(template: &BottleTemplate) -> Vec<CreateStep> {
    let mut steps = vec![CreateStep::WriteBottleConfig, CreateStep::CreatePrefix];
    if template.wow64 {
        // Their trick: run the INF twice so the Wow64 side gets the same
        // registry writes (rundll32 is 32-bit on macOS under Rosetta).
        steps.push(CreateStep::RunInf { pass: 1 });
        steps.push(CreateStep::RunInf { pass: 2 });
    } else {
        steps.push(CreateStep::RunInf { pass: 1 });
    }
    steps.push(CreateStep::RestoreDosDevices);
    steps.push(CreateStep::Wineboot);
    steps
}

/// The registry writes a template needs before the first run.
pub fn registry_plan(template: &BottleTemplate) -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "HKCU\\Software\\Wine",
            "Version",
            template.windows.registry_value().to_string(),
        ),
        (
            "HKCU\\Software\\Wine",
            "WindowsVersion",
            template.windows.build().to_string(),
        ),
    ]
}

/// The INF document for a template.
///
/// It only carries what a bottle actually needs: the Direct3D components and
/// the fonts Windows games ask for by name.
pub fn inf_document(template: &BottleTemplate) -> String {
    let mut out = String::new();
    out.push_str("[Version]\n");
    out.push_str(&format!("Signature=\"$WINDOWS NT$\"\n"));
    out.push_str(&format!(
        "Class=MS_Windows95\nClassDesc={}\n",
        template.label_ar
    ));
    out.push_str(&format!("ProviderName=Jisr\n"));
    out.push_str("ProductName=Jisr bottle\n");
    out.push_str("\n[Install]\n");
    out.push_str(&format!(
        "Win98.DLLs.D3D9,D3D10Core,D3D11,d3dcompiler_43,d3dx9_43,d3dx10_43,d3dx11_43\n"
    ));
    out.push_str("Win98.Fonts.Fonts.Marlett\n");
    out.push_str("Win98.Fonts.Fonts.Microssans\n");
    out.push_str("Win98.Fonts.Fonts.Tahoma\n");
    out.push_str("Win98.Fonts.Fonts.TimesNewRoman\n");
    out.push_str("Win98.Fonts.Fonts.TahomaBD\n");
    out.push_str("Win98.Fonts.Fonts.Arial\n");
    out.push_str("Win98.Fonts.Fonts.CourierNew\n");
    out.push_str("Win98.Fonts.Fonts.SegoeUI\n");
    out.push_str("\n[DefaultInstall]\n");
    out.push_str("Win98.DLLs.D3D9,D3D10Core,D3D11,d3dcompiler_43,d3dx9_43,d3dx10_43,d3dx11_43\n");
    out.push_str("Win98.Fonts.Fonts.Marlett,Microssans,Tahoma,TimesNewRoman,TahomaBD,Arial,CourierNew,SegoeUI\n");
    out
}

/// The bottle-local configuration written for a new bottle.
pub fn bottle_config(template: &BottleTemplate, bottle_id: &str, timestamp: &str) -> String {
    let mut out = String::new();
    out.push_str("; Jisr bottle configuration (written atomically)\n\n");
    out.push_str("[Bottle]\n");
    out.push_str(&format!("BottleID={bottle_id}\n"));
    out.push_str(&format!("Timestamp={timestamp}\n"));
    out.push_str("Encoding=UTF-8\n");
    out.push_str(&format!("Template={}\n", template.key));
    out.push_str(&format!("WineArch={}\n", template.arch.as_str()));
    out.push_str("\n[ManagedUpdatePolicy]\n");
    out
}

/// Which Windows version a bottle currently reports, from its config.
pub fn windows_version_from_config(config: &str) -> Option<WindowsVersion> {
    let mut value = None;
    for line in config.lines() {
        let line = line.trim();
        if let Some((key, val)) = line.split_once('=') {
            if key.trim().eq_ignore_ascii_case("Template") {
                value = get(val).map(|t| t.windows);
            }
        }
    }
    value
}

/// Suggested graphics backend for a template, from the Direct3D levels it
/// configures: a template that configures D3D12 can use the accelerated path.
pub fn suggested_backend(template: &BottleTemplate) -> crate::graphics_backend::GraphicsBackend {
    if template.d3d_levels.contains(&"12") {
        crate::graphics_backend::GraphicsBackend::Dxvk
    } else {
        crate::graphics_backend::GraphicsBackend::Dxvk
    }
}

/// Default file associations for a template (the extension list games use).
pub fn default_associations(template: &BottleTemplate) -> BTreeMap<&'static str, &'static str> {
    let mut map = BTreeMap::new();
    let defaults: &[&str] = &[
        ".asf", ".cfc", ".doc", ".dot", ".mdb", ".moh", ".mpw", ".mpx", ".pdf", ".pic", ".pict",
        ".ppt", ".prj", ".psd", ".rtf", ".tiff", ".txt", ".xls",
    ];
    for ext in defaults {
        map.insert(*ext, "winemenubuilder.exe");
    }
    map.insert("exe", "winemenubuilder.exe");
    let _ = template;
    map
}

/// Default template when a bottle never declared one.
pub const DEFAULT_TEMPLATE: &str = "win10_64";

/// The template a bottle declares, or the default when it declares nothing
/// (or something we no longer ship).
pub fn template_of(prefix: &Path) -> &'static BottleTemplate {
    crate::bottle_settings::read_setting(prefix, "Template")
        .and_then(|key| get(&key))
        .unwrap_or_else(|| get(DEFAULT_TEMPLATE).expect("default template must exist"))
}

/// Write the bottle configuration file when it is absent. Never overwrites an
/// existing file, so a re-provision cannot reset a user's bottle.
fn ensure_config(prefix: &Path, bottle_id: &str) -> Result<PathBuf, String> {
    let path = crate::bottle_settings::settings_path(prefix);
    if path.exists() {
        return Ok(path);
    }
    let template = template_of(prefix);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string());
    std::fs::create_dir_all(prefix).map_err(|e| format!("تعذّر إنشاء مجلد القارورة: {e}"))?;
    std::fs::write(&path, bottle_config(template, bottle_id, &timestamp))
        .map_err(|e| format!("تعذّر كتابة إعداد القارورة: {e}"))?;
    Ok(path)
}

/// Where the generated INF lands while it is being applied.
fn inf_path(prefix: &Path) -> PathBuf {
    prefix.join("jisr-template.inf")
}

/// One `wine` invocation against a prefix, output captured.
fn wine_run(wine: &Path, prefix: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new(wine)
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .args(args)
        .output()
        .map_err(|e| format!("تعذّر تشغيل Wine: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail = stderr
        .lines()
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .unwrap_or("بلا تفاصيل");
    Err(format!(
        "فشل {} (رمز {}): {}",
        args.join(" "),
        out.status.code().unwrap_or(-1),
        tail.trim()
    ))
}

/// Apply a template to a prefix that `wineboot` has already created.
///
/// This is the part that makes a bottle a *personality* rather than a bare
/// prefix: the declared Windows version goes into the registry, the INF pass
/// installs the Direct3D components and fonts, and the `y:` drive is restored
/// because the INF pass remaps it. Wow64 bottles run the INF twice so the
/// 32-bit side receives the same registry writes.
pub fn provision(wine: &Path, prefix: &Path, bottle_id: &str) -> Result<(), String> {
    let template = template_of(prefix);
    ensure_config(prefix, bottle_id)?;

    for (key, name, value) in registry_plan(template) {
        wine_run(
            wine,
            prefix,
            &["reg", "add", key, "/v", name, "/d", &value, "/f"],
        )?;
    }

    let inf = inf_path(prefix);
    std::fs::write(&inf, inf_document(template))
        .map_err(|e| format!("تعذّر كتابة INF القالب: {e}"))?;
    let inf_arg = inf.to_string_lossy().to_string();
    let passes = if template.wow64 { 2 } else { 1 };
    let mut last_error = None;
    for _ in 0..passes {
        if let Err(e) = wine_run(
            wine,
            prefix,
            &[
                "rundll32.exe",
                "setupapi.dll,InstallHinfSection",
                "DefaultInstall",
                "132",
                &inf_arg,
            ],
        ) {
            last_error = Some(e);
        }
    }
    let _ = std::fs::remove_file(&inf);
    if let Some(e) = last_error {
        return Err(e);
    }

    restore_dos_devices(prefix)?;
    Ok(())
}

/// Re-point `y:` at the prefix's own temp directory.
///
/// `y:` is the drive the INF pass uses as scratch. Leaving it pointing at the
/// host root would hand the prefix a window onto the whole filesystem, so we
/// restore it to something local and harmless.
fn restore_dos_devices(prefix: &Path) -> Result<(), String> {
    let devices = prefix.join("dosdevices");
    std::fs::create_dir_all(&devices).map_err(|e| format!("تعذّر إنشاء dosdevices: {e}"))?;
    let target = prefix.join("drive_c").join("windows").join("temp");
    std::fs::create_dir_all(&target).map_err(|e| format!("تعذّر إنشاء temp: {e}"))?;
    let link = devices.join("y:");
    if std::fs::symlink_metadata(&link).is_ok() {
        let _ = std::fs::remove_file(&link);
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).map_err(|e| format!("تعذّر استعادة y:: {e}"))?;
    #[cfg(not(unix))]
    let _ = (target, link);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ships_thirteen_templates() {
        assert_eq!(TEMPLATES.len(), 13);
    }

    #[test]
    fn every_template_has_a_unique_key() {
        let mut keys: Vec<&str> = TEMPLATES.iter().map(|t| t.key).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count, "duplicate template keys");
    }

    #[test]
    fn lookup_is_case_insensitive_and_rejects_unknown_keys() {
        assert_eq!(get("win11_64").map(|t| t.key), Some("win11_64"));
        assert_eq!(get(" WIN10_64 ").map(|t| t.key), Some("win10_64"));
        assert!(get("win12").is_none());
        assert!(get("../etc").is_none());
    }

    #[test]
    fn seven_templates_are_32_bit_and_six_are_64_bit() {
        let win32 = TEMPLATES
            .iter()
            .filter(|t| t.arch == WineArch::Win32)
            .count();
        let win64 = TEMPLATES
            .iter()
            .filter(|t| t.arch == WineArch::Win64)
            .count();
        assert_eq!(win32, 7);
        assert_eq!(win64, 6);
    }

    #[test]
    fn wow64_bottles_run_the_inf_twice() {
        let plan = create_plan(get("win10_64").unwrap());
        let inf_passes = plan
            .iter()
            .filter(|s| matches!(s, CreateStep::RunInf { .. }))
            .count();
        assert_eq!(inf_passes, 2, "64-bit bottles install the INF twice");
        let plan32 = create_plan(get("win10").unwrap());
        assert_eq!(
            plan32
                .iter()
                .filter(|s| matches!(s, CreateStep::RunInf { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn creation_starts_with_config_and_ends_with_wineboot() {
        let plan = create_plan(get("win7_64").unwrap());
        assert_eq!(plan.first(), Some(&CreateStep::WriteBottleConfig));
        assert_eq!(plan.last(), Some(&CreateStep::Wineboot));
        assert!(plan.contains(&CreateStep::RestoreDosDevices));
    }

    #[test]
    fn registry_plan_sets_the_windows_version() {
        let plan = registry_plan(get("win10_64").unwrap());
        assert!(plan.iter().any(|(key, name, value)| key.contains("Wine")
            && *name == "Version"
            && value == "win10"));
    }

    #[test]
    fn inf_document_carries_directx_and_fonts() {
        let inf = inf_document(get("win10_64").unwrap());
        assert!(inf.contains("[DefaultInstall]"));
        assert!(inf.contains("d3dcompiler_43"));
        assert!(inf.contains("Fonts.Fonts.Tahoma"));
    }

    #[test]
    fn bottle_config_round_trips_the_template() {
        let config = bottle_config(get("win11_64").unwrap(), "abc123", "2026-10-21T00:00:00");
        assert!(config.contains("BottleID=abc123"));
        assert!(config.contains("Template=win11_64"));
        assert!(config.contains("WineArch=win64"));
        assert_eq!(
            windows_version_from_config(&config),
            Some(WindowsVersion::Win11)
        );
    }

    #[test]
    fn unknown_template_in_config_is_ignored() {
        assert_eq!(windows_version_from_config("Template=win42\n"), None);
    }

    #[test]
    fn default_associations_cover_common_game_formats() {
        let assoc = default_associations(get("win10_64").unwrap());
        assert!(assoc.contains_key(".pdf"));
        assert!(assoc.contains_key("exe"));
        assert_eq!(assoc.get(".pdf").copied(), Some("winemenubuilder.exe"));
    }

    #[test]
    fn d3d12_templates_are_marked_as_such() {
        assert!(get("win11_64").unwrap().d3d_levels.contains(&"12"));
        assert!(!get("winxp").unwrap().d3d_levels.contains(&"12"));
    }

    fn temp_prefix(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "jisr-template-{name}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn undeclared_bottle_falls_back_to_the_default_template() {
        let dir = temp_prefix("default");
        assert_eq!(template_of(&dir).key, DEFAULT_TEMPLATE);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn declared_template_wins() {
        let dir = temp_prefix("declared");
        crate::bottle_settings::write_setting(&dir, "Template", "winxp").unwrap();
        assert_eq!(template_of(&dir).key, "winxp");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_config_never_overwrites() {
        let dir = temp_prefix("config");
        let path = ensure_config(&dir, "b1").expect("write");
        let first = std::fs::read_to_string(&path).unwrap();
        assert!(first.contains("BottleID=b1"));

        crate::bottle_settings::write_setting(&dir, "GraphicsBackend", "dxvk").unwrap();
        ensure_config(&dir, "b1").expect("second call");
        let second = std::fs::read_to_string(&path).unwrap();
        assert!(second.contains("GraphicsBackend=dxvk"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn provisioning_writes_a_config_then_fails_loudly_without_wine() {
        let dir = temp_prefix("nowine");
        let missing = dir.join("no-such-wine");
        let err = provision(&missing, &dir, "b2").expect_err("no wine binary");
        assert!(err.contains("Wine"));
        assert!(
            crate::bottle_settings::settings_path(&dir).exists(),
            "config is written before the first Wine call"
        );
        assert!(!inf_path(&dir).exists(), "INF must not be left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
