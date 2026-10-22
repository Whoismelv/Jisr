//! Compatibility-profile store.
//!
//! A directory of profile files that describe how a Windows application
//! behaves under Wine, what has to be installed with it, and how well it runs.
//!
//! * The file format is the `.c4p` XML schema (see [`crate::c4p`]) so profiles
//!   stay readable by other tools and easy to hand-edit.
//! * Profiles are ours: we ship an empty store and let the community (or the
//!   user) drop profiles in. Importing a foreign file is a read-only, local
//!   operation.
//! * Lookups are served from one merged database: every `.c4p` in the store
//!   is merged into a single view.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::c4p::{self, AppProfile, CompatDb, Medal};

/// The profiles we ship with the app (our own content, in the same schema).
const SEED_PROFILES: &str = include_str!("../resources/compat/jisr-seed.c4p");

/// Where our profile files live.
pub fn store_dir() -> PathBuf {
    crate::data_dir().join("compat")
}

/// A profile as the UI wants it.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileSummary {
    pub appid: String,
    /// Best name for the requested language, falling back to the default one.
    pub name: String,
    pub category: String,
    /// `gold` / `silver` / `bronze` / … when a rating exists.
    pub medal: Option<String>,
    /// Star rating 0..=5.
    pub rating: Option<u8>,
    /// Bottle template the profile asks for.
    pub bottle_template: Option<String>,
    /// Applications that must be installed first.
    pub dependencies: Vec<String>,
    /// Does the profile declare itself as an application (not a component)?
    pub is_application: bool,
}

/// Counters for the UI.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct StoreStats {
    pub profiles: usize,
    pub files: usize,
    pub ratings: usize,
}

/// The merged view of every profile file in the store.
pub struct ProfileStore {
    root: PathBuf,
    db: CompatDb,
    files: usize,
}

impl ProfileStore {
    /// Build a store over a directory (defaults to our app-support folder).
    pub fn open(root: Option<PathBuf>) -> ProfileStore {
        let root = root.unwrap_or_else(store_dir);
        let mut store = ProfileStore {
            root,
            db: CompatDb::default(),
            files: 0,
        };
        store.seed_if_empty();
        store.reload();
        store
    }

    /// Write the shipped profiles on first run.
    ///
    /// The user can delete or replace them afterwards — we only seed a store
    /// that has no profile files at all.
    pub fn seed_if_empty(&self) {
        let has_files = std::fs::read_dir(&self.root)
            .map(|entries| {
                entries.flatten().any(|entry| {
                    entry
                        .path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("c4p") || e.eq_ignore_ascii_case("tie"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if has_files {
            return;
        }
        if std::fs::create_dir_all(&self.root).is_err() {
            return;
        }
        let _ = std::fs::write(self.root.join("jisr-seed.c4p"), SEED_PROFILES);
    }

    /// Re-read every profile file from disk.
    ///
    /// Files are merged in name order; a later file overrides an earlier one
    /// for the same application id, so a newer profile wins over an older one.
    pub fn reload(&mut self) {
        self.db = CompatDb::default();
        self.files = 0;
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("c4p") || e.eq_ignore_ascii_case("tie"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for path in paths {
            let Ok(parsed) = c4p::load(&path) else {
                continue;
            };
            self.files += 1;
            for (appid, profile) in parsed.apps {
                self.db.apps.insert(appid, profile);
            }
            self.db.highlighted.extend(parsed.highlighted);
            self.db.revoked.extend(parsed.revoked);
            self.db.disabled_licenses.extend(parsed.disabled_licenses);
        }
    }

    /// The merged database.
    pub fn db(&self) -> &CompatDb {
        &self.db
    }

    /// Search profiles by id, name or category.
    pub fn search(&self, query: &str, lang: &str, limit: usize) -> Vec<ProfileSummary> {
        self.db
            .search(query)
            .into_iter()
            .take(limit)
            .map(|profile| summarize(profile, lang))
            .collect()
    }

    /// One profile by id.
    pub fn get(&self, appid: &str, lang: &str) -> Option<ProfileSummary> {
        self.db.apps.get(appid).map(|p| summarize(p, lang))
    }

    /// Best rating for an application on a platform.
    pub fn medal_for(&self, appid: &str, platform: &str) -> Option<&Medal> {
        self.db.medal_for(appid, platform)
    }

    /// Store counters.
    pub fn stats(&self) -> StoreStats {
        let stats = self.db.stats();
        StoreStats {
            profiles: stats.apps,
            files: self.files,
            ratings: stats.medals,
        }
    }

    /// Copy a foreign profile file into the store.
    ///
    /// The file is validated first: a file that does not parse is rejected and
    /// nothing is written.
    pub fn import(&mut self, source: &Path) -> Result<StoreStats, String> {
        let parsed = c4p::load(source).map_err(|e| e.to_string())?;
        if parsed.apps.is_empty() {
            return Err("الملف لا يحتوي أي بروفايل".to_string());
        }
        std::fs::create_dir_all(&self.root).map_err(|e| format!("تعذّر إنشاء المجلد: {e}"))?;
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported");
        let target = self.root.join(format!("{stem}.c4p"));
        std::fs::copy(source, &target).map_err(|e| format!("تعذّر النسخ: {e}"))?;
        self.reload();
        Ok(self.stats())
    }

    /// Map of appid -> profile, for callers that need the raw data.
    pub fn profiles(&self) -> &BTreeMap<String, AppProfile> {
        &self.db.apps
    }
}

fn summarize(profile: &AppProfile, lang: &str) -> ProfileSummary {
    let medal = profile.best_medal("Mac");
    ProfileSummary {
        appid: profile.appid.clone(),
        name: profile.name_for(lang).unwrap_or_default().to_string(),
        category: profile.categories.first().cloned().unwrap_or_default(),
        medal: medal.map(|m| m.kind.clone()),
        rating: medal.map(|m| m.rating),
        bottle_template: profile.bottle_template.clone(),
        dependencies: profile.install.pre_dependencies.clone(),
        is_application: profile.flags.iter().any(|f| f == "application"),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Export the merged profile database as a `.c4p` file, so it can be shared.
#[tauri::command]
pub fn compat_export(path: String) -> Result<String, String> {
    let target = expand_tilde(&path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("تعذّر إنشاء المجلد: {e}"))?;
    }
    let store = ProfileStore::open(None);
    let profiles: Vec<AppProfile> = store.profiles().values().cloned().collect();
    let xml = c4p::to_c4p(&profiles);
    std::fs::write(&target, xml).map_err(|e| format!("تعذّر الكتابة: {e}"))?;
    Ok(target.to_string_lossy().to_string())
}

/// Expand a leading `~` into the user's home directory.
///
/// The UI hands us paths like `~/Documents/jisr-profiles.c4p`; the filesystem
/// layer does not expand `~` on its own.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return PathBuf::from(home).join(rest);
            }
        }
    }
    PathBuf::from(path)
}

#[tauri::command]
pub fn compat_stats() -> StoreStats {
    ProfileStore::open(None).stats()
}

#[tauri::command]
pub fn compat_search(
    query: String,
    lang: Option<String>,
    limit: Option<usize>,
) -> Vec<ProfileSummary> {
    let lang = lang.unwrap_or_else(|| "ar".to_string());
    let limit = limit.unwrap_or(50).clamp(1, 500);
    ProfileStore::open(None).search(&query, &lang, limit)
}

#[tauri::command]
pub fn compat_get(appid: String, lang: Option<String>) -> Option<ProfileSummary> {
    let lang = lang.unwrap_or_else(|| "ar".to_string());
    ProfileStore::open(None).get(&appid, &lang)
}

#[tauri::command]
pub fn compat_import(path: String) -> Result<StoreStats, String> {
    let mut store = ProfileStore::open(None);
    store.import(Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<c4p><applications>
  <app appid="com.example.game" timestamp="2026-01-01T00:00:00-06:00">
    <name>Example Game</name>
    <name lang="ar">لعبة مثال</name>
    <appprofile>
      <medal rating="5" platform="Mac" version="26.1.0" num="7" last="2026-04-28T08:35:44-05:00">gold</medal>
      <category>Games/Action</category>
      <flag>application</flag>
      <bottletemplate>win10_64</bottletemplate>
    </appprofile>
    <installprofile>
      <predependency>com.example.runtime</predependency>
    </installprofile>
  </app>
</applications></c4p>"#;

    fn temp_store(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("jisr-compat-{name}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn empty_store_has_no_profiles() {
        let dir = temp_store("empty");
        // The store seeds itself on first run, so the "empty" case is about a
        // store that already has files (or a store we never seed).
        let mut store = ProfileStore::open(Some(dir.clone()));
        std::fs::remove_file(dir.join("jisr-seed.c4p")).unwrap();
        store.reload();
        assert_eq!(store.stats().profiles, 0);
        assert!(store.search("anything", "ar", 10).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shipped_profiles_are_seeded_on_first_run() {
        let dir = temp_store("seed");
        let store = ProfileStore::open(Some(dir.clone()));
        assert!(dir.join("jisr-seed.c4p").exists(), "seed must be written");
        assert!(
            store.stats().profiles >= 4,
            "seed profiles: {:?}",
            store.stats()
        );
        // The shipped profiles are our own content, in Arabic too.
        let hits = store.search("ستيم", "ar", 5);
        assert!(!hits.is_empty(), "Arabic name must be searchable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_profiles_from_the_store() {
        let dir = temp_store("load");
        std::fs::write(dir.join("games.c4p"), PROFILE).unwrap();
        let store = ProfileStore::open(Some(dir.clone()));
        let stats = store.stats();
        assert_eq!(stats.profiles, 1);
        assert_eq!(stats.files, 1);
        assert_eq!(stats.ratings, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_returns_localized_names_and_medals() {
        let dir = temp_store("search");
        std::fs::write(dir.join("games.c4p"), PROFILE).unwrap();
        let store = ProfileStore::open(Some(dir.clone()));
        let hits = store.search("مثال", "ar", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "لعبة مثال");
        assert_eq!(hits[0].medal.as_deref(), Some("gold"));
        assert_eq!(hits[0].rating, Some(5));
        assert_eq!(hits[0].bottle_template.as_deref(), Some("win10_64"));
        assert!(hits[0].is_application);
        assert_eq!(
            hits[0].dependencies,
            vec!["com.example.runtime".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn later_files_override_earlier_ones() {
        let dir = temp_store("override");
        std::fs::write(dir.join("a-first.c4p"), PROFILE).unwrap();
        let other = PROFILE.replace("gold", "bronze");
        std::fs::write(dir.join("b-second.c4p"), other).unwrap();
        let store = ProfileStore::open(Some(dir.clone()));
        assert_eq!(store.stats().profiles, 1);
        let hits = store.search("example", "en", 10);
        assert_eq!(hits[0].medal.as_deref(), Some("bronze"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_validates_before_writing() {
        let dir = temp_store("import");
        let mut store = ProfileStore::open(Some(dir.clone()));
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let bad = outside.join("broken.c4p");
        std::fs::write(&bad, "<c4p><applications><app").unwrap();
        assert!(store.import(&bad).is_err());
        assert!(!dir.join("broken.c4p").exists());

        let good = outside.join("good.c4p");
        std::fs::write(&good, PROFILE).unwrap();
        let before = store.stats().profiles;
        let stats = store.import(&good).expect("import must succeed");
        assert_eq!(
            stats.profiles,
            before + 1,
            "the imported profile must be added"
        );
        assert!(store.profiles().contains_key("com.example.game"));
        assert!(dir.join("good.c4p").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_writes_a_readable_c4p_file() {
        let dir = temp_store("export");
        let _store = ProfileStore::open(Some(dir.clone()));
        let target = dir.join("out.c4p");
        let written = compat_export(target.to_string_lossy().to_string()).expect("export");
        assert!(Path::new(&written).exists());
        let text = std::fs::read_to_string(&target).unwrap();
        assert!(text.contains("<c4p>"));
        assert!(text.contains("com.jisr.steam"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tilde_is_expanded_to_the_home_directory() {
        if std::env::var("HOME").is_err() {
            return;
        }
        let expanded = expand_tilde("~/Documents/x.c4p");
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        assert_eq!(expanded, home.join("Documents").join("x.c4p"));
        assert_eq!(expand_tilde("/tmp/x.c4p"), PathBuf::from("/tmp/x.c4p"));
    }

    #[test]
    fn import_rejects_files_without_profiles() {
        let dir = temp_store("import-empty");
        let mut store = ProfileStore::open(Some(dir.clone()));
        let empty = dir.join("empty.c4p");
        std::fs::write(&empty, "<c4p><applications></applications></c4p>").unwrap();
        assert!(store.import(&empty).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
