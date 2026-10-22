//! Diagnostics and fixes.
//!
//! A rules file (308 KB-class problem in the wild: 268 fixes across 98 Linux
//! distributions) shrinks to what actually bites on macOS — and every check is
//! a pure function over a [`Probe`] so it can be unit-tested without a Mac.
//!
//! Checks:
//! * missing GStreamer (a hard Wine requirement for media),
//! * 32-bit support missing (a lot of legacy games need it),
//! * the renderer left on the slow OpenGL path,
//! * DXVK not deployed although a DirectX title is installed,
//! * Rosetta missing (the whole engine is x86_64),
//! * msync not enabled (input lag in some titles).
//!
//! The rules live in `fixes.xml` next to the app.

use std::collections::BTreeMap;
use std::path::Path;

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warning => "warning",
            Level::Error => "error",
        }
    }

    fn parse(raw: &str) -> Level {
        match raw.trim().to_ascii_lowercase().as_str() {
            "error" => Level::Error,
            "warning" | "warn" => Level::Warning,
            _ => Level::Info,
        }
    }
}

/// One finding (a problem, plus how to fix it).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Issue {
    pub id: String,
    pub level: Level,
    pub title: String,
    pub description: String,
    /// What the user should do.
    pub fix: Option<String>,
    /// A command we can run (macOS) — shown to the user, never run silently.
    pub command: Option<String>,
}

/// A rule: a machine check plus the finding it produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub level: Level,
    pub title: String,
    pub description: String,
    pub fix: Option<String>,
    pub command: Option<String>,
    /// The check this rule applies to.
    pub check: String,
}

/// The rules database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FixesDb {
    pub rules: Vec<Rule>,
}

impl FixesDb {
    /// Parse a fixes file.
    pub fn parse(xml: &str) -> Result<FixesDb, String> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut db = FixesDb::default();
        let mut current: Option<Rule> = None;
        let mut buf = Vec::new();

        loop {
            let ev = reader
                .read_event_into(&mut buf)
                .map_err(|e| format!("XML error at byte {}: {e}", reader.error_position()))?;
            match ev {
                Event::Eof => break,
                Event::Start(ref e) | Event::Empty(ref e) => {
                    let name = e.name().as_ref().to_string();
                    let attrs = attributes(e)?;
                    let empty = matches!(ev, Event::Empty(_));
                    match name.as_str() {
                        "error" | "rule" => {
                            let rule = Rule {
                                id: attr(&attrs, "id").unwrap_or_default(),
                                level: Level::parse(&attr(&attrs, "level").unwrap_or_default()),
                                title: attr(&attrs, "title").unwrap_or_default(),
                                description: attr(&attrs, "desc").unwrap_or_default(),
                                fix: attr(&attrs, "fix"),
                                command: attr(&attrs, "command"),
                                check: attr(&attrs, "check").unwrap_or_default(),
                            };
                            if empty {
                                db.rules.push(rule);
                                current = None;
                            } else {
                                current = Some(rule);
                            }
                        }
                        "check" => {
                            let kind = attr(&attrs, "kind").unwrap_or_default();
                            if let Some(rule) = current.as_mut() {
                                if rule.check.is_empty() {
                                    rule.check = kind;
                                }
                            }
                            if empty {
                                if let Some(rule) = current.take() {
                                    db.rules.push(rule);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::End(ref e) => {
                    let name = e.name().as_ref().to_string();
                    if name == "error" || name == "rule" {
                        if let Some(rule) = current.take() {
                            db.rules.push(rule);
                        }
                    }
                }
                _ => {}
            }
            buf.clear();
        }
        if let Some(rule) = current.take() {
            db.rules.push(rule);
        }
        Ok(db)
    }

    /// Load the fixes file from a directory (optional).
    pub fn load_dir(dir: &Path) -> FixesDb {
        let path = dir.join("fixes.xml");
        match std::fs::read_to_string(&path) {
            Ok(text) => FixesDb::parse(&text).unwrap_or_default(),
            Err(_) => FixesDb::default(),
        }
    }
}

/// The machine state we check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Probe {
    /// Is GStreamer installed?
    pub gstreamer: bool,
    /// Does the engine support 32-bit Windows programs?
    pub has_32bit: bool,
    /// Is the renderer set to the fast Vulkan path?
    pub vulkan_renderer: bool,
    /// Is DXVK deployed into the prefix?
    pub dxvk_deployed: bool,
    /// Is Rosetta 2 installed (the engine is x86_64)?
    pub rosetta: bool,
    /// Is msync enabled?
    pub msync: bool,
    /// Does a DirectX title exist in the bottle?
    pub has_directx_title: bool,
}

/// Evaluate the rules against a probe.
pub fn diagnose(db: &FixesDb, probe: &Probe) -> Vec<Issue> {
    let mut issues = Vec::new();
    for rule in &db.rules {
        let hit = match rule.check.as_str() {
            "gstreamer" => !probe.gstreamer,
            "32bit" => !probe.has_32bit,
            "renderer" => !probe.vulkan_renderer,
            "dxvk" => probe.has_directx_title && !probe.dxvk_deployed,
            "rosetta" => !probe.rosetta,
            "msync" => !probe.msync,
            _ => false,
        };
        if !hit {
            continue;
        }
        issues.push(Issue {
            id: rule.id.clone(),
            level: rule.level,
            title: rule.title.clone(),
            description: rule.description.clone(),
            fix: rule.fix.clone(),
            command: rule.command.clone(),
        });
    }
    // Most severe first, then by id — stable output.
    issues.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.id.cmp(&b.id)));
    issues
}

/// The fixes we ship (our own rules, in our own schema).
pub const DEFAULT_FIXES: &str = include_str!("../resources/fixes/fixes.xml");

/// The fixes database that ships with the app.
pub fn default_db() -> FixesDb {
    FixesDb::parse(DEFAULT_FIXES).unwrap_or_default()
}

/// Where user fixes live.
pub fn fixes_dir() -> std::path::PathBuf {
    crate::data_dir().join("fixes")
}

type Attrs = Vec<(String, String)>;

fn attributes(e: &quick_xml::events::BytesStart) -> Result<Attrs, String> {
    let mut out = Vec::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|err| err.to_string())?;
        let key = attr.key.as_ref().to_string();
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|err| err.to_string())?
            .into_owned();
        out.push((key, value));
    }
    Ok(out)
}

fn attr(attrs: &Attrs, key: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.clone())
}

/// Summary counts for the UI.
pub fn summarize(issues: &[Issue]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for issue in issues {
        *counts.entry(issue.level.as_str()).or_insert(0) += 1;
    }
    counts
}

/// Build the probe for a bottle from the real machine state.
///
/// The checks are deliberately conservative: we only report a problem when we
/// can see it, never the other way round.
pub fn probe_for_bottle(prefix: &Path) -> Probe {
    let gstreamer = if cfg!(target_os = "macos") {
        Path::new("/Library/Frameworks/GStreamer.framework").exists()
            || Path::new("/opt/homebrew/bin/gst-launch-1.0").exists()
            || Path::new("/usr/local/bin/gst-launch-1.0").exists()
    } else {
        true
    };

    let rosetta = if cfg!(target_os = "macos") {
        Path::new("/Library/Apple/usr/libexec/oah").exists()
            || Path::new("/usr/libexec/oah").exists()
    } else {
        true
    };

    // 32-bit support: the engine must ship an i386-windows directory.
    let has_32bit = crate::bottle::wine_bin()
        .and_then(|wine| {
            wine.parent()
                .and_then(|bin| bin.parent())
                .map(|root| root.to_path_buf())
        })
        .map(|root| root.join("lib").join("wine").join("i386-windows").is_dir())
        .unwrap_or(false);

    // The fast renderer leaves a marker in the prefix.
    let vulkan_renderer = std::fs::read_to_string(prefix.join(".jisr-wined3d-renderer.ok"))
        .map(|value| value.trim() == "vulkan")
        .unwrap_or(false);

    // DXVK deployed into this bottle: the per-app layer marker exists.
    let dxvk_deployed = std::fs::read_dir(prefix)
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".jisr-layer-")
            })
        })
        .unwrap_or(false);

    let msync = crate::bottle_settings::read_setting(prefix, "Msync")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // A DirectX title is likely when the bottle has Steam games installed.
    let has_directx_title = prefix
        .join("drive_c")
        .join("Program Files (x86)")
        .join("Steam")
        .join("steamapps")
        .is_dir()
        || prefix
            .join("drive_c")
            .join("Program Files")
            .join("Steam")
            .join("steamapps")
            .is_dir();

    Probe {
        gstreamer,
        has_32bit,
        vulkan_renderer,
        dxvk_deployed,
        rosetta,
        msync,
        has_directx_title,
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Check a bottle for known problems (and how to fix them).
#[tauri::command]
pub fn fixes_check(bottle: String) -> Vec<Issue> {
    if !crate::bottle::valid_bottle_id(&bottle) {
        return Vec::new();
    }
    let prefix = crate::bottle::bottle_dir(&bottle);
    let db = FixesDb::load_dir(&fixes_dir());
    let db = if db.rules.is_empty() {
        default_db()
    } else {
        db
    };
    diagnose(&db, &probe_for_bottle(&prefix))
}

/// Turn msync on or off for a bottle.
#[tauri::command]
pub fn fixes_set_msync(bottle: String, enabled: bool) -> Result<bool, String> {
    if !crate::bottle::valid_bottle_id(&bottle) {
        return Err("معرّف القارورة غير صالح".to_string());
    }
    let prefix = crate::bottle::bottle_dir(&bottle);
    crate::bottle_settings::write_setting(&prefix, "Msync", if enabled { "1" } else { "0" })?;
    Ok(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<fixes>
  <error id="MissingGStreamer" level="error" check="gstreamer" title="GStreamer ناقص"
         desc="محرّك Wine يحتاج GStreamer للوسائط" fix="ثبّته من Homebrew"
         command="brew install --cask gstreamer"/>
  <error id="No32bit" level="warning" check="32bit" title="لا دعم 32-بت"
         desc="بعض الألعاب القديمة تحتاج 32-بت" fix="استخدم محرك فيه wow64"/>
  <error id="SlowRenderer" level="info" check="renderer" title="المحرّك على OpenGL"
         desc="المسار البطيء" fix="بدّل renderer إلى vulkan"/>
  <error id="NoDxvk" level="warning" check="dxvk" title="بدون DXVK"
         desc="لعبة DirectX بلا طبقة DXVK" fix="فعّل DXVK"/>
</fixes>"#;

    fn probe() -> Probe {
        Probe {
            gstreamer: false,
            has_32bit: false,
            vulkan_renderer: false,
            dxvk_deployed: false,
            rosetta: true,
            msync: true,
            has_directx_title: false,
        }
    }

    #[test]
    fn parses_the_shipped_fixes() {
        let db = default_db();
        assert!(!db.rules.is_empty(), "shipped fixes must not be empty");
        assert!(db.rules.iter().any(|r| r.id == "MissingGStreamer"));
    }

    #[test]
    fn detects_missing_pieces() {
        let db = FixesDb::parse(FIXES).expect("parse");
        let issues = diagnose(&db, &probe());
        let ids: Vec<&str> = issues.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"MissingGStreamer"));
        assert!(ids.contains(&"No32bit"));
        assert!(ids.contains(&"SlowRenderer"));
    }

    #[test]
    fn a_healthy_machine_has_no_errors() {
        let db = FixesDb::parse(FIXES).expect("parse");
        let good = Probe {
            gstreamer: true,
            has_32bit: true,
            vulkan_renderer: true,
            dxvk_deployed: true,
            rosetta: true,
            msync: true,
            has_directx_title: true,
        };
        let issues = diagnose(&db, &good);
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn dxvk_is_only_required_when_a_directx_title_exists() {
        let db = FixesDb::parse(FIXES).expect("parse");
        let mut p = probe();
        p.has_directx_title = false;
        assert!(!diagnose(&db, &p).iter().any(|i| i.id == "NoDxvk"));
        p.has_directx_title = true;
        assert!(diagnose(&db, &p).iter().any(|i| i.id == "NoDxvk"));
    }

    #[test]
    fn issues_are_ordered_most_severe_first() {
        let db = FixesDb::parse(FIXES).expect("parse");
        let issues = diagnose(&db, &probe());
        let levels: Vec<Level> = issues.iter().map(|i| i.level).collect();
        let mut sorted = levels.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(levels, sorted);
    }

    #[test]
    fn summary_counts_by_level() {
        let db = FixesDb::parse(FIXES).expect("parse");
        let counts = summarize(&diagnose(&db, &probe()));
        assert_eq!(counts.get("error"), Some(&1));
        assert_eq!(counts.get("warning"), Some(&1));
    }
}
