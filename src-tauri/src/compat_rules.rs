//! Per-application compatibility rules.
//!
//! A rule database that adjusts the runtime **per application** instead of per
//! bottle: it lives next to the app, is plain XML, and is applied from the
//! launcher before Wine starts.
//!
//! Rules are matched on the application (Steam id, executable path) and may
//! carry hacks:
//!
//! * `graphics_backend` — pick the D3D stack for this title
//! * `env_vars` — set or derive environment variables
//! * `append_args` — command line additions/removals
//! * `dll_overrides` — per-DLL native/builtin overrides
//!
//! The **first matching rule wins** for each hack type.

use std::collections::BTreeMap;
use std::path::Path;

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::graphics_backend::GraphicsBackend;

/// What a rule matches on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPred {
    /// A Steam application id (the number in the Steam store URL).
    SteamGameId(u32),
    /// The executable path ends with this suffix.
    PathSuffix(String),
    /// Product / version / platform triple.
    Product {
        product: String,
        cxversion: Option<String>,
        platform: Option<String>,
    },
}

/// A single adjustment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hack {
    /// Force a graphics backend for this application.
    GraphicsBackend {
        backend: GraphicsBackend,
        force: bool,
    },
    /// Set environment variables; `by_setting` derives a value from another one.
    EnvVars {
        set: BTreeMap<String, String>,
        by_setting: BTreeMap<String, String>,
    },
    /// Command line surgery.
    AppendArgs {
        append: Vec<String>,
        insert: Vec<String>,
        forbidden: Vec<String>,
        required: Vec<String>,
    },
    /// Per-DLL overrides (`native` / `builtin` / `disabled`).
    DllOverrides {
        dlls: BTreeMap<String, String>,
        load_order: Vec<String>,
    },
}

impl Hack {
    /// Stable kind name (used in reports and in the UI).
    pub fn kind(&self) -> &'static str {
        match self {
            Hack::GraphicsBackend { .. } => "graphics_backend",
            Hack::EnvVars { .. } => "env_vars",
            Hack::AppendArgs { .. } => "append_args",
            Hack::DllOverrides { .. } => "dll_overrides",
        }
    }
}

/// One rule: a matcher plus its hacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub matcher: MatchPred,
    pub hacks: Vec<Hack>,
}

/// The context a rule is evaluated against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// Steam app id, when the game comes from Steam.
    pub steam_game_id: Option<u32>,
    /// Path of the executable that will be launched.
    pub exe_path: Option<String>,
    /// Product identifier (`cxmac` for us).
    pub product: Option<String>,
    /// Product version.
    pub cxversion: Option<String>,
    /// Platform (`Mac`).
    pub platform: Option<String>,
}

impl Context {
    fn matches(&self, matcher: &MatchPred) -> bool {
        match matcher {
            MatchPred::SteamGameId(id) => self.steam_game_id == Some(*id),
            MatchPred::PathSuffix(suffix) => self
                .exe_path
                .as_deref()
                .map(|p| {
                    let p = p.replace('\\', "/").to_lowercase();
                    let suffix = suffix.replace('\\', "/").to_lowercase();
                    p.ends_with(&suffix)
                })
                .unwrap_or(false),
            MatchPred::Product {
                product,
                cxversion,
                platform,
            } => {
                if self.product.as_deref() != Some(product.as_str()) {
                    return false;
                }
                if let Some(want) = cxversion {
                    match self.cxversion.as_deref() {
                        Some(have) if have == want => {}
                        _ => return false,
                    }
                }
                if let Some(want) = platform {
                    if self.platform.as_deref() != Some(want.as_str()) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

/// The outcome of evaluating the rules for one context.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolution {
    /// Graphics backend forced by a rule (if any).
    pub backend: Option<GraphicsBackend>,
    /// Environment variables to set.
    pub env: BTreeMap<String, String>,
    /// DLL overrides to write into the bottle registry.
    pub dll_overrides: BTreeMap<String, String>,
    /// Extra command line arguments.
    pub append_args: Vec<String>,
    /// Arguments that must not be present.
    pub forbidden_args: Vec<String>,
    /// Arguments that must be present.
    pub required_args: Vec<String>,
    /// DLL load order hints.
    pub load_order: Vec<String>,
    /// Which rule produced the backend (for reports).
    pub matched_rule: Option<usize>,
}

/// The rule database.
#[derive(Debug, Clone, Default)]
pub struct RulesDb {
    pub rules: Vec<Rule>,
}

impl RulesDb {
    /// Parse a rule file.
    pub fn parse(xml: &str) -> Result<RulesDb, String> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut db = RulesDb::default();
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
                        "rule" => {
                            current = Some(Rule {
                                matcher: MatchPred::PathSuffix(String::new()),
                                hacks: Vec::new(),
                            });
                        }
                        "match" => {
                            let pred = parse_match(&attrs)?;
                            if let Some(rule) = current.as_mut() {
                                rule.matcher = pred;
                            }
                        }
                        "hack" => {
                            let hack = parse_hack(&attrs)?;
                            if let Some(rule) = current.as_mut() {
                                rule.hacks.push(hack);
                            }
                        }
                        _ => {}
                    }
                    let _ = empty;
                }
                Event::End(ref e) => {
                    let name = e.name().as_ref().to_string();
                    if name == "rule" {
                        if let Some(rule) = current.take() {
                            db.rules.push(rule);
                        }
                    }
                }
                _ => {}
            }
            buf.clear();
        }

        // Rules that were only opened (never closed) still count.
        if let Some(rule) = current.take() {
            db.rules.push(rule);
        }
        // A rule without a matcher is meaningless — refuse the whole file so a
        // broken edit cannot silently disable a fix.
        if db
            .rules
            .iter()
            .any(|rule| matches!(&rule.matcher, MatchPred::PathSuffix(suffix) if suffix.is_empty()))
        {
            return Err("قاعدة بلا شرط مطابقة".to_string());
        }
        Ok(db)
    }

    /// Load every `*.jdb` file in a directory (sorted, later files win).
    pub fn load_dir(dir: &Path) -> Result<RulesDb, String> {
        let mut merged = RulesDb::default();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(merged);
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("jdb"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("تعذّر قراءة {}: {e}", path.display()))?;
            let parsed = RulesDb::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            merged.rules.extend(parsed.rules);
        }
        Ok(merged)
    }

    /// Evaluate the rules for a context. First match per hack type wins.
    pub fn resolve(&self, context: &Context) -> Resolution {
        let mut resolution = Resolution::default();
        let mut seen_backend = false;
        let mut seen_env = false;
        let mut seen_args = false;
        let mut seen_dlls = false;

        for (index, rule) in self.rules.iter().enumerate() {
            if !context.matches(&rule.matcher) {
                continue;
            }
            for hack in &rule.hacks {
                match hack {
                    Hack::GraphicsBackend { backend, force } => {
                        if !seen_backend || *force {
                            resolution.backend = Some(*backend);
                            resolution.matched_rule = Some(index);
                            seen_backend = true;
                        }
                    }
                    Hack::EnvVars { set, by_setting } => {
                        if !seen_env {
                            resolution.env.extend(set.clone());
                            resolution.env.extend(by_setting.clone());
                            seen_env = true;
                        }
                    }
                    Hack::AppendArgs {
                        append,
                        insert,
                        forbidden,
                        required,
                    } => {
                        if !seen_args {
                            resolution.append_args.extend(append.clone());
                            resolution.append_args.extend(insert.clone());
                            resolution.forbidden_args.extend(forbidden.clone());
                            resolution.required_args.extend(required.clone());
                            seen_args = true;
                        }
                    }
                    Hack::DllOverrides { dlls, load_order } => {
                        if !seen_dlls {
                            resolution.dll_overrides.extend(dlls.clone());
                            resolution.load_order.extend(load_order.clone());
                            seen_dlls = true;
                        }
                    }
                }
            }
        }
        resolution
    }

    /// Number of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

fn parse_match(attrs: &Attrs) -> Result<MatchPred, String> {
    if let Some(raw) = attr(attrs, "steam_game_id") {
        let id: u32 = raw
            .trim()
            .parse()
            .map_err(|_| format!("steam_game_id غير صالح: {raw}"))?;
        return Ok(MatchPred::SteamGameId(id));
    }
    if let Some(raw) = attr(attrs, "path_suffix") {
        return Ok(MatchPred::PathSuffix(raw));
    }
    if let Some(product) = attr(attrs, "product") {
        return Ok(MatchPred::Product {
            product,
            cxversion: attr(attrs, "cxversion"),
            platform: attr(attrs, "platform"),
        });
    }
    Err("قاعدة بلا شرط مطابقة".to_string())
}

fn parse_hack(attrs: &Attrs) -> Result<Hack, String> {
    let kind = attr(attrs, "type").unwrap_or_default();
    match kind.as_str() {
        "graphics_backend" => {
            let backend_raw = attr(attrs, "backend").unwrap_or_default();
            let backend = GraphicsBackend::parse(&backend_raw)
                .ok_or_else(|| format!("محرك رسومي غير معروف: {backend_raw}"))?;
            let force = attr(attrs, "force")
                .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                .unwrap_or(false);
            Ok(Hack::GraphicsBackend { backend, force })
        }
        "env_vars" => {
            let mut set = BTreeMap::new();
            let mut by_setting = BTreeMap::new();
            for (key, value) in attrs {
                match key.as_str() {
                    "type" => {}
                    "set" => {
                        if let Some((k, v)) = value.split_once('=') {
                            set.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                    "by_setting" => {
                        if let Some((k, v)) = value.split_once('=') {
                            by_setting.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                    _ => {}
                }
            }
            Ok(Hack::EnvVars { set, by_setting })
        }
        "append_args" => Ok(Hack::AppendArgs {
            append: split_list(attr(attrs, "append").as_deref()),
            insert: split_list(attr(attrs, "insert").as_deref()),
            forbidden: split_list(attr(attrs, "forbidden").as_deref()),
            required: split_list(attr(attrs, "required").as_deref()),
        }),
        "dll_overrides" => {
            let mut dlls = BTreeMap::new();
            for (key, value) in attrs {
                if key == "dll" {
                    if let Some((dll, val)) = value.split_once('=') {
                        dlls.insert(dll.trim().to_string(), val.trim().to_string());
                    }
                }
            }
            Ok(Hack::DllOverrides {
                dlls,
                load_order: split_list(attr(attrs, "load_order").as_deref()),
            })
        }
        other => Err(format!("نوع hack غير معروف: {other}")),
    }
}

fn split_list(raw: Option<&str>) -> Vec<String> {
    raw.map(|value| {
        value
            .split(|c| c == ',' || c == ';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
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

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// A rule as the UI shows it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleInfo {
    pub index: usize,
    pub matcher: String,
    pub hacks: Vec<String>,
}

/// Where our rule files live.
pub fn rules_dir() -> std::path::PathBuf {
    crate::data_dir().join("rules")
}

/// List every rule we know about.
#[tauri::command]
pub fn rules_list() -> Vec<RuleInfo> {
    let Ok(db) = RulesDb::load_dir(&rules_dir()) else {
        return Vec::new();
    };
    db.rules
        .iter()
        .enumerate()
        .map(|(index, rule)| RuleInfo {
            index,
            matcher: match &rule.matcher {
                MatchPred::SteamGameId(id) => format!("steam_game_id={id}"),
                MatchPred::PathSuffix(suffix) => format!("path_suffix={suffix}"),
                MatchPred::Product {
                    product,
                    cxversion,
                    platform,
                } => {
                    let mut parts = vec![format!("product={product}")];
                    if let Some(v) = cxversion {
                        parts.push(format!("cxversion={v}"));
                    }
                    if let Some(p) = platform {
                        parts.push(format!("platform={p}"));
                    }
                    parts.join(" ")
                }
            },
            hacks: rule.hacks.iter().map(|h| h.kind().to_string()).collect(),
        })
        .collect()
}

/// Save a rule file (validated before it is written).
#[tauri::command]
pub fn rules_save(name: String, xml: String) -> Result<(), String> {
    let stem = name
        .trim()
        .trim_end_matches(".jdb")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();
    if stem.is_empty() {
        return Err("اسم ملف القاعدة غير صالح".to_string());
    }
    RulesDb::parse(&xml).map_err(|e| format!("القاعدة غير صالحة: {e}"))?;
    let dir = rules_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("تعذّر إنشاء المجلد: {e}"))?;
    std::fs::write(dir.join(format!("{stem}.jdb")), xml)
        .map_err(|e| format!("تعذّر حفظ القاعدة: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<jdb>
  <rules>
    <rule>
      <match steam_game_id="220"/>
      <hack type="graphics_backend" backend="dxmt" force="true"/>
      <hack type="env_vars" set="DXVK_ASYNC=1" by_setting="GAME=hl2"/>
      <hack type="dll_overrides" dll="d3d9=native" load_order="d3d11,d3d9"/>
    </rule>
    <rule>
      <match path_suffix="\game.exe"/>
      <hack type="append_args" append="-windowed" forbidden="-fullscreen" required="-dx11"/>
    </rule>
    <rule>
      <match product="cxmac" platform="Mac"/>
      <hack type="graphics_backend" backend="dxvk"/>
    </rule>
  </rules>
</jdb>"#;

    #[test]
    fn parses_all_rule_kinds() {
        let db = RulesDb::parse(RULES).expect("parse");
        assert_eq!(db.len(), 3);
        assert_eq!(db.rules[0].matcher, MatchPred::SteamGameId(220));
        assert_eq!(
            db.rules[1].matcher,
            MatchPred::PathSuffix("\\game.exe".into())
        );
        assert!(matches!(
            db.rules[2].matcher,
            MatchPred::Product { ref platform, .. } if platform.as_deref() == Some("Mac")
        ));
    }

    #[test]
    fn steam_rules_win_for_the_matching_game() {
        let db = RulesDb::parse(RULES).expect("parse");
        let context = Context {
            steam_game_id: Some(220),
            ..Default::default()
        };
        let resolved = db.resolve(&context);
        assert_eq!(resolved.backend, Some(GraphicsBackend::Dxmt));
        assert_eq!(
            resolved.env.get("DXVK_ASYNC").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            resolved.dll_overrides.get("d3d9").map(String::as_str),
            Some("native")
        );
        assert_eq!(
            resolved.load_order,
            vec!["d3d11".to_string(), "d3d9".to_string()]
        );
    }

    #[test]
    fn non_matching_games_fall_through_to_the_product_rule() {
        let db = RulesDb::parse(RULES).expect("parse");
        let context = Context {
            steam_game_id: Some(570),
            product: Some("cxmac".into()),
            platform: Some("Mac".into()),
            ..Default::default()
        };
        let resolved = db.resolve(&context);
        assert_eq!(resolved.backend, Some(GraphicsBackend::Dxvk));
    }

    #[test]
    fn path_suffix_matching_is_case_insensitive() {
        let db = RulesDb::parse(RULES).expect("parse");
        let context = Context {
            exe_path: Some("C:\\Games\\Thing\\GAME.EXE".into()),
            ..Default::default()
        };
        let resolved = db.resolve(&context);
        assert_eq!(resolved.append_args, vec!["-windowed".to_string()]);
        assert_eq!(resolved.forbidden_args, vec!["-fullscreen".to_string()]);
        assert_eq!(resolved.required_args, vec!["-dx11".to_string()]);
    }

    #[test]
    fn first_matching_rule_wins_for_each_hack_type() {
        let xml = r#"<jdb><rules>
          <rule><match product="cxmac"/><hack type="graphics_backend" backend="dxmt"/></rule>
          <rule><match product="cxmac"/><hack type="graphics_backend" backend="dxvk"/></rule>
        </rules></jdb>"#;
        let db = RulesDb::parse(xml).expect("parse");
        let context = Context {
            product: Some("cxmac".into()),
            ..Default::default()
        };
        assert_eq!(db.resolve(&context).backend, Some(GraphicsBackend::Dxmt));
    }

    #[test]
    fn force_lets_a_later_rule_override_the_backend() {
        let xml = r#"<jdb><rules>
          <rule><match product="cxmac"/><hack type="graphics_backend" backend="dxmt"/></rule>
          <rule><match steam_game_id="7"/><hack type="graphics_backend" backend="dxvk" force="true"/></rule>
        </rules></jdb>"#;
        let db = RulesDb::parse(xml).expect("parse");
        let context = Context {
            product: Some("cxmac".into()),
            steam_game_id: Some(7),
            ..Default::default()
        };
        assert_eq!(db.resolve(&context).backend, Some(GraphicsBackend::Dxvk));
    }

    #[test]
    fn unknown_backends_are_rejected() {
        let xml = r#"<jdb><rules><rule><match product="x"/>
          <hack type="graphics_backend" backend="wat"/></rule></rules></jdb>"#;
        assert!(RulesDb::parse(xml).is_err());
    }

    #[test]
    fn rules_without_a_matcher_are_rejected() {
        let xml = r#"<jdb><rules><rule><hack type="append_args" append="-x"/>
          </rule></rules></jdb>"#;
        assert!(RulesDb::parse(xml).is_err());
    }

    #[test]
    fn empty_database_resolves_to_nothing() {
        let db = RulesDb::default();
        let resolved = db.resolve(&Context::default());
        assert!(resolved.backend.is_none());
        assert!(resolved.env.is_empty());
        assert!(resolved.dll_overrides.is_empty());
    }
}
