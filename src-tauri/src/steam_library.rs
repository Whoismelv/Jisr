// ============================================================
//  جسر — مكتبة ستيم المثبّتة (قراءة محلية فقط)
//  • نقرأ steamapps/libraryfolders.vdf + appmanifest_*.acf من قارورة ستيم
//  • ما ننفّذ أي مسار من الملفات: التشغيل عبر Steam نفسه بأمر -applaunch
//  • كل المسارات تُفحص (لا ".."، ولا خروج عن C:) — قراءة فقط بلا كتابة
//  • VDF محلّل صغير ومحصّن: نستخرج أزواج المفاتيح/القيم فقط
// ============================================================
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, State};

use crate::{bottle, graphics, log, log_keys, steam};

/// سقوف أمان: ملفات VDF المحلية صغيرة بطبيعتها
const MAX_VDF_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LIBRARIES: usize = 64;
const MAX_MANIFESTS: usize = 4_000;

#[derive(Serialize, Clone)]
pub struct SteamGame {
    pub appid: u32,
    pub name: String,
    pub size_bytes: u64,
}

// ============================================================
//  محلّل VDF مبسّط (KeyValues) — يكفي لمكتبة ستيم
// ============================================================
#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Vec<Tok> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                let mut s = String::new();
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' if i + 1 < bytes.len() => {
                            // فك الهروب الشائع في VDF (المسار غالباً \\)
                            // ملاحظة: أي هروب غير معروف لازم يتقدّم بايت واحد فقط،
                            // وإلا ممكن نقف داخل حرف UTF-8 (اسم عربي) ونسبب panic.
                            match bytes[i + 1] {
                                b'\\' => {
                                    s.push('\\');
                                    i += 2;
                                }
                                b'"' => {
                                    s.push('"');
                                    i += 2;
                                }
                                b'n' => {
                                    s.push('\n');
                                    i += 2;
                                }
                                b't' => {
                                    s.push('\t');
                                    i += 2;
                                }
                                b if b.is_ascii() => {
                                    s.push(b as char);
                                    i += 2;
                                }
                                _ => {
                                    s.push('\\');
                                    i += 1;
                                }
                            }
                        }
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => {
                            // نحافظ على Unicode (أسماء الألعاب العربية وغيرها)
                            if let Some(ch) = text[i..].chars().next() {
                                s.push(ch);
                                i += ch.len_utf8();
                            } else {
                                break;
                            }
                        }
                    }
                }
                out.push(Tok::Str(s));
            }
            b'{' => {
                out.push(Tok::Open);
                i += 1;
            }
            b'}' => {
                out.push(Tok::Close);
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// كل أزواج "مفتاح" "قيمة" في النص (يتجاهل الأقواس والبنية المتداخلة)
fn vdf_pairs(text: &str) -> Vec<(String, String)> {
    let toks = tokenize(text);
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < toks.len() {
        if let (Tok::Str(k), Tok::Str(v)) = (&toks[i], &toks[i + 1]) {
            out.push((k.clone(), v.clone()));
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn pair<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

// ============================================================
//  المسارات — ويندوز داخل القارورة → مسار ماك
// ============================================================

/// C:\... → <prefix>/drive_c/...   |   D:\... → <prefix>/dosdevices/d:/...
/// نرفض أي ".." وأي خروج فعلي (بعد canonicalize) عن drive_c.
fn win_to_host(prefix: &Path, win: &str) -> Option<PathBuf> {
    let win = win.trim();
    let bytes = win.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return None;
    }
    let drive = (bytes[0] as char).to_ascii_lowercase();
    let rest = &win[2..];
    if !(rest.starts_with('\\') || rest.starts_with('/')) {
        return None;
    }

    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split(['\\', '/']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return None;
    }

    let base = if drive == 'c' {
        prefix.join("drive_c")
    } else {
        prefix.join("dosdevices").join(format!("{drive}:"))
    };
    let full = parts.iter().fold(base, |acc, p| acc.join(p));

    if drive == 'c' {
        // الحارس الحقيقي: بعد فك الروابط، لازم يبقى داخل drive_c
        let root = std::fs::canonicalize(prefix.join("drive_c")).ok()?;
        let real = std::fs::canonicalize(&full).ok()?;
        if !real.starts_with(&root) {
            return None;
        }
        Some(real)
    } else {
        // أقراص خارجية/مكتبات إضافية: نقرأ منها فقط (بلا كتابة أو تنفيذ)
        std::fs::canonicalize(&full).ok()
    }
}

fn read_limited(path: &Path) -> Option<String> {
    let md = std::fs::metadata(path).ok()?;
    if !md.is_file() || md.len() > MAX_VDF_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// اسم معروض آمن: بلا أسطر جديدة/تحكّم، وبحد طول معقول
fn clean_name(raw: &str) -> Option<String> {
    let name: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > 200 {
        return None;
    }
    Some(name)
}

/// installdir لازم يكون مجلداً واحداً بسيطاً (ما نسمح بمسار داخل المانيفست).
/// نرفض الفواصل يدوياً بدل الاعتماد على منصة التشغيل فقط: ملفات ستيم (ACF/VDF)
/// بصيغة ويندوز، والشرطة المائلة العكسية فاصل فيها حتى لو البرنامج شغال على ماك.
/// بدون هذا الفحص كان "..\\evil" يمرّ على ماك ويُرفض على ويندوز (فشل CI).
fn safe_component(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "." || raw == ".." {
        return None;
    }
    if raw.contains('/') || raw.contains('\\') {
        return None;
    }
    let mut comps = Path::new(raw).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(c)), None) if c.to_str() == Some(raw) => Some(raw),
        _ => None,
    }
}

fn parse_manifest(text: &str) -> Option<(u32, String, u64, Option<u64>)> {
    let pairs = vdf_pairs(text);
    let appid: u32 = pair(&pairs, "appid")?.trim().parse().ok()?;
    if appid == 0 {
        return None;
    }
    let name = clean_name(pair(&pairs, "name")?)?;
    let size = pair(&pairs, "SizeOnDisk")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let flags = pair(&pairs, "StateFlags").and_then(|v| v.trim().parse::<u64>().ok());
    Some((appid, name, size, flags))
}

fn scan() -> Vec<SteamGame> {
    let Some((mac_exe, _)) = steam::steam_exe() else {
        return Vec::new();
    };
    let Some(steam_dir) = mac_exe.parent() else {
        return Vec::new();
    };
    let prefix = bottle::bottle_dir(bottle::MAIN_ID);
    let root_apps = steam_dir.join("steamapps");

    // جذور المكتبات: الأساسية + أي مسار في libraryfolders.vdf
    let mut libs: Vec<PathBuf> = Vec::new();
    {
        let mut seen: HashMap<PathBuf, ()> = HashMap::new();
        let mut add_lib = |host: PathBuf| {
            let apps = host.join("steamapps");
            if !apps.is_dir() {
                return;
            }
            let Ok(real) = std::fs::canonicalize(&apps) else {
                return;
            };
            if libs.len() < MAX_LIBRARIES && !seen.contains_key(&real) {
                seen.insert(real.clone(), ());
                libs.push(real);
            }
        };

        add_lib(steam_dir.to_path_buf());

        if let Some(text) = read_limited(&root_apps.join("libraryfolders.vdf")) {
            for (k, v) in vdf_pairs(&text) {
                if k.eq_ignore_ascii_case("path")
                    && let Some(host) = win_to_host(&prefix, &v)
                {
                    add_lib(host);
                }
            }
        }
    }

    let mut games: HashMap<u32, SteamGame> = HashMap::new();
    let mut scanned = 0usize;

    for apps in &libs {
        let Ok(rd) = std::fs::read_dir(apps) else {
            continue;
        };
        for e in rd.flatten() {
            if scanned >= MAX_MANIFESTS {
                break;
            }
            let fname = e.file_name().to_string_lossy().to_ascii_lowercase();
            if !fname.starts_with("appmanifest_") || !fname.ends_with(".acf") {
                continue;
            }
            scanned += 1;

            let Some(text) = read_limited(&e.path()) else {
                continue;
            };
            let Some((appid, name, size, flags)) = parse_manifest(&text) else {
                continue;
            };
            // 4 = Fully Installed (نتجاهل أي لعبة غير مكتملة)
            if flags.unwrap_or(4) & 4 == 0 {
                continue;
            }

            // تأكيد فعلي: مجلد اللعبة موجود (المانيفست وحده لا يكفي)
            let installed = vdf_pairs(&text)
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("installdir"))
                .and_then(|(_, v)| safe_component(v))
                .map(|dir| apps.join("common").join(dir).is_dir())
                .unwrap_or(false);
            if !installed {
                continue;
            }

            games.entry(appid).or_insert(SteamGame {
                appid,
                name,
                size_bytes: size,
            });
        }
    }

    let mut list: Vec<SteamGame> = games.into_values().collect();
    list.sort_by_key(|g| g.name.to_lowercase());
    list
}

// ============================================================
//  أوامر Tauri
// ============================================================

/// كل ألعاب ستيم المثبّتة داخل القارورة الرئيسية
#[tauri::command]
pub async fn steam_games_list() -> Result<Vec<SteamGame>, String> {
    if cfg!(windows) {
        return Ok(Vec::new());
    }
    tauri::async_runtime::spawn_blocking(scan)
        .await
        .map_err(|e| format!("تعذّر قراءة مكتبة ستيم: {e}"))
}

/// تشغيل لعبة عبر Steam (Steam لازم يكون شغّالاً — أمر قصير ثم يخرج)
#[tauri::command]
pub async fn steam_game_launch(
    app: AppHandle,
    state: State<'_, steam::SteamState>,
    appid: u32,
) -> Result<(), String> {
    let _trace = crate::debuglog::trace("steam", "steam_game_launch", format!("appid={appid}"));
    if cfg!(windows) {
        return Err("تشغيل ألعاب ستيم يحتاج Wine — هذي بيئة تطوير ويندوز".into());
    }
    if appid == 0 {
        return Err("رقم اللعبة غير صالح".into());
    }
    if !steam::is_running(&state) {
        return Err("شغّل ستيم أولاً، ثم اضغط تشغيل اللعبة".into());
    }
    if !bottle::bottle_ready(bottle::MAIN_ID) {
        return Err("جهّز القارورة الرئيسية أول".into());
    }
    let Some((mac_exe, dos_exe)) = steam::steam_exe() else {
        return Err("ستيم مو مثبّت بعد".into());
    };
    let Some(wine) = bottle::wine_bin() else {
        return Err("ما لقينا محرك Wine — ثبّت المحرك أول".into());
    };

    let prefix = bottle::bottle_dir(bottle::MAIN_ID);
    bottle::sync_ca_bundle(&prefix);
    if let Some(dir) = mac_exe.parent() {
        steam::prepare_steam_ui(&app, dir);
    }

    // بلا shell: appid رقم فقط، وsteam.exe يوجّه الأمر للعميل الشغّال
    let mut cmd = tokio::process::Command::new(&wine);
    cmd.env("WINEPREFIX", &prefix);
    bottle::apply_wine_env(&mut cmd, true);
    graphics::apply_env(&mut cmd, true);
    cmd.arg(&dos_exe)
        .arg("-applaunch")
        .arg(appid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(dir) = mac_exe.parent() {
        cmd.current_dir(dir);
    }

    let mut child = cmd.spawn().map_err(|e| {
        let msg = format!("تعذّر إرسال أمر التشغيل لستيم: {e}");
        log(
            &app,
            "error",
            log_keys::STEAM_GAME_LAUNCH_FAILED,
            json!({ "appid": appid, "error": msg }),
        );
        msg
    })?;

    log(
        &app,
        "info",
        log_keys::STEAM_GAME_LAUNCHING,
        json!({ "appid": appid }),
    );

    // ننتظر الأمر القصير بالخلفية — خطأ الخروج فقط يُسجَّل (بلا إزعاج)
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        match child.wait().await {
            Ok(st) if !st.success() => log(
                &app2,
                "warn",
                log_keys::STEAM_GAME_LAUNCH_FAILED,
                json!({ "appid": appid, "error": format!("رمز {}", st.code().unwrap_or(-1)) }),
            ),
            Ok(_) => {}
            Err(e) => log(
                &app2,
                "warn",
                log_keys::STEAM_GAME_LAUNCH_FAILED,
                json!({ "appid": appid, "error": e.to_string() }),
            ),
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vdf_pairs_and_escaped_paths() {
        let text = r#"
            "libraryfolders"
            {
                "0"
                {
                    "path" "C:\\Program Files (x86)\\Steam"
                    "label" "Main"
                }
                "1"
                {
                    "path" "D:\\Games\\SteamLibrary"
                }
            }
        "#;
        let pairs = vdf_pairs(text);
        assert_eq!(pair(&pairs, "path"), Some(r"C:\Program Files (x86)\Steam"));
        assert!(pairs.iter().any(|(_, v)| v == r"D:\Games\SteamLibrary"));
    }

    #[test]
    fn malformed_escape_keeps_unicode_intact() {
        // هروب غير معروف قبل حرف عربي: لازم ما يسبب panic ولا يقص الحرف
        let pairs = vdf_pairs("\"name\" \"أ\\لعبة\"");
        assert_eq!(pair(&pairs, "name"), Some("أ\\لعبة"));
    }

    #[test]
    fn manifest_requires_installed_game() {
        let text = r#"
            "AppState"
            {
                "appid" "480"
                "name" "Spacewar"
                "installdir" "Spacewar"
                "SizeOnDisk" "12345"
                "StateFlags" "4"
            }
        "#;
        let (appid, name, size, flags) = parse_manifest(text).unwrap();
        assert_eq!(appid, 480);
        assert_eq!(name, "Spacewar");
        assert_eq!(size, 12345);
        assert_eq!(flags, Some(4));
        assert_eq!(safe_component("Spacewar"), Some("Spacewar"));
        assert_eq!(safe_component("..\\evil"), None);
        assert_eq!(safe_component("a/b"), None);
        assert_eq!(safe_component(".."), None);
        assert_eq!(safe_component("."), None);
        assert_eq!(safe_component("a\\b"), None);
    }
}
