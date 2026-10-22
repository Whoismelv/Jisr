//! Compatibility profile database (`.c4p` / `.tie` XML).
//!
//! The app answers "does this game work?" and "what has to be installed or
//! patched" from a local profile database, with no network round-trip.
//!
//! The on-disk schema we read and write (our own `jisrdb` files use exactly
//! this shape, so profiles stay portable and hand-editable):
//!
//! ```text
//! <c4p>
//!   <highlightedapps><highlightedapp appid="..." platform="Mac"/></highlightedapps>
//!   <revokelist><revokeapp appid="..." first="..." last="..."/></revokelist>
//!   <disabledlicenses><id>...</id></disabledlicenses>
//!   <applications>
//!     <app appid="jisr.app.3" timestamp="2023-11-22T07:52:25-06:00">
//!       <cxversion product="jisr">10.2</cxversion>
//!       <name>Half-Life</name>
//!       <name lang="ar">...</name>
//!       <contributor userid="42">Someone</contributor>
//!       <appprofile>
//!         <description>...</description>
//!         <medal rating="5" platform="Mac" version="26.1.0" num="3"
//!                last="2026-04-28T08:35:44-05:00">gold</medal>
//!         <category>Games/First Person Shooter</category>
//!         <flag>application</flag>
//!         <installedkeypattern>Half-Life</installedkeypattern>
//!         <installeddisplaypattern>Half-Life</installeddisplaypattern>
//!         <bottletemplate>win2000</bottletemplate>
//!       </appprofile>
//!       <installprofile>
//!         <predependency>jisr.app.4099</predependency>
//!         <cxdiagcheck>AppRequire:MissingLibGL</cxdiagcheck>
//!         <postinstallreboot>true</postinstallreboot>
//!       </installprofile>
//!       <cdprofile><cdglob><glob>DATA.TAG</glob><pattern>...</pattern></cdglob></cdprofile>
//!     </app>
//!   </applications>
//! </c4p>
//! ```
//!
//! A few element names (`cxversion`, `cxdiagcheck`, `cxoffice`) are legacy
//! spellings baked into the file format. They are kept as *accepted aliases*
//! only — the parser never requires them and our own exports use the neutral
//! names. Unknown elements are ignored on purpose: the format grows, and a new
//! element must never break an older build.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// A name or description that may exist in several languages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedText {
    /// ISO language tag (`None` = the default/unspecified locale).
    pub lang: Option<String>,
    pub text: String,
}

/// One compatibility rating for a specific platform and product version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Medal {
    /// Star rating, 0..=5.
    pub rating: u8,
    /// `Mac`, `Linux`, `Android`, `ChromeOS`, ...
    pub platform: String,
    /// Product version the rating was recorded against (e.g. `26.1.0`).
    pub version: String,
    /// How many people reported this rating.
    pub num: i64,
    /// Timestamp of the last report.
    pub last: String,
    /// `gold`, `silver`, `bronze`, `platinum`, `ungold`, ...
    pub kind: String,
}

impl Medal {
    /// Rank of the medal kind. Higher is better. Unknown kinds rank lowest.
    pub fn kind_rank(&self) -> u8 {
        match self.kind.as_str() {
            "platinum" => 4,
            "gold" => 3,
            "silver" => 2,
            "bronze" => 1,
            _ => 0,
        }
    }

    /// Is this a "works well" rating rather than a downgrade marker.
    pub fn is_positive(&self) -> bool {
        self.kind_rank() >= 2 && self.rating >= 3
    }
}

/// A disc-detection rule: a file glob plus a content pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdGlob {
    pub glob: String,
    pub pattern: String,
}

/// Everything that has to happen *after* the installer ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallProfile {
    /// App ids that must be installed first.
    pub pre_dependencies: Vec<String>,
    /// Diagnostic ids the engine checks (`AppRequire:MissingLibGL`, `Closed`, ...).
    pub cxdiag_checks: Vec<String>,
    /// Whether the bottle must be rebooted after install.
    pub post_install_reboot: bool,
}

impl InstallProfile {
    pub fn is_empty(&self) -> bool {
        self.pre_dependencies.is_empty()
            && self.cxdiag_checks.is_empty()
            && !self.post_install_reboot
    }
}

/// One application entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppProfile {
    pub appid: String,
    pub timestamp: String,
    /// Product versions this profile applies to (`cxversion product="jisr"`).
    pub cx_versions: Vec<(String, String)>,
    pub names: Vec<LocalizedText>,
    pub descriptions: Vec<LocalizedText>,
    pub contributors: Vec<String>,
    pub categories: Vec<String>,
    /// `application`, `component`, `virtual`, ...
    pub flags: Vec<String>,
    /// Registry key pattern that proves the app is installed in a bottle.
    pub installed_key_pattern: Option<String>,
    /// Display-name pattern used when the registry key is not descriptive.
    pub installed_display_pattern: Option<String>,
    /// Bottle template the app wants (e.g. `win2000`).
    pub bottle_template: Option<String>,
    pub medals: Vec<Medal>,
    pub install: InstallProfile,
    pub cd_globs: Vec<CdGlob>,
}

impl AppProfile {
    /// The best name for a given language, falling back to the default one.
    pub fn name_for(&self, lang: &str) -> Option<&str> {
        self.names
            .iter()
            .find(|n| n.lang.as_deref() == Some(lang))
            .or_else(|| self.names.iter().find(|n| n.lang.is_none()))
            .map(|n| n.text.as_str())
    }

    /// The best medal for a platform: prefer kind, then rating, then reports.
    pub fn best_medal(&self, platform: &str) -> Option<&Medal> {
        self.medals
            .iter()
            .filter(|m| m.platform.eq_ignore_ascii_case(platform))
            .max_by_key(|m| (m.kind_rank(), m.rating, m.num))
    }
}

/// An application whose profile was revoked for a time window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokedApp {
    pub appid: String,
    pub first: String,
    pub last: String,
}

/// Counters for the UI ("we know 5308 apps, 181780 ratings").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DbStats {
    pub apps: usize,
    pub medals: usize,
    pub install_profiles: usize,
    pub highlighted: usize,
    pub revoked: usize,
    pub disabled_licenses: usize,
}

/// The whole database.
#[derive(Debug, Clone, Default)]
pub struct CompatDb {
    pub apps: BTreeMap<String, AppProfile>,
    pub highlighted: Vec<(String, String)>,
    pub revoked: Vec<RevokedApp>,
    pub disabled_licenses: Vec<String>,
}

impl CompatDb {
    pub fn stats(&self) -> DbStats {
        DbStats {
            apps: self.apps.len(),
            medals: self.apps.values().map(|a| a.medals.len()).sum(),
            install_profiles: self.apps.values().filter(|a| !a.install.is_empty()).count(),
            highlighted: self.highlighted.len(),
            revoked: self.revoked.len(),
            disabled_licenses: self.disabled_licenses.len(),
        }
    }

    /// Case-insensitive search over app ids, names and categories.
    pub fn search(&self, needle: &str) -> Vec<&AppProfile> {
        let q = needle.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<&AppProfile> = self
            .apps
            .values()
            .filter(|a| {
                a.appid.to_lowercase().contains(&q)
                    || a.names.iter().any(|n| n.text.to_lowercase().contains(&q))
                    || a.categories.iter().any(|c| c.to_lowercase().contains(&q))
            })
            .collect();
        hits.sort_by(|a, b| {
            let rank = |p: &AppProfile| {
                if p.appid.eq_ignore_ascii_case(&q) {
                    0
                } else if p.names.iter().any(|n| n.text.eq_ignore_ascii_case(&q)) {
                    1
                } else {
                    2
                }
            };
            rank(a).cmp(&rank(b)).then_with(|| a.appid.cmp(&b.appid))
        });
        hits
    }

    /// Best medal for an app on a platform.
    pub fn medal_for(&self, appid: &str, platform: &str) -> Option<&Medal> {
        self.apps.get(appid).and_then(|a| a.best_medal(platform))
    }

    /// Is an app revoked right now?
    pub fn is_revoked(&self, appid: &str, now: &str) -> bool {
        self.revoked
            .iter()
            .find(|r| r.appid == appid)
            .map(|r| r.first.as_str() <= now && now <= r.last.as_str())
            .unwrap_or(false)
    }
}

/// Parse failure with the byte position where it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub position: u64,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.position)
    }
}

impl std::error::Error for ParseError {}

/// Load a database from disk.
pub fn load(path: &Path) -> Result<CompatDb, ParseError> {
    let file = File::open(path).map_err(|e| ParseError {
        message: format!("cannot open {}: {e}", path.display()),
        position: 0,
    })?;
    parse(BufReader::new(file))
}

/// Parse a database from any buffered reader.
pub fn parse<R: BufRead>(reader: R) -> Result<CompatDb, ParseError> {
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut db = CompatDb::default();
    let mut app: Option<AppProfile> = None;
    let mut install: Option<InstallProfile> = None;
    let mut pending_medal: Option<Medal> = None;
    let mut cd_glob = String::new();
    let mut cd_pattern = String::new();
    // (element, lang attribute) currently collecting text
    let mut text_kind: Option<String> = None;
    let mut text_lang: Option<String> = None;
    let mut text_buf = String::new();

    let mut buf = Vec::new();
    loop {
        let ev = xml.read_event_into(&mut buf).map_err(|e| ParseError {
            message: e.to_string(),
            position: xml.error_position(),
        })?;
        match ev {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                let name = e.name().as_ref().to_string();
                let attrs = attributes(e)?;
                let empty = matches!(ev, Event::Empty(_));
                match name.as_str() {
                    "app" => {
                        flush_app(&mut db, &mut app);
                        let mut profile = AppProfile::default();
                        profile.appid = attr(&attrs, "appid").unwrap_or_default();
                        profile.timestamp = attr(&attrs, "timestamp").unwrap_or_default();
                        app = Some(profile);
                    }
                    "appprofile" => {}
                    "installprofile" => install = Some(InstallProfile::default()),
                    "cdglob" => {
                        cd_glob.clear();
                        cd_pattern.clear();
                    }
                    "highlightedapp" => {
                        if let (Some(a), Some(p)) =
                            (attr(&attrs, "appid"), attr(&attrs, "platform"))
                        {
                            db.highlighted.push((a, p));
                        }
                    }
                    "revokeapp" => db.revoked.push(RevokedApp {
                        appid: attr(&attrs, "appid").unwrap_or_default(),
                        first: attr(&attrs, "first").unwrap_or_default(),
                        last: attr(&attrs, "last").unwrap_or_default(),
                    }),
                    "medal" => {
                        pending_medal = Some(Medal {
                            rating: attr(&attrs, "rating")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0),
                            platform: attr(&attrs, "platform").unwrap_or_default(),
                            version: attr(&attrs, "version").unwrap_or_default(),
                            num: attr(&attrs, "num")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0),
                            last: attr(&attrs, "last").unwrap_or_default(),
                            kind: String::new(),
                        });
                        if !empty {
                            text_kind = Some("medal".into());
                            text_lang = None;
                            text_buf.clear();
                        }
                    }
                    "name" | "description" => {
                        text_kind = Some(name.clone());
                        text_lang = attr(&attrs, "lang");
                        text_buf.clear();
                        if empty {
                            push_text(&mut app, &mut install, &name, text_lang.take(), "");
                            text_kind = None;
                        }
                    }
                    "cxversion" => {
                        let product = attr(&attrs, "product").unwrap_or_default();
                        text_kind = Some("cxversion".into());
                        text_lang = Some(product);
                        text_buf.clear();
                        if empty {
                            push_text(&mut app, &mut install, "cxversion", text_lang.take(), "");
                            text_kind = None;
                        }
                    }
                    "contributor" => {
                        text_kind = Some("contributor".into());
                        text_lang = None;
                        text_buf.clear();
                        if empty {
                            push_text(&mut app, &mut install, "contributor", None, "");
                            text_kind = None;
                        }
                    }
                    "category"
                    | "flag"
                    | "installedkeypattern"
                    | "installeddisplaypattern"
                    | "bottletemplate"
                    | "predependency"
                    | "cxdiagcheck"
                    | "glob"
                    | "pattern"
                    | "id" => {
                        text_kind = Some(name.clone());
                        text_lang = None;
                        text_buf.clear();
                        if empty {
                            push_text(&mut app, &mut install, &name, None, "");
                            text_kind = None;
                        }
                    }
                    "postinstallreboot" => {
                        let value = attr(&attrs, "value").unwrap_or_default();
                        if value.is_empty() && !empty {
                            // <postinstallreboot>true</postinstallreboot>
                            text_kind = Some("postinstallreboot".into());
                            text_buf.clear();
                        } else {
                            let enabled = value.eq_ignore_ascii_case("true") || value == "1";
                            if let Some(install) = install.as_mut() {
                                install.post_install_reboot = enabled;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(ref t) => {
                if text_kind.is_some() {
                    text_buf.push_str(&t.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Event::End(ref e) => {
                let name = e.name().as_ref().to_string();
                match name.as_str() {
                    "name"
                    | "description"
                    | "cxversion"
                    | "contributor"
                    | "category"
                    | "flag"
                    | "installedkeypattern"
                    | "installeddisplaypattern"
                    | "bottletemplate"
                    | "predependency"
                    | "cxdiagcheck"
                    | "glob"
                    | "pattern"
                    | "postinstallreboot" => {
                        let kind = text_kind.take().unwrap_or_else(|| name.clone());
                        let lang = text_lang.take();
                        let value = std::mem::take(&mut text_buf);
                        push_text(&mut app, &mut install, &kind, lang, &value);
                        if kind == "glob" {
                            cd_glob = value;
                        } else if kind == "pattern" {
                            cd_pattern = value;
                        }
                    }
                    "id" => {
                        let value = std::mem::take(&mut text_buf);
                        text_kind = None;
                        text_lang = None;
                        let value = value.trim();
                        if !value.is_empty() {
                            db.disabled_licenses.push(value.to_string());
                        }
                    }
                    "medal" => {
                        let value = std::mem::take(&mut text_buf);
                        text_kind = None;
                        text_lang = None;
                        if let Some(mut medal) = pending_medal.take() {
                            medal.kind = value.trim().to_string();
                            push_medal(&mut app, medal);
                        }
                    }
                    "appprofile" => {}
                    "installprofile" => {
                        if let Some(profile) = install.take() {
                            if let Some(app) = app.as_mut() {
                                app.install
                                    .pre_dependencies
                                    .extend(profile.pre_dependencies);
                                app.install.cxdiag_checks.extend(profile.cxdiag_checks);
                                app.install.post_install_reboot |= profile.post_install_reboot;
                            }
                        }
                    }
                    "cdglob" => {
                        if let Some(profile) = app.as_mut() {
                            if !cd_glob.is_empty() {
                                profile.cd_globs.push(CdGlob {
                                    glob: std::mem::take(&mut cd_glob),
                                    pattern: std::mem::take(&mut cd_pattern),
                                });
                            }
                        }
                    }
                    "app" => flush_app(&mut db, &mut app),
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    flush_app(&mut db, &mut app);
    Ok(db)
}

fn flush_app(db: &mut CompatDb, app: &mut Option<AppProfile>) {
    if let Some(profile) = app.take() {
        if !profile.appid.is_empty() {
            db.apps.insert(profile.appid.clone(), profile);
        }
    }
}

fn push_medal(app: &mut Option<AppProfile>, medal: Medal) {
    if let Some(profile) = app.as_mut() {
        profile.medals.push(medal);
    }
}

fn push_text(
    app: &mut Option<AppProfile>,
    install: &mut Option<InstallProfile>,
    kind: &str,
    lang: Option<String>,
    value: &str,
) {
    let value = value.trim();
    match kind {
        "name" => {
            if let Some(profile) = app.as_mut() {
                profile.names.push(LocalizedText {
                    lang,
                    text: value.to_string(),
                });
            }
        }
        "description" => {
            if let Some(profile) = app.as_mut() {
                profile.descriptions.push(LocalizedText {
                    lang,
                    text: value.to_string(),
                });
            }
        }
        "cxversion" => {
            if let Some(profile) = app.as_mut() {
                profile
                    .cx_versions
                    .push((lang.unwrap_or_default(), value.to_string()));
            }
        }
        "contributor" => {
            if let Some(profile) = app.as_mut() {
                profile.contributors.push(value.to_string());
            }
        }
        "category" => {
            if let Some(profile) = app.as_mut() {
                profile.categories.push(value.to_string());
            }
        }
        "flag" => {
            if let Some(profile) = app.as_mut() {
                profile.flags.push(value.to_string());
            }
        }
        "installedkeypattern" => {
            if let Some(profile) = app.as_mut() {
                profile.installed_key_pattern = Some(value.to_string());
            }
        }
        "installeddisplaypattern" => {
            if let Some(profile) = app.as_mut() {
                profile.installed_display_pattern = Some(value.to_string());
            }
        }
        "bottletemplate" => {
            if let Some(profile) = app.as_mut() {
                profile.bottle_template = Some(value.to_string());
            }
        }
        "predependency" => {
            if let Some(install) = install.as_mut() {
                install.pre_dependencies.push(value.to_string());
            }
        }
        "cxdiagcheck" => {
            if let Some(install) = install.as_mut() {
                install.cxdiag_checks.push(value.to_string());
            }
        }
        "postinstallreboot" => {
            if let Some(install) = install.as_mut() {
                install.post_install_reboot = value.eq_ignore_ascii_case("true") || value == "1";
            }
        }
        _ => {}
    }
}

type Attrs = Vec<(String, String)>;

fn attributes(e: &quick_xml::events::BytesStart) -> Result<Attrs, ParseError> {
    let mut out = Vec::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|err| ParseError {
            message: err.to_string(),
            position: 0,
        })?;
        let key = attr.key.as_ref().to_string();
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|err| ParseError {
                message: err.to_string(),
                position: 0,
            })?
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

/// Serialise profiles back into the `.c4p` XML format.
///
/// This is how a user shares their profile database with somebody else — the
/// output is a plain text file that any c4p-capable tool can read.
pub fn to_c4p(profiles: &[AppProfile]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<c4p>\n  <applications>\n");
    for profile in profiles {
        out.push_str(&format!(
            "    <app appid=\"{}\" timestamp=\"{}\">\n",
            escape_attr(&profile.appid),
            escape_attr(&profile.timestamp)
        ));
        for (product, version) in &profile.cx_versions {
            out.push_str(&format!(
                "      <cxversion product=\"{}\">{}</cxversion>\n",
                escape_attr(product),
                escape_text(version)
            ));
        }
        for name in &profile.names {
            match &name.lang {
                Some(lang) => out.push_str(&format!(
                    "      <name lang=\"{}\">{}</name>\n",
                    escape_attr(lang),
                    escape_text(&name.text)
                )),
                None => out.push_str(&format!("      <name>{}</name>\n", escape_text(&name.text))),
            }
        }
        for category in &profile.categories {
            out.push_str(&format!(
                "      <category>{}</category>\n",
                escape_text(category)
            ));
        }
        for flag in &profile.flags {
            out.push_str(&format!("      <flag>{}</flag>\n", escape_text(flag)));
        }
        if let Some(template) = &profile.bottle_template {
            out.push_str(&format!(
                "      <bottletemplate>{}</bottletemplate>\n",
                escape_text(template)
            ));
        }
        for medal in &profile.medals {
            out.push_str(&format!(
                "      <medal rating=\"{}\" platform=\"{}\" version=\"{}\" num=\"{}\" last=\"{}\">{}</medal>\n",
                medal.rating,
                escape_attr(&medal.platform),
                escape_attr(&medal.version),
                medal.num,
                escape_attr(&medal.last),
                escape_text(&medal.kind)
            ));
        }
        if !profile.install.pre_dependencies.is_empty() {
            out.push_str("      <installprofile>\n");
            for dependency in &profile.install.pre_dependencies {
                out.push_str(&format!(
                    "        <predependency>{}</predependency>\n",
                    escape_text(dependency)
                ));
            }
            for check in &profile.install.cxdiag_checks {
                out.push_str(&format!(
                    "        <cxdiagcheck>{}</cxdiagcheck>\n",
                    escape_text(check)
                ));
            }
            if profile.install.post_install_reboot {
                out.push_str("        <postinstallreboot>true</postinstallreboot>\n");
            }
            out.push_str("      </installprofile>\n");
        }
        out.push_str("    </app>\n");
    }
    out.push_str("  </applications>\n</c4p>\n");
    out
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Reject a path that would escape the directory it is applied to.
///
/// Enforces the same invariant every profile recipe must hold: no path
/// component may match `..`, and the path must stay relative.
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    if path.contains(':') {
        return false;
    }
    !path
        .split(['/', '\\'])
        .any(|part| part == ".." || part == ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<c4p>
  <highlightedapps>
    <highlightedapp appid="jisr.app.3" platform="Mac"/>
    <highlightedapp appid="jisr.app.3" platform="Linux"/>
  </highlightedapps>
  <revokelist>
    <revokeapp appid="jisr.app.99" first="2018-01-01T00:00:00-06:00" last="2079-12-31T00:00:00-06:00"/>
  </revokelist>
  <disabledlicenses>
    <id>hV5x1EhTKhgSMUhk</id>
    <id>fCXW2trrXw2MaWmR</id>
  </disabledlicenses>
  <applications>
    <app appid="jisr.app.3" timestamp="2023-11-22T07:52:25-06:00">
      <cxversion product="jisr">10.2</cxversion>
      <name>Half-Life</name>
      <name lang="ar">هالف لايف</name>
      <contributor userid="6">Someone</contributor>
      <appprofile>
        <description>Gravity gun, quarantine.</description>
        <medal rating="5" platform="Mac" version="26.1.0" num="3" last="2026-04-28T08:35:44-05:00">gold</medal>
        <medal rating="5" platform="Mac" version="26.2.0" num="1" last="2026-06-11T06:29:23-05:00">ungold</medal>
        <medal rating="4" platform="Linux" version="26.2.0" num="9" last="2026-06-11T06:29:23-05:00">silver</medal>
        <category>Games/First Person Shooter</category>
        <flag>application</flag>
        <installedkeypattern>Half-Life</installedkeypattern>
        <bottletemplate>win2000</bottletemplate>
      </appprofile>
      <installprofile>
        <predependency>jisr.app.4099</predependency>
        <cxdiagcheck>AppRequire:MissingLibGL</cxdiagcheck>
        <postinstallreboot>true</postinstallreboot>
      </installprofile>
      <cdprofile>
        <cdglob>
          <glob>DATA.TAG</glob>
          <pattern>Application=Half-Life</pattern>
        </cdglob>
      </cdprofile>
    </app>
    <app appid="jisr.app.7" timestamp="2013-07-30T12:48:07-05:00">
      <name>Word 2000</name>
      <appprofile>
        <medal rating="5" platform="Linux" version="9.0.0" num="0" last="2013-03-27T12:24:47-05:00">gold</medal>
      </appprofile>
    </app>
  </applications>
</c4p>"#;

    fn db() -> CompatDb {
        parse(FIXTURE.as_bytes()).expect("fixture must parse")
    }

    #[test]
    fn parses_apps_and_counters() {
        let db = db();
        let stats = db.stats();
        assert_eq!(stats.apps, 2);
        assert_eq!(stats.medals, 4);
        assert_eq!(stats.install_profiles, 1);
        assert_eq!(stats.highlighted, 2);
        assert_eq!(stats.revoked, 1);
        assert_eq!(stats.disabled_licenses, 2);
    }

    #[test]
    fn reads_app_fields() {
        let db = db();
        let app = db.apps.get("jisr.app.3").expect("app present");
        assert_eq!(app.timestamp, "2023-11-22T07:52:25-06:00");
        assert_eq!(
            app.cx_versions,
            vec![("jisr".to_string(), "10.2".to_string())]
        );
        assert_eq!(app.name_for("ar"), Some("هالف لايف"));
        assert_eq!(app.name_for("de"), Some("Half-Life"));
        assert_eq!(
            app.categories,
            vec!["Games/First Person Shooter".to_string()]
        );
        assert_eq!(app.flags, vec!["application".to_string()]);
        assert_eq!(app.installed_key_pattern.as_deref(), Some("Half-Life"));
        assert_eq!(app.bottle_template.as_deref(), Some("win2000"));
        assert_eq!(app.contributors, vec!["Someone".to_string()]);
    }

    #[test]
    fn reads_install_profile() {
        let db = db();
        let app = db.apps.get("jisr.app.3").unwrap();
        assert_eq!(
            app.install.pre_dependencies,
            vec!["jisr.app.4099".to_string()]
        );
        assert_eq!(
            app.install.cxdiag_checks,
            vec!["AppRequire:MissingLibGL".to_string()]
        );
        assert!(app.install.post_install_reboot);
    }

    #[test]
    fn reads_cd_glob_rules() {
        let db = db();
        let app = db.apps.get("jisr.app.3").unwrap();
        assert_eq!(app.cd_globs.len(), 1);
        assert_eq!(app.cd_globs[0].glob, "DATA.TAG");
        assert_eq!(app.cd_globs[0].pattern, "Application=Half-Life");
    }

    #[test]
    fn medal_ranking_prefers_gold_over_ungold() {
        let db = db();
        let app = db.apps.get("jisr.app.3").unwrap();
        let best = app.best_medal("Mac").expect("a mac medal");
        assert_eq!(best.kind, "gold");
        assert!(best.is_positive());
        // "ungold" ranks below "gold" even with the same rating.
        let ungold = app
            .medals
            .iter()
            .find(|m| m.kind == "ungold")
            .expect("ungold medal");
        assert!(!ungold.is_positive());
        assert!(ungold.kind_rank() < best.kind_rank());
    }

    #[test]
    fn medal_lookup_is_platform_scoped() {
        let db = db();
        assert_eq!(
            db.medal_for("jisr.app.3", "Linux").map(|m| m.kind.as_str()),
            Some("silver")
        );
        assert!(db.medal_for("jisr.app.7", "Mac").is_none());
    }

    #[test]
    fn search_matches_id_name_and_category() {
        let db = db();
        assert_eq!(db.search("half-life").len(), 1);
        assert_eq!(db.search("jisr.app.7").len(), 1);
        assert_eq!(db.search("shooter").len(), 1);
        assert!(db.search("nothing-here").is_empty());
        assert!(db.search("  ").is_empty());
    }

    #[test]
    fn revoked_apps_are_time_windowed() {
        let db = db();
        assert!(db.is_revoked("jisr.app.99", "2020-01-01"));
        assert!(!db.is_revoked("jisr.app.99", "2017-01-01"));
        assert!(!db.is_revoked("jisr.app.3", "2020-01-01"));
    }

    #[test]
    fn unknown_elements_do_not_break_parsing() {
        let xml = r#"<c4p><applications><app appid="x"><futurething foo="bar">
            <nested><deep>value</deep></nested></futurething>
            <name>Still Parsed</name></app></applications></c4p>"#;
        let db = parse(xml.as_bytes()).expect("must parse");
        let app = db.apps.get("x").unwrap();
        assert_eq!(app.name_for("en"), Some("Still Parsed"));
    }

    #[test]
    fn exported_profiles_round_trip() {
        let db = db();
        let profiles: Vec<AppProfile> = db.apps.values().cloned().collect();
        let xml = to_c4p(&profiles);
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("<c4p>"));
        assert!(xml.contains("jisr.app.3"));
        assert!(xml.contains("<medal rating=\"5\""));
        // The exported file must parse back.
        let reparsed = parse(xml.as_bytes()).expect("exported file must parse");
        assert_eq!(reparsed.apps.len(), db.apps.len());
        assert_eq!(
            reparsed.apps.get("jisr.app.3").map(|a| a.medals.len()),
            Some(3)
        );
    }

    #[test]
    fn exported_xml_escapes_markup() {
        let profile = AppProfile {
            appid: "com.example.a&b".to_string(),
            names: vec![LocalizedText {
                lang: None,
                text: "A < B & C".to_string(),
            }],
            ..Default::default()
        };
        let xml = to_c4p(&[profile]);
        assert!(xml.contains("com.example.a&amp;b"));
        assert!(xml.contains("A &lt; B &amp; C"));
    }

    #[test]
    fn rejects_paths_that_escape_the_target_directory() {
        assert!(is_safe_relative_path("Program Files/App/file.dll"));
        assert!(!is_safe_relative_path("../evil.dll"));
        assert!(!is_safe_relative_path("dir/../../evil.dll"));
        assert!(!is_safe_relative_path("/etc/passwd"));
        assert!(!is_safe_relative_path("C:\\Windows\\system32"));
        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("./relative"));
    }

    /// اختياري: تشغيل على قاعدة توافق حقيقية (23MB / آلاف التطبيقات).
    /// PowerShell: `$env:JISR_C4P_FILE="C:\...\legacy.tie"; cargo test real_c4p -- --ignored`
    #[test]
    #[ignore]
    fn real_c4p_database_parses() {
        let Ok(path) = std::env::var("JISR_C4P_FILE") else {
            return;
        };
        let path = Path::new(&path);
        let db = load(path).expect("real database must parse");
        let stats = db.stats();
        assert!(
            stats.apps > 1_000,
            "expected a populated database: {stats:?}"
        );
        assert!(stats.medals > 1_000, "expected medals: {stats:?}");
        assert!(!db.disabled_licenses.is_empty());
        // A known profile must be searchable by name.
        assert!(!db.search("Half-Life").is_empty());
    }
}
