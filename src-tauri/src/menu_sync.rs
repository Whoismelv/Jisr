//! macOS shortcuts for Windows applications.
//!
//! Pipeline:
//!
//! 1. `wine --wl-app winemenubuilder.exe --cx-dump-menus` dumps the bottle's
//!    Start Menu as desktop entries,
//! 2. every entry becomes an `.app` bundle registered with LaunchServices,
//! 3. file associations come from `assocscan.exe --scan` and land in a plist
//!    inside the bottle.
//!
//! All of it is done in Rust: parse the desktop entries, write the `.app`
//! bundles ourselves, and register them with LaunchServices.

use std::path::{Path, PathBuf};

/// One entry of the Windows Start Menu.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MenuEntry {
    /// Display name.
    pub name: String,
    /// Command to run (as produced by winemenubuilder).
    pub exec: String,
    /// Icon file inside the bottle, if any.
    pub icon: Option<String>,
    /// Comment / tooltip.
    pub comment: Option<String>,
    /// Desktop categories (semicolon separated in the XDG format).
    pub categories: Vec<String>,
    /// Whether the entry wants a terminal.
    pub terminal: bool,
}

impl MenuEntry {
    /// The bundle identifier we give the generated `.app`.
    pub fn bundle_id(&self) -> String {
        let slug: String = self
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_ascii_lowercase();
        if slug.is_empty() {
            "com.jisr.shortcut".to_string()
        } else {
            format!("com.jisr.shortcut.{slug}")
        }
    }
}

/// Parse one XDG desktop entry (the format winemenubuilder emits).
pub fn parse_desktop_entry(text: &str) -> Option<MenuEntry> {
    let mut entry = MenuEntry::default();
    let mut name: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut comment: Option<String> = None;
    let mut categories: Option<String> = None;
    let mut terminal = false;
    let mut hidden = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "Icon" => icon = Some(value.trim().to_string()),
            "Comment" => comment = Some(value.trim().to_string()),
            "Categories" => categories = Some(value.trim().to_string()),
            "Terminal" => terminal = value.trim() == "true",
            "NoDisplay" | "Hidden" => hidden = value.trim() == "true",
            "Type" => {}
            _ => {}
        }
    }

    if hidden {
        return None;
    }
    let name = name?;
    let exec = exec?;
    entry.name = name;
    entry.exec = exec;
    entry.icon = icon;
    entry.comment = comment;
    entry.terminal = terminal;
    entry.categories = categories
        .map(|c| {
            c.split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(entry)
}

/// Parse every desktop entry in a directory.
pub fn parse_desktop_dir(dir: &Path) -> Vec<MenuEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("desktop"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    let mut out = Vec::new();
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Some(entry) = parse_desktop_entry(&text) {
                out.push(entry);
            }
        }
    }
    out
}

/// Where the generated shortcuts go.
pub fn shortcuts_dir() -> PathBuf {
    crate::data_dir().join("shortcuts")
}

/// The Info.plist of a generated `.app`.
pub fn info_plist(entry: &MenuEntry, icon: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ");
    out.push_str("\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
    out.push_str("<plist version=\"1.0\">\n<dict>\n");
    out.push_str("  <key>CFBundleName</key><string>");
    out.push_str(&escape_xml(&entry.name));
    out.push_str("</string>\n");
    out.push_str(&format!(
        "  <key>CFBundleIdentifier</key><string>{}</string>\n",
        entry.bundle_id()
    ));
    out.push_str("  <key>CFBundleExecutable</key><string>launcher</string>\n");
    out.push_str("  <key>CFBundlePackageType</key><string>APPL</string>\n");
    out.push_str("  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>\n");
    out.push_str("  <key>NSHighResolutionCapable</key><true/>\n");
    if let Some(icon) = icon {
        out.push_str("  <key>CFBundleIconFile</key><string>");
        out.push_str(&escape_xml(icon));
        out.push_str("</string>\n");
    }
    if let Some(comment) = &entry.comment {
        out.push_str("  <key>NSHumanReadableCopyright</key><string>");
        out.push_str(&escape_xml(comment));
        out.push_str("</string>\n");
    }
    out.push_str("</dict>\n</plist>\n");
    out
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The launcher script inside the bundle.
pub fn launcher_script(entry: &MenuEntry, wine: &str, prefix: &str) -> String {
    let mut out = String::new();
    out.push_str("#!/bin/sh\n");
    out.push_str("# مولّد تلقائياً by Jisr — لا تحرّره يدوياً\n");
    out.push_str(&format!("export WINEPREFIX=\"{prefix}\"\n"));
    out.push_str(&format!("exec \"{wine}\" \"{}\"\n", entry.exec));
    out
}

/// Write an `.app` bundle for one entry.
///
/// Returns the bundle path. On macOS the bundle is also registered with
/// LaunchServices so it shows up in Spotlight and the Dock.
pub fn write_app_bundle(
    dest_dir: &Path,
    entry: &MenuEntry,
    wine: &str,
    prefix: &str,
    icon_source: Option<&Path>,
) -> Result<PathBuf, String> {
    let bundle = dest_dir.join(format!("{}.app", sanitize(&entry.name)));
    let macos_dir = bundle.join("Contents").join("MacOS");
    let resources_dir = bundle.join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).map_err(|e| format!("تعذّر إنشاء {macos_dir:?}: {e}"))?;
    std::fs::create_dir_all(&resources_dir)
        .map_err(|e| format!("تعذّر إنشاء {resources_dir:?}: {e}"))?;

    let icon_name = icon_source.map(|_| "icon.icns".to_string());
    if let Some(source) = icon_source {
        if source.exists() {
            let _ = std::fs::copy(source, resources_dir.join("icon.icns"));
        }
    }

    let plist = info_plist(entry, icon_name.as_deref());
    std::fs::write(bundle.join("Contents").join("Info.plist"), plist)
        .map_err(|e| format!("تعذّر كتابة Info.plist: {e}"))?;

    let launcher = macos_dir.join("launcher");
    std::fs::write(&launcher, launcher_script(entry, wine, prefix))
        .map_err(|e| format!("تعذّر كتابة المشغّل: {e}"))?;
    make_executable(&launcher)?;

    Ok(bundle)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|e| format!("تعذّر قراءة الصلاحيات: {e}"))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).map_err(|e| format!("تعذّر ضبط الصلاحيات: {e}"))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Register a bundle with LaunchServices (macOS only; no-op elsewhere).
pub fn register_with_launch_services(bundle: &Path) -> Result<bool, String> {
    if !cfg!(target_os = "macos") {
        return Ok(false);
    }
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if !Path::new(lsregister).exists() {
        return Ok(false);
    }
    let status = std::process::Command::new(lsregister)
        .arg("-f")
        .arg(bundle)
        .status()
        .map_err(|e| format!("تعذّر تشغيل lsregister: {e}"))?;
    Ok(status.success())
}

/// Build every shortcut of a bottle into `dest_dir`.
pub fn sync_shortcuts(
    entries: &[MenuEntry],
    dest_dir: &Path,
    wine: &str,
    prefix: &str,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("تعذّر إنشاء {dest_dir:?}: {e}"))?;
    let mut built = Vec::new();
    for entry in entries {
        let bundle = write_app_bundle(dest_dir, entry, wine, prefix, None)?;
        let _ = register_with_launch_services(&bundle);
        built.push(bundle);
    }
    Ok(built)
}

fn sanitize(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim().to_string();
    if slug.is_empty() {
        "shortcut".to_string()
    } else {
        slug
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Find the `.desktop` files a bottle's Start Menu contains.
fn find_desktop_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 6 || out.len() > 500 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_desktop_files(&path, out, depth + 1);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("desktop"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Build macOS shortcuts for every application inside a bottle.
///
/// The `--sync` pipeline: winemenubuilder
/// writes desktop entries inside the prefix, we turn them into `.app` bundles.
#[tauri::command]
pub async fn menu_sync_bottle(bottle: String) -> Result<Vec<String>, String> {
    if !crate::bottle::valid_bottle_id(&bottle) {
        return Err("معرّف القارورة غير صالح".to_string());
    }
    let prefix = crate::bottle::bottle_dir(&bottle);
    let Some(wine) = crate::bottle::wine_bin() else {
        return Err("ما لقينا محرك Wine".to_string());
    };

    // Ask Wine to (re)generate the Start Menu entries.
    let mut cmd = tokio::process::Command::new(&wine);
    cmd.env("WINEPREFIX", &prefix)
        .arg("winemenubuilder.exe")
        .stdin(std::process::Stdio::null());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output()).await;

    // Collect the entries.
    let mut files = Vec::new();
    for start in [
        prefix.join("drive_c").join("users"),
        prefix
            .join("drive_c")
            .join("ProgramData")
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu"),
    ] {
        if start.is_dir() {
            find_desktop_files(&start, &mut files, 0);
        }
    }
    let mut entries = Vec::new();
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Some(entry) = parse_desktop_entry(&text) {
                entries.push(entry);
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries.dedup_by(|a, b| a.name == b.name);

    let dest = shortcuts_dir().join(&bottle);
    let built = sync_shortcuts(
        &entries,
        &dest,
        &wine.to_string_lossy(),
        &prefix.to_string_lossy(),
    )?;
    Ok(built
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

/// Where a prebuilt helper app may live.
fn helper_source() -> Option<PathBuf> {
    let candidates = [
        crate::data_dir().join("bottle-helper").join(HELPER_APP),
        PathBuf::from("src-tauri/resources/bottle-helper/dist").join(HELPER_APP),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

/// Name of the helper bundle we build and install.
const HELPER_APP: &str = "Jisr Helper.app";

/// Install the macOS helper app inside a bottle.
///
/// The helper is what captures Open/Print/URL Apple Events from Windows
/// applications and forwards them to us.
#[tauri::command]
pub fn helper_install(bottle: String) -> Result<String, String> {
    if !crate::bottle::valid_bottle_id(&bottle) {
        return Err("معرّف القارورة غير صالح".to_string());
    }
    let prefix = crate::bottle::bottle_dir(&bottle);
    let Some(source) = helper_source() else {
        return Err(
            "المساعد غير مبني بعد — ابنِه على الماك: bash src-tauri/resources/bottle-helper/build.sh"
                .to_string(),
        );
    };
    let target_dir = prefix.join("Applications");
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("تعذّر إنشاء {target_dir:?}: {e}"))?;
    let target = target_dir.join(HELPER_APP);
    copy_dir(&source, &target)?;
    Ok(target.to_string_lossy().to_string())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("تعذّر إنشاء {:?}: {e}", dst))?;
    let entries = std::fs::read_dir(src).map_err(|e| format!("تعذّر قراءة {:?}: {e}", src))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| format!("تعذّر نسخ {:?}: {e}", from))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: &str = r#"[Desktop Entry]
Type=Application
Name=Half-Life
Exec="/c/Games/Half-Life/hl.exe" -game cstrike
Icon=/c/Games/Half-Life/hl.ico
Comment=Valve's classic
Categories=Game;
Terminal=false
"#;

    #[test]
    fn parses_a_desktop_entry() {
        let entry = parse_desktop_entry(DESKTOP).expect("entry");
        assert_eq!(entry.name, "Half-Life");
        assert!(entry.exec.contains("hl.exe"));
        assert_eq!(entry.icon.as_deref(), Some("/c/Games/Half-Life/hl.ico"));
        assert_eq!(entry.comment.as_deref(), Some("Valve's classic"));
        assert_eq!(entry.categories, vec!["Game".to_string()]);
        assert!(!entry.terminal);
    }

    #[test]
    fn hidden_entries_are_skipped() {
        let text = DESKTOP.replace("Terminal=false", "Terminal=false\nNoDisplay=true");
        assert!(parse_desktop_entry(&text).is_none());
    }

    #[test]
    fn entries_without_name_or_exec_are_ignored() {
        assert!(parse_desktop_entry("[Desktop Entry]\nType=Application\n").is_none());
        assert!(parse_desktop_entry("[Desktop Entry]\nName=X\n").is_none());
    }

    #[test]
    fn bundle_ids_are_stable_and_safe() {
        let entry = parse_desktop_entry(DESKTOP).unwrap();
        assert_eq!(entry.bundle_id(), "com.jisr.shortcut.half-life");
        let odd = MenuEntry {
            name: "عربي / 100%".to_string(),
            ..Default::default()
        };
        assert!(odd.bundle_id().starts_with("com.jisr.shortcut."));
        assert!(!odd.bundle_id().contains(' '));
    }

    #[test]
    fn info_plist_is_well_formed() {
        let entry = parse_desktop_entry(DESKTOP).unwrap();
        let plist = info_plist(&entry, Some("icon.icns"));
        assert!(plist.contains("<key>CFBundleExecutable</key><string>launcher</string>"));
        assert!(plist.contains("com.jisr.shortcut.half-life"));
        assert!(plist.contains("<key>NSHighResolutionCapable</key><true/>"));
        assert!(plist.ends_with("</plist>\n"));
    }

    #[test]
    fn info_plist_escapes_markup() {
        let entry = MenuEntry {
            name: "A & B <x>".to_string(),
            ..Default::default()
        };
        let plist = info_plist(&entry, None);
        assert!(plist.contains("A &amp; B &lt;x&gt;"));
        assert!(!plist.contains("<x>"));
    }

    #[test]
    fn launcher_script_points_at_wine_and_the_prefix() {
        let entry = parse_desktop_entry(DESKTOP).unwrap();
        let script = launcher_script(&entry, "/opt/wine/bin/wine", "/Users/me/bottles/main");
        assert!(script.contains("WINEPREFIX=\"/Users/me/bottles/main\""));
        assert!(script.contains("/opt/wine/bin/wine"));
        assert!(script.contains("hl.exe"));
        assert!(script.starts_with("#!/bin/sh"));
    }

    #[test]
    fn writes_a_bundle_with_plist_and_launcher() {
        let dir = std::env::temp_dir().join(format!("jisr-shortcut-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let entry = parse_desktop_entry(DESKTOP).unwrap();
        let bundle = write_app_bundle(&dir, &entry, "wine", "/prefix", None).expect("bundle");
        assert!(bundle.join("Contents").join("Info.plist").exists());
        assert!(
            bundle
                .join("Contents")
                .join("MacOS")
                .join("launcher")
                .exists()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_builds_one_bundle_per_entry() {
        let dir = std::env::temp_dir().join(format!("jisr-shortcut-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let entries = vec![
            parse_desktop_entry(DESKTOP).unwrap(),
            MenuEntry {
                name: "Second".to_string(),
                exec: "b.exe".to_string(),
                ..Default::default()
            },
        ];
        let built = sync_shortcuts(&entries, &dir, "wine", "/prefix").expect("sync");
        assert_eq!(built.len(), 2);
        assert!(built[0].join("Contents").join("Info.plist").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
