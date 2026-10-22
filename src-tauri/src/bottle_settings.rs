//! Per-bottle settings file.
//!
//! Every bottle keeps its own small INI file inside the prefix
//! (`<prefix>/jisr.conf`). It holds the settings that belong to *that*
//! bottle — the graphics backend, msync, and whatever we add later.
//!
//! Format: one `[Jisr]` section plus any extra sections a bottle may carry,
//! written atomically (temp file + rename) so a crash never leaves half a file.
//!
//! Migration: bottles created by earlier builds stored the same keys in
//! `crossover.conf`. [`load_text`] falls back to that file when `jisr.conf`
//! is absent, and the first write moves the values over and removes the
//! legacy file — so no setting is ever lost on upgrade.

use std::path::{Path, PathBuf};

/// Section that owns our keys.
pub const SECTION: &str = "Jisr";

/// Legacy file name kept for one-way migration only.
const LEGACY_FILE: &str = "crossover.conf";

/// Where the settings of a bottle live.
pub fn settings_path(prefix: &Path) -> PathBuf {
    prefix.join("jisr.conf")
}

fn legacy_path(prefix: &Path) -> PathBuf {
    prefix.join(LEGACY_FILE)
}

/// Read the settings text, preferring the current file and falling back to
/// the legacy one. Returns the path that was actually read.
fn load_text(prefix: &Path) -> Option<(PathBuf, String)> {
    let current = settings_path(prefix);
    if let Ok(text) = std::fs::read_to_string(&current) {
        return Some((current, text));
    }
    let legacy = legacy_path(prefix);
    std::fs::read_to_string(&legacy)
        .ok()
        .map(|text| (legacy, text))
}

/// Section names that carry our keys (current + legacy only).
fn is_our_section(name: &str) -> bool {
    name.eq_ignore_ascii_case(SECTION)
        || name.eq_ignore_ascii_case(LEGACY_FILE.trim_end_matches(".conf"))
}

/// Read a setting from a bottle.
pub fn read_setting(prefix: &Path, key: &str) -> Option<String> {
    let (_, text) = load_text(prefix)?;
    let mut current_section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((section, _)) = line.split_once('[').and_then(|(_, r)| r.split_once(']')) {
            current_section = section.trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if is_our_section(&current_section) && k.trim().eq_ignore_ascii_case(key) {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// Parse an INI body into ordered sections.
fn parse_sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((section, _)) = line.split_once('[').and_then(|(_, r)| r.split_once(']')) {
            current = Some(sections.len());
            sections.push((section.trim().to_string(), Vec::new()));
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let Some(index) = current else {
            continue;
        };
        sections[index]
            .1
            .push((k.trim().to_string(), v.trim().to_string()));
    }
    sections
}

/// Rewrite every legacy section name to the current one, merging duplicates.
fn normalize_sections(
    sections: Vec<(String, Vec<(String, String)>)>,
) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for (name, fields) in sections {
        let name = if is_our_section(&name) {
            SECTION.to_string()
        } else {
            name
        };
        match out.iter_mut().find(|(existing, _)| existing == &name) {
            Some((_, into)) => into.extend(fields),
            None => out.push((name, fields)),
        }
    }
    out
}

/// Write a setting into a bottle (keeps the other settings).
pub fn write_setting(prefix: &Path, key: &str, value: &str) -> Result<(), String> {
    let path = settings_path(prefix);
    let read_from = load_text(prefix);
    let mut sections = match &read_from {
        Some((_, text)) => normalize_sections(parse_sections(text)),
        None => Vec::new(),
    };

    match sections.iter().position(|(name, _)| name == SECTION) {
        Some(index) => {
            let fields = &mut sections[index].1;
            match fields.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
                Some(entry) => entry.1 = value.to_string(),
                None => fields.push((key.to_string(), value.to_string())),
            }
        }
        None => sections.push((
            SECTION.to_string(),
            vec![(key.to_string(), value.to_string())],
        )),
    }

    let mut out = String::new();
    out.push_str("; Jisr bottle settings (bottle-local, written atomically)\n");
    for (section, fields) in sections {
        out.push_str(&format!("[{section}]\n"));
        for (key, value) in fields {
            out.push_str(&format!("{key}={value}\n"));
        }
        out.push('\n');
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("تعذّر إنشاء المجلد: {e}"))?;
    }
    let tmp = path.with_extension("conf.tmp");
    std::fs::write(&tmp, out).map_err(|e| format!("تعذّر الكتابة: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("تعذّر استبدال الملف: {e}"))?;

    // Migration is complete once the new file exists: drop the legacy one.
    if let Some((legacy, _)) = read_from {
        if legacy != path {
            let _ = std::fs::remove_file(legacy);
        }
    }
    Ok(())
}

/// All settings of a bottle, as key/value pairs of the `[Jisr]` section.
pub fn all_settings(prefix: &Path) -> Vec<(String, String)> {
    let Some((_, text)) = load_text(prefix) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((section, _)) = line.split_once('[').and_then(|(_, r)| r.split_once(']')) {
            current = section.trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if is_our_section(&current) {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "jisr-settings-{name}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_and_writes_settings() {
        let dir = temp_dir("rw");
        assert_eq!(read_setting(&dir, "GraphicsBackend"), None);
        write_setting(&dir, "GraphicsBackend", "dxvk").unwrap();
        assert_eq!(
            read_setting(&dir, "GraphicsBackend").as_deref(),
            Some("dxvk")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_other_settings_and_sections() {
        let dir = temp_dir("keep");
        std::fs::write(
            settings_path(&dir),
            "[Jisr]\nGraphicsBackend=dxvk\n\n[Custom]\nKey=Value\n",
        )
        .unwrap();
        write_setting(&dir, "Msync", "1").unwrap();
        let text = std::fs::read_to_string(settings_path(&dir)).unwrap();
        assert!(text.contains("[Custom]"));
        assert!(text.contains("Key=Value"));
        assert!(text.contains("GraphicsBackend=dxvk"));
        assert!(text.contains("Msync=1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrites_an_existing_value() {
        let dir = temp_dir("over");
        write_setting(&dir, "GraphicsBackend", "dxvk").unwrap();
        write_setting(&dir, "GraphicsBackend", "d3dmetal").unwrap();
        assert_eq!(
            read_setting(&dir, "GraphicsBackend").as_deref(),
            Some("d3dmetal")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lists_all_settings() {
        let dir = temp_dir("list");
        write_setting(&dir, "GraphicsBackend", "dxvk").unwrap();
        write_setting(&dir, "Msync", "1").unwrap();
        let all = all_settings(&dir);
        assert_eq!(all.len(), 2);
        assert!(
            all.iter()
                .any(|(k, v)| k == "GraphicsBackend" && v == "dxvk")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_legacy_file_on_first_write() {
        let dir = temp_dir("migrate");
        std::fs::write(
            legacy_path(&dir),
            "; old\n[Bottle]\nTemplate=win10_64\n\n[CrossOver]\nGraphicsBackend=dxmt\nMsync=0\n",
        )
        .unwrap();
        assert_eq!(
            read_setting(&dir, "GraphicsBackend").as_deref(),
            Some("dxmt")
        );
        write_setting(&dir, "Msync", "1").unwrap();
        assert!(!legacy_path(&dir).exists(), "legacy file must be gone");
        let text = std::fs::read_to_string(settings_path(&dir)).unwrap();
        assert!(text.contains("[Jisr]"));
        assert!(text.contains("GraphicsBackend=dxmt"));
        assert!(text.contains("Msync=1"));
        assert!(text.contains("[Bottle]"));
        assert!(!text.to_lowercase().contains("[crossover]"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_sections_are_merged_into_one() {
        let dir = temp_dir("merge");
        std::fs::write(legacy_path(&dir), "[CrossOver]\nA=1\n\n[crossover]\nB=2\n").unwrap();
        write_setting(&dir, "C", "3").unwrap();
        let all = all_settings(&dir);
        assert_eq!(all.len(), 3);
        assert!(
            all.iter()
                .all(|(k, _)| ["A", "B", "C"].contains(&k.as_str()))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
