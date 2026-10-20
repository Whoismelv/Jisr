// ============================================================
//  جسر — مكتبة ستيم المثبّتة (قراءة محلية فقط)
//  • نقرأ steamapps/libraryfolders.vdf + appmanifest_*.acf من قارورة ستيم
//  • ما ننفّذ أي مسار من الملفات: التشغيل عبر Steam نفسه بأمر -applaunch
//  • كل المسارات تُفحص (لا ".."، ولا خروج عن C:) — قراءة فقط بلا كتابة
//  • VDF محلّل صغير ومحصّن: نستخرج أزواج المفاتيح/القيم فقط
// ============================================================
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{bottle, graphics, log, log_keys, pe, steam};

/// سقوف أمان: ملفات VDF المحلية صغيرة بطبيعتها
const MAX_VDF_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LIBRARIES: usize = 64;
const MAX_MANIFESTS: usize = 4_000;

#[derive(Serialize, Clone)]
pub struct SteamGame {
    pub appid: u32,
    pub name: String,
    pub size_bytes: u64,
    /// فحص اللعبة الحقيقي (من ملفها التنفيذي) — قبل ما يجرّب المستخدم ويضيع وقته
    pub probe: GameProbe,
    /// خطة التشغيل التلقائي: أي طبقة تناسب هذه اللعبة وأي موانع فيها
    pub plan: GamePlan,
}

/// خطة «تشغيل تلقائي» لكل لعبة — يختارها المخطّط من حقائق الـPE
#[derive(Serialize, Clone, Default)]
pub struct GamePlan {
    /// الطبقات المطلوبة (معرّفات تقنية)
    pub layers: Vec<String>,
    /// أسماء الطبقات بالعربي (للعرض)
    pub layer_labels: Vec<String>,
    /// موانع تمنع التشغيل (مضاد غش، DX12، …)
    pub blockers: Vec<String>,
    /// ملاحظات (بالعربي) للمستخدم
    pub notes: Vec<String>,
    /// إجراءات مطلوبة قبل التشغيل (تثبيت طبقة، تجاوز ريجستري…)
    pub actions: Vec<String>,
    /// نقدر نشغّلها؟
    pub launchable: bool,
    /// التركيبة الفعلية المستخدمة (من ملف التعريف)
    pub profile_ar: String,
    /// كم محاولة تلقائية جرت على هذه اللعبة
    pub profile_attempts: u32,
}

/// حالة الطبقات المتاحة على هذا الجهاز (لحساب الخطة)
fn layer_availability() -> crate::auto::LayerAvailability {
    let i386_vulkan = bottle::wine_bin()
        .map(|w| engine_has_i386_vulkan(&w))
        .unwrap_or(false);
    crate::auto::LayerAvailability {
        dxvk: graphics::is_installed(),
        dxvk_d3d9: graphics::d3d9_available(),
        d3dx9: crate::d3dx::is_installed(),
        // DXMT/vkd3d غير مدمجين بعد (الخطة توضّح للمستخدم المسار المتاح)
        dxmt: false,
        vkd3d: false,
        engine_has_i386_vulkan: i386_vulkan,
    }
}

/// خطة اللعبة: الطبقات + الموانع + الخطوات (بلا رقم لعبة — للاختبارات)
#[allow(dead_code)]
pub fn plan_for(probe: &GameProbe) -> GamePlan {
    plan_for_appid(probe, 0)
}

/// خطة اللعبة + التركيبة الفعلية (ملف التعريف) — نعطيها رقم اللعبة
pub fn plan_for_appid(probe: &GameProbe, appid: u32) -> GamePlan {
    let facts = crate::auto::GameFacts {
        probe: probe.clone(),
        needs_d3dx9: probe.needs_d3dx9,
        needs_d3dcompiler: probe.needs_d3dcompiler,
    };
    let plan = crate::auto::plan_with_facts(&facts, &layer_availability(), &graphics::mode());
    let profile = profile_for(appid);
    GamePlan {
        layers: plan.layers.iter().map(|l| l.as_str().to_string()).collect(),
        layer_labels: plan
            .layers
            .iter()
            .map(|l| l.label_ar().to_string())
            .collect(),
        blockers: plan
            .blockers
            .iter()
            .map(|b| b.label_ar().to_string())
            .collect(),
        notes: plan.notes.clone(),
        actions: plan.actions.iter().map(|a| a.kind().to_string()).collect(),
        launchable: plan.is_launchable(),
        profile_ar: profile.describe_ar(),
        profile_attempts: profile.attempts,
    }
}

/// نتيجة فحص اللعبة: DirectX المطلوب + المعمارية + موانع معروفة
#[derive(Serialize, Clone, Default, PartialEq, Debug)]
pub struct GameProbe {
    /// مسار الملف التنفيذي الرئيسي داخل مجلد اللعبة
    pub exe: Option<String>,
    /// `d3d9` · `d3d11` · `d3d12` · `vulkan` · `opengl` · `none`
    pub dx: Option<String>,
    /// 64-بت؟
    pub is_64: Option<bool>,
    /// رموز موانع: `dx8` · `dx12` · `anticheat` · `x86` · `dotnet`
    pub issues: Vec<String>,
    /// اللعبة تستدعي d3dx9_*.dll ⇒ تحتاج D3DX9 الأصلي (نسخة Wine تنهار)
    pub needs_d3dx9: bool,
    /// اللعبة تستدعي d3dcompiler_*.dll ⇒ مترجم HLSL (نسخة Wine تنهار)
    pub needs_d3dcompiler: bool,
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

/// جذور مكتبات ستيم (مجلدات steamapps): الأساسية + أي مسار في libraryfolders.vdf
fn library_roots() -> Vec<PathBuf> {
    let Some((mac_exe, _)) = steam::steam_exe() else {
        return Vec::new();
    };
    let Some(steam_dir) = mac_exe.parent() else {
        return Vec::new();
    };
    let prefix = bottle::bottle_dir(bottle::MAIN_ID);
    let root_apps = steam_dir.join("steamapps");

    let mut libs: Vec<PathBuf> = Vec::new();
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
    libs
}

/// مجلد تثبيت لعبة (appid) داخل القارورة — None لو ما لقيناها
fn install_dir(appid: u32) -> Option<PathBuf> {
    for apps in library_roots() {
        let acf = apps.join(format!("appmanifest_{appid}.acf"));
        let Some(text) = read_limited(&acf) else {
            continue;
        };
        let pairs = vdf_pairs(&text);
        let Some(dir) = pair(&pairs, "installdir").and_then(safe_component) else {
            continue;
        };
        let full = apps.join("common").join(dir);
        if full.is_dir() {
            return Some(full);
        }
    }
    None
}

/// يبحث عن ملف بالاسم داخل مجلد (غير حساس لحالة الأحرف)
fn find_file_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_ascii_lowercase();
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_file()
            && p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase() == want)
                .unwrap_or(false)
        {
            return Some(p);
        }
    }
    None
}

/// يستبدل قيمة `windowedmode` داخل settings.txt.
/// يرجع None لو الملف ما فيه هذا المفتاح (لعبة ما تدعمه) — ما نكتب شي عندها.
/// نحافظ على نمط نهاية السطر عشان محرّك اللعبة يقرأه بنفس الطريقة.
fn patch_windowed(text: &str, enabled: bool) -> Option<String> {
    let want = if enabled { "true" } else { "false" };
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut found = false;
    let mut out = String::with_capacity(text.len() + 8);

    for raw in text.split_inclusive('\n') {
        let had_nl = raw.ends_with('\n');
        let body = raw.trim_end_matches('\n').trim_end_matches('\r');
        let lower = body.trim_start().to_ascii_lowercase();
        let is_key = lower
            .strip_prefix("windowedmode")
            .map(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
            .unwrap_or(false);
        if is_key && body.contains('=') {
            found = true;
            let eq = body.find('=').unwrap_or(0);
            out.push_str(body[..eq].trim_end());
            out.push_str(" = ");
            out.push_str(want);
            out.push_str(eol);
            continue;
        }
        out.push_str(body);
        if had_nl {
            out.push_str(eol);
        }
    }

    if !found {
        return None;
    }
    Some(out)
}

/// نتيجة أمر «تشغيل في نافذة»
#[derive(Serialize, Clone)]
pub struct WindowedResult {
    pub applied: bool,
    pub file: Option<String>,
}

/// يفعّل (أو يعطّل) وضع النافذة للعبة عبر ملف settings.txt الخاص بها.
/// السبب: ألعاب قديقة (DirectX 9) تاخذ الشاشة بالكامل في الوضع الكامل، وداخل
/// Wine على الماك هذا يعني شاشة سودا. وضع النافذة يمنعها تماماً.
/// ما نكتب إلا داخل مجلد اللعبة، ومع نسخة احتياطية (.jisr.bak) قبل أول تعديل.
#[tauri::command]
pub async fn steam_game_windowed(
    app: AppHandle,
    appid: u32,
    enabled: bool,
) -> Result<WindowedResult, String> {
    let _t = crate::debuglog::trace(
        "steam",
        "steam_game_windowed",
        format!("appid={appid} enabled={enabled}"),
    );
    if appid == 0 {
        return Err("رقم اللعبة غير صالح".into());
    }
    let dir = install_dir(appid)
        .ok_or_else(|| "ما لقيت مجلد اللعبة — تأكد إنها منزّلة بالكامل".to_string())?;
    let file = patch_settings_windowed(&dir, enabled)?;

    log(
        &app,
        "info",
        log_keys::STEAM_GAME_WINDOWED,
        json!({ "appid": appid, "enabled": enabled, "file": file.display().to_string() }),
    );

    Ok(WindowedResult {
        applied: true,
        file: Some(file.display().to_string()),
    })
}

/// يفعّل/يعطّل وضع النافذة في `settings.txt` داخل مجلد اللعبة (مع نسخة احتياطية).
/// مستخدم من الأمر ومن **ملف التعريف** (لعبة تُشغّل دائماً في نافذة).
fn patch_settings_windowed(dir: &Path, enabled: bool) -> Result<PathBuf, String> {
    let file = find_file_ci(dir, "settings.txt").ok_or_else(|| {
        "هذي اللعبة ما فيها ملف settings.txt — فعّل وضع النافذة من إعدادات اللعبة نفسها".to_string()
    })?;

    let md = std::fs::metadata(&file).map_err(|e| format!("تعذّر قراءة إعدادات اللعبة: {e}"))?;
    if md.len() > 512 * 1024 {
        return Err("ملف إعدادات اللعبة كبير بشكل غير متوقع — ما نلمسه".into());
    }
    let bytes = std::fs::read(&file).map_err(|e| format!("تعذّر قراءة إعدادات اللعبة: {e}"))?;
    // بعض الألعاب القديمة تحفظ إعداداتها بـ UTF-16 (مع BOM) — نحترم ترميزها
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]);
    let text = if utf16 {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };
    let Some(patched) = patch_windowed(&text, enabled) else {
        return Err(
            "هذي اللعبة ما تدعم windowedmode في settings.txt — جرّب من قائمة إعدادات اللعبة".into(),
        );
    };

    // نسخة احتياطية مرة واحدة (لا نلمس الأصل)
    let backup = file.with_extension("txt.jisr.bak");
    if !backup.is_file() {
        let _ = std::fs::copy(&file, &backup);
    }
    let out: Vec<u8> = if utf16 {
        let mut v: Vec<u8> = vec![0xFF, 0xFE];
        for unit in patched.encode_utf16() {
            v.extend_from_slice(&unit.to_le_bytes());
        }
        v
    } else {
        patched.into_bytes()
    };
    std::fs::write(&file, out).map_err(|e| format!("تعذّر حفظ إعدادات اللعبة: {e}"))?;
    Ok(file)
}

/// مجلد لعبة مثبّتة (من المانيفست + تأكيد وجوده)
fn game_dir(appid: u32) -> Option<PathBuf> {
    for apps in library_roots() {
        let manifest = apps.join(format!("appmanifest_{appid}.acf"));
        let Some(text) = read_limited(&manifest) else {
            continue;
        };
        let pairs = vdf_pairs(&text);
        let Some(dir) = pairs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("installdir"))
            .and_then(|(_, v)| safe_component(v))
        else {
            continue;
        };
        let full = apps.join("common").join(dir);
        if full.is_dir() {
            return Some(full);
        }
    }
    None
}

/// هل الملف التنفيذي للمحرك فيه دعم Vulkan للـ32-بت؟
/// بدونه: DXVK 32-بت ما يقدر يحمّل `vulkan-1.dll` ⇒ Wine يرجع لـWineD3D
/// (ترجمة GLSL) ⇒ نفس خطأ الـpixel shader.
fn engine_has_i386_vulkan(wine: &Path) -> bool {
    let Some(root) = wine.parent().and_then(Path::parent) else {
        return false;
    };
    ["lib/wine", "lib64/wine", "lib/wine64"].iter().any(|dir| {
        root.join(dir)
            .join("i386-windows")
            .join("vulkan-1.dll")
            .is_file()
    })
}

/// اسم اللعبة الرسمي من المانيفست (للرسائل)
fn game_name(appid: u32) -> Option<String> {
    for apps in library_roots() {
        let manifest = apps.join(format!("appmanifest_{appid}.acf"));
        if let Some(text) = read_limited(&manifest) {
            if let Some((_, name, _, _)) = parse_manifest(&text) {
                return Some(name);
            }
        }
    }
    None
}

/// ملفات مساعدة ما نحسبها «اللعبة» (مثبّتات/أدوات كراش)
const SKIP_EXE: [&str; 12] = [
    "unins",
    "vcredist",
    "dxsetup",
    "dxwebsetup",
    "setup",
    "install",
    "crashhandler",
    "crashreport",
    "errorreporter",
    "unitycrashhandler",
    "steamerrorreporter",
    "report",
];

const MAX_SCAN_ENTRIES: usize = 4_000;

/// أكبر ملف .exe في مجلد اللعبة — عادةً هو الملف الرئيسي
fn find_main_exe(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((current, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in rd.flatten() {
            seen += 1;
            if seen > MAX_SCAN_ENTRIES {
                return best.map(|(_, p)| p);
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if depth < 3 && !name.starts_with('.') {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            if !name.ends_with(".exe") || SKIP_EXE.iter().any(|s| name.contains(s)) {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if best.as_ref().map(|(b, _)| size > *b).unwrap_or(true) {
                best = Some((size, entry.path()));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// هل اللعبة تستخدم مضاد غش يحتاج درايفر ويندوز (مستحيل على الماك)؟
fn uses_kernel_anticheat(dir: &Path) -> bool {
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((current, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in rd.flatten() {
            seen += 1;
            if seen > MAX_SCAN_ENTRIES {
                return false;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.contains("easyanticheat")
                || name.contains("battleye")
                || name == "beclient.dll"
                || name == "beclient_x64.dll"
                || name.contains("vgc")
                || name.contains("vanguard")
                || name.contains("gameguard")
                || name.contains("xigncode")
                || name.contains("anticheat")
            {
                return true;
            }
            if depth < 3 && entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    false
}

/// ذاكرة مؤقتة: فحص كل لعبة مرة وحدة (مفتاح = مجلد اللعبة + وقت تعديله)
static PROBE_CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, (u64, GameProbe)>>> =
    std::sync::OnceLock::new();

fn probe_game_dir(dir: &Path) -> GameProbe {
    let stamp = std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = PROBE_CACHE.get_or_init(Default::default);

    if let Ok(guard) = cache.lock() {
        if let Some((cached_stamp, probe)) = guard.get(dir) {
            if *cached_stamp == stamp {
                return probe.clone();
            }
        }
    }

    let mut probe = GameProbe::default();
    if let Some(exe) = find_main_exe(dir) {
        probe.exe = Some(exe.display().to_string());
        if let Some(info) = pe::probe_file(&exe) {
            probe.dx = Some(info.dx().as_str().to_string());
            probe.is_64 = Some(info.is_64);
            if let Some(blocker) = info.dx().blocker() {
                probe.issues.push(blocker.to_string());
            }
            if !info.is_64 {
                probe.issues.push("x86".into());
            }
            if info.dx() == pe::DxLevel::None && info.dotnet {
                probe.issues.push("dotnet".into());
            }
            // D3DX9 / d3dcompiler: نسخ Wine المدمجة تنهار في ألعاب DX9 القديمة
            probe.needs_d3dx9 = info
                .imports
                .iter()
                .any(|i| i.starts_with("d3dx9_") || i.starts_with("d3dx10_"));
            probe.needs_d3dcompiler = info.imports.iter().any(|i| i.starts_with("d3dcompiler_"));
            crate::debuglog::raw(
                "info",
                "library",
                &format!("game_probe {} | {}", exe.display(), info.summary()),
            );
        }
    }
    if uses_kernel_anticheat(dir) {
        probe.issues.push("anticheat".into());
    }

    if let Ok(mut guard) = cache.lock() {
        guard.insert(dir.to_path_buf(), (stamp, probe.clone()));
    }
    probe
}

fn scan() -> Vec<SteamGame> {
    let libs = library_roots();
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
            let pairs = vdf_pairs(&text);
            let Some(dir) = pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("installdir"))
                .and_then(|(_, v)| safe_component(v))
                .map(|dir| apps.join("common").join(dir))
            else {
                continue;
            };
            if !dir.is_dir() || games.contains_key(&appid) {
                continue;
            }

            let probe = probe_game_dir(&dir);
            let plan = plan_for_appid(&probe, appid);
            games.insert(
                appid,
                SteamGame {
                    appid,
                    name,
                    size_bytes: size,
                    probe,
                    plan,
                },
            );
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

/// ملف تعريف اللعبة (التركيبة التي نجحت معها) — من `combo`، وإلا الافتراضي
pub fn profile_for(appid: u32) -> crate::combo::GameProfile {
    crate::combo::load(appid).unwrap_or_else(|| crate::combo::GameProfile::default_for(appid))
}

/// يترجم ملف التعريف إلى قيم الريجستري (native/builtin لكل DLL)
///
/// **مطابقة الواقع:** ما نقول `native` إلا إذا كانت الملفات موجودة فعلاً،
/// وإلا Wine يرجع للنسخة المدمجة بصمت ويبقى الملف «يكدب» على التطبيق.
fn profile_overrides(profile: &crate::combo::GameProfile) -> graphics::LayerOverrides {
    use crate::combo::{Compiler, D3d9Layer, D3dx9Layer};
    let dxvk_ready = graphics::is_installed() && graphics::d3d9_available();
    let d3dx_ready = crate::d3dx::is_installed();
    let want_dxvk = profile.d3d9 == D3d9Layer::Dxvk;
    let want_native_d3dx = profile.d3dx9 == D3dx9Layer::Native;
    let want_native_compiler = profile.compiler == Compiler::Native;
    graphics::LayerOverrides {
        d3d9: if want_dxvk && dxvk_ready {
            "native"
        } else {
            "builtin"
        },
        d3d10core: if want_dxvk && dxvk_ready {
            "native"
        } else {
            "builtin"
        },
        d3d11: if want_dxvk && dxvk_ready {
            "native"
        } else {
            "builtin"
        },
        d3dx9_43: if want_native_d3dx && d3dx_ready {
            "native"
        } else {
            "builtin"
        },
        d3dcompiler_43: if want_native_compiler && d3dx_ready {
            "native"
        } else {
            "builtin"
        },
    }
}

/// يطبّق كل=settings ملف التعريف على القارورة:
/// التجاوزات (افتراضي عام + خاص بلعبة) + محرك الرسم + الـoverlay.
async fn apply_profile(
    wine: &Path,
    prefix: &Path,
    profile: &crate::combo::GameProfile,
    app_exe: Option<&str>,
) -> Result<(), String> {
    let ov = profile_overrides(profile);
    graphics::ensure_layer(wine, prefix, &ov, profile.appid, app_exe).await?;

    // محرك الرسم لـWineD3D: vulkan (عبر MoltenVK) أو gl (الأقدم)
    let renderer = match profile.renderer {
        crate::combo::Renderer::Vulkan => "vulkan",
        crate::combo::Renderer::Gl => "gl",
    };
    let _ = graphics::ensure_renderer(wine, prefix, renderer).await;

    // Steam overlay: هوك على IDirect3DDevice9 — سبب معروف لانهيارات DX9.
    // نكتب القيمة **دائماً** (مفعّل/معطّل) حتى ما يبقى معطّلاً للأبد.
    if let Some(exe) = app_exe {
        let _ = exe;
    }
    let mut cmd = tokio::process::Command::new(wine);
    cmd.env("WINEPREFIX", prefix)
        .env("WINEMSYNC", "1")
        .arg("reg")
        .arg("add")
        .arg(format!(
            "HKCU\\Software\\Valve\\Steam\\Apps\\{}",
            profile.appid
        ))
        .arg("/v")
        .arg("OverlayEnable")
        .arg("/t")
        .arg("REG_DWORD")
        .arg("/d")
        .arg(if profile.overlay { "1" } else { "0" })
        .arg("/f")
        .stdin(std::process::Stdio::null());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await;
    Ok(())
}

/// يستخرج تقرير انهيار Wine (نص winedbg) من مخرجات اللعبة
fn extract_crash_report(text: &str) -> Option<String> {
    let idx = text.rfind("Unhandled exception:")?;
    let rest = &text[idx..];
    // **كامل** = فيه سطر Backtrace. لو التقطناه وهو يكتب ⇒ جدول الوحدات
    // ناقص ⇒ تصنيف خاطئ (D3DX المدمجة تبان «أصلية») ⇒ نبدّل اللعبة للتركيبة الغلط.
    if !rest.contains("Backtrace:") {
        return None;
    }
    let end = rest
        .find("\nSystem information:")
        .map(|i| (i + 600).min(rest.len()))
        .unwrap_or_else(|| rest.len().min(24_000));
    Some(rest[..end].to_string())
}

/// يطبّع اسم وحدة/ملف للمقارنة: بلا مسار، بلا امتداد، أحرف صغيرة
fn normalize_module_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let base = base.strip_suffix(".exe").unwrap_or(base);
    let base = base.strip_suffix(".dll").unwrap_or(base);
    base.to_ascii_lowercase()
}

/// يراقب مخرجات اللعبة ويكتشف تقرير الانهيار ليعالجها تلقائياً.
///
/// **آلية الحلقة المغلقة:** نقرأ تقرير winedbg ⇒ نصنّفه ⇒ نختار التركيبة
/// التالية ⇒ نعيد التشغيل. هذا اللي يخلّي «الزر يشتغل» بدل ما نخسر وقت
/// المستخدم في كل دورة يدوية.
fn spawn_crash_watcher(
    app: AppHandle,
    appid: u32,
    profile: crate::combo::GameProfile,
    attempt: u8,
    game_exe: Option<String>,
) {
    tauri::async_runtime::spawn(async move {
        // **سجل ستيم** هو where تكتب أخطاء اللعبة: اللعبة يطلقها عميل ستيم
        // فترث مقابض stdout/stderr الخاصة بستيم (وليس سجل إطلاق الأمر).
        // **السجلّان**: أخطاء اللعبة تكتب في أي منهما حسب طريقة الإطلاق
        // (سجل ستيم = العميل الشغّال، سجل إطلاق الأمر = العملية التي أطلقت اللعبة).
        let logs = [steam::steam_log_path(), launch_log_path()];
        // نبدأ من **نهاية** كل سجل: أي انهيار قديم ما يخلّينا نعيد المحاولة
        let mut seen: [usize; 2] = [
            std::fs::metadata(&logs[0])
                .map(|m| m.len() as usize)
                .unwrap_or(0),
            std::fs::metadata(&logs[1])
                .map(|m| m.len() as usize)
                .unwrap_or(0),
        ];
        for _ in 0..120 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let mut report: Option<String> = None;
            for (i, log) in logs.iter().enumerate() {
                let Ok(text) = std::fs::read_to_string(log) else {
                    continue;
                };
                // ستيم قُطع سجله عند إعادة التشغيل ⇒ نرجع للمؤشر صفر
                if text.len() < seen[i] {
                    seen[i] = 0;
                }
                if text.len() <= seen[i] {
                    continue;
                }
                let fresh = &text[seen[i].min(text.len())..];
                let Some(found) = extract_crash_report(fresh) else {
                    // تقرير ناقص (يكتب) ⇒ ما نقدّم المؤشر، ننتظر الجولة الجاية
                    continue;
                };
                seen[i] = text.len();
                report = Some(found);
                break;
            }
            let Some(report) = report else {
                continue;
            };
            // السجل مشترك بين الألعاب ⇒ نتأكد إن الانهيار لِهذه اللعبة
            // (وإلا نطبّق حكم لعبة أخرى على هذه)
            if let Some(exe) = &game_exe {
                // جدول Wine يكتب اسم الموديول **بلا** امتداد ⇒ نطبّع الطرفين
                let name = normalize_module_name(exe);
                let parsed = crate::triage::parse_report(&report);
                let mentions = parsed
                    .as_ref()
                    .map(|r| {
                        r.modules
                            .iter()
                            .any(|m| normalize_module_name(&m.name) == name)
                    })
                    .unwrap_or(false);
                if !mentions {
                    crate::debuglog::raw(
                        "debug",
                        "steam",
                        &format!("crash report ignored appid={appid} (not {name})"),
                    );
                    continue;
                }
            }
            let Some(verdict) = crate::triage::triage(&report) else {
                continue;
            };
            crate::debuglog::raw(
                "warn",
                "steam",
                &format!(
                    "crash verdict appid={appid} class={} action={} module={:?}",
                    crate::triage::class_as_str(verdict.class),
                    verdict.action,
                    verdict.crashed_module
                ),
            );
            write_crash_state(
                appid,
                &CrashState {
                    at_ms: crate::now_ms(),
                    appid,
                    class: crate::triage::class_as_str(verdict.class).to_string(),
                    headline_ar: verdict.headline_ar.clone(),
                    next_ar: verdict.next_ar.clone(),
                    action: verdict.action.clone(),
                    attempts: attempt as u32 + 1,
                },
            );
            let _ = app.emit("jisr-crash", appid);

            // إجراءات «الطبقة مو مثبّتة أصلاً» ⇒ نبدأ التنزيل بدل تبديل راية
            match verdict.action.as_str() {
                "install-d3dx9-native" if !crate::d3dx::is_installed() => {
                    crate::debuglog::raw(
                        "info",
                        "steam",
                        "D3DX9 الأصلي مو مثبّت — نبدأ تنزيل حزمة DirectX",
                    );
                    crate::download::ensure_d3dx9_layer(&app);
                }
                "enable-dxvk" if !graphics::is_installed() => {
                    crate::debuglog::raw("info", "steam", "DXVK مو مثبّت — نبدأ تنزيله");
                    crate::download::ensure_graphics_layer(&app);
                }
                _ => {}
            }

            // إعادة محاولة تلقائية بتركيبة مختلفة (حد ٣ محاولات)
            if attempt < 2 {
                if let Some(next) = profile.next_candidate(&verdict.action) {
                    if auto_retry_enabled() {
                        let _ = crate::combo::save(&next);
                        crate::debuglog::raw(
                            "info",
                            "steam",
                            &format!(
                                "auto retry appid={appid} attempt={} profile={}",
                                attempt + 1,
                                next.describe_ar()
                            ),
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        // استدعاء متكرر غير مباشر ⇒ نحتاج Box (الحجم لا ينتهي)
                        match Box::pin(launch_game(app.clone(), appid, next, attempt + 1, true))
                            .await
                        {
                            Ok(()) => {}
                            Err(e) => {
                                // فشل إعادة التشغيل ⇒ نخبر المستخدم (مو نبلع الخطأ)
                                crate::debuglog::raw(
                                    "warn",
                                    "steam",
                                    &format!("auto retry failed appid={appid}: {e}"),
                                );
                                write_crash_state(
                                    appid,
                                    &CrashState {
                                        at_ms: crate::now_ms(),
                                        appid,
                                        class: "retry-failed".into(),
                                        headline_ar: "ما قدرنا نعيد تشغيل اللعبة تلقائياً".into(),
                                        next_ar: e,
                                        action: verdict.action.clone(),
                                        attempts: attempt as u32 + 1,
                                    },
                                );
                                let _ = app.emit("jisr-crash", appid);
                            }
                        }
                    }
                }
            }
            return;
        }
    });
}

/// هل نعيد المحاولة تلقائياً بعد الانهيار؟ (إعداد — افتراضياً نعم)
pub fn auto_retry_enabled() -> bool {
    match std::fs::read_to_string(crate::data_dir().join("auto-retry.txt")) {
        Ok(t) => !t.trim().eq_ignore_ascii_case("off") && !t.trim().eq_ignore_ascii_case("0"),
        Err(_) => true,
    }
}

/// تفعيل/تعطيل إعادة المحاولة التلقائية
#[tauri::command]
pub fn steam_auto_retry(enabled: bool) -> Result<bool, String> {
    let path = crate::data_dir().join("auto-retry.txt");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("تعذّر تجهيز مجلد الإعدادات: {e}"))?;
    }
    std::fs::write(&path, if enabled { "on\n" } else { "off\n" })
        .map_err(|e| format!("تعذّر حفظ الإعداد: {e}"))?;
    Ok(enabled)
}

/// حالة إعادة المحاولة التلقائية
#[tauri::command]
pub fn steam_auto_retry_status() -> bool {
    auto_retry_enabled()
}

/// ملف التعريف الحالي للعبة (للواجهة)
#[tauri::command]
pub fn steam_game_profile(appid: u32) -> serde_json::Value {
    profile_for(appid).to_json()
}

/// تصفير ملف تعريف اللعبة (رجوع للافتراضي)
#[tauri::command]
pub fn steam_game_profile_reset(appid: u32) -> Result<(), String> {
    crate::combo::clear(appid).map_err(|e| format!("تعذّر تصفير الملف: {e}"))
}

/// آخر انهيار مُصنَّف للعبة
#[tauri::command]
pub fn steam_crash_state(appid: u32) -> Option<CrashState> {
    read_crash_state(appid)
}

/// كل حالات الانهيار المحفوظة (لعرضها على كل الألعاب مرة واحدة)
#[tauri::command]
pub fn steam_crash_states() -> Vec<CrashState> {
    let dir = crate::data_dir().join("crashes");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            // ملفات .json فقط (نتجاهل ملفات .tmp المؤقتة)
            if e.path().extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(e.path()) {
                if let Ok(state) = serde_json::from_str::<CrashState>(&text) {
                    out.push(state);
                }
            }
        }
    }
    out
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
    let profile = profile_for(appid);
    launch_game(app, appid, profile, 0, false).await
}

/// ينفّذ التشغيل بملف تعريف محدّد (ويستدعي نفسه عند إعادة المحاولة)
async fn launch_game(
    app: AppHandle,
    appid: u32,
    profile: crate::combo::GameProfile,
    attempt: u8,
    is_retry: bool,
) -> Result<(), String> {
    // فحص الجاهزية في **كل** إطلاق (مو بس الأول) — بعد إعادة تشغيل ستيم
    // من طرف المستخدم قد يكون قد أُغلق ⇒ ما نرسل أمراً لعميل ميت.
    if let Some(st) = app.try_state::<steam::SteamState>() {
        if !steam::is_running(&st) {
            return Err("ستيم مو شغّال — شغّله ثم أعد المحاولة".into());
        }
    }
    let Some((mac_exe, dos_exe)) = steam::steam_exe() else {
        return Err("ستيم مو مثبّت بعد".into());
    };
    let Some(wine) = bottle::wine_bin() else {
        return Err("ما لقينا محرك Wine — ثبّت المحرك أول".into());
    };

    let prefix = bottle::bottle_dir(bottle::MAIN_ID);
    bottle::sync_ca_bundle(&prefix);
    // فحص اللعبة قبل التشغيل: نسجّل DirectX والمعمارية والموانع المعروفة
    // (هذا اللي يخلّينا نعرف السبب بدل التخمين)
    let probe = game_dir(appid).map(|dir| (dir.clone(), probe_game_dir(&dir)));
    if let Some((_, p)) = &probe {
        crate::debuglog::raw(
            "info",
            "steam",
            &format!(
                "game_probe appid={appid} dx={} x64={} issues={:?} exe={}",
                p.dx.as_deref().unwrap_or("?"),
                p.is_64.map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
                p.issues,
                p.exe.as_deref().unwrap_or("?")
            ),
        );
    }
    // ملف التعريف: لكل لعبة تركيبتها (DXVK/D3DX9/المترجم/محرك الرسم/الـoverlay).
    // **أولاً الملفات** (DXVK + D3DX9 الأصلي) — ثم **التجاوزات** حسب التعريف.
    let deployed = graphics::deploy_to_prefix(&prefix).is_ok();
    let d3dx_ok = crate::d3dx::deploy_files(&prefix).is_ok();
    // اسم ملف اللعبة (بلا مسار) — نستخدمه لتركيبة خاصة بهذه اللعبة
    let game_exe: Option<String> = probe
        .as_ref()
        .and_then(|(_, p)| p.exe.as_ref())
        .map(|e| e.rsplit(['/', '\\']).next().unwrap_or(e).to_string());
    let profile_result = apply_profile(&wine, &prefix, &profile, game_exe.as_deref()).await;
    let reg_ok = profile_result.is_ok();
    if let Err(e) = &profile_result {
        crate::debuglog::raw("warn", "steam", &format!("profile apply failed: {e}"));
    }
    // ⚠️ لا نفرض وضع النافذة تلقائياً: يكتب في ملف اللعبة الحقيقي داخل
    // مكتبة ستيم (خارج مجلد التطبيق) وما فيه رجوع تلقائي. يبقى يدوياً
    // من زر «نافذة» في بطاقة اللعبة.
    // حزام أمان للعرض: يمنع الألعاب القديمة من تعتيم الشاشة أو احتكارها بالكامل
    let safety = bottle::ensure_mac_safety(&wine, &prefix).await;

    // حقائق الطبقة — نعرضها في الواجهة عشان المستخدم يعرف بدون سجل
    let win = prefix.join("drive_c").join("windows");
    let d3d9_64 = win.join("system32").join("d3d9.dll").is_file();
    let d3d9_32 = win.join("syswow64").join("d3d9.dll").is_file();
    let vulkan_32 = engine_has_i386_vulkan(&wine);
    crate::debuglog::raw(
        "info",
        "steam",
        &format!(
            "layer state: dxvk_deployed={deployed} registry_overrides={reg_ok} d3dx9={d3dx_ok} profile={} mac_safety={safety} d3d9_64={d3d9_64} d3d9_32={d3d9_32} vulkan_32={vulkan_32}",
            profile.describe_ar()
        ),
    );
    if let Some((dir, p)) = &probe {
        steam::write_launch_state(&steam::LaunchState {
            at_ms: crate::now_ms(),
            game: game_name(appid).unwrap_or_else(|| {
                dir.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            }),
            dx: p.dx.clone().unwrap_or_else(|| "?".into()),
            is_64: p.is_64.unwrap_or(true),
            dxvk_deployed: deployed,
            registry_ok: reg_ok,
            d3d9_64,
            d3d9_32,
            vulkan_32,
        });
    }
    if let Some(dir) = mac_exe.parent() {
        steam::prepare_steam_ui(&app, dir);
    }

    // بلا shell: appid رقم فقط، وsteam.exe يوجّه الأمر للعميل الشغّال
    let mut cmd = tokio::process::Command::new(&wine);
    cmd.env("WINEPREFIX", &prefix);
    bottle::apply_wine_env(&mut cmd, true);
    graphics::apply_env(&mut cmd, true);
    let (out, err) = launch_stdio(appid);
    cmd.arg(&dos_exe)
        .arg("-applaunch")
        .arg(appid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err);
    if let Some(dir) = mac_exe.parent() {
        cmd.current_dir(dir);
    }

    // سجل كامل للأمر — لو ما اشتغلت اللعبة نعرف وش انطلق بالضبط
    crate::debuglog::raw(
        "info",
        "steam",
        &format!(
            "spawn game appid={appid} wine={} exe={} prefix={} graphics={} dxvk_deployed={deployed} mac_safety={safety}",
            wine.display(),
            dos_exe,
            prefix.display(),
            graphics::active_layer()
        ),
    );

    // وصف مجلد اللعبة (تشخيص ميداني): هل فيه settings.txt؟ وما الملفات التنفيذية؟
    if let Some(dir) = install_dir(appid) {
        let mut exes: Vec<String> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if exes.len() >= 8 {
                    break;
                }
                let p = e.path();
                if p.extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("exe"))
                    .unwrap_or(false)
                {
                    exes.push(
                        p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    );
                }
            }
        }
        let has_settings = find_file_ci(&dir, "settings.txt").is_some();
        crate::debuglog::raw(
            "info",
            "steam",
            &format!(
                "game_dir={} settings_txt={has_settings} exes={exes:?}",
                dir.display()
            ),
        );
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
    // حالة انهيار سابقة؟ نمسحها **عند إطلاق المستخدم** فقط — لو مسحناها
    // عند إعادة المحاولة الآلية، مايشوف المستخدم سبب إعادة التشغيل.
    if !is_retry {
        clear_crash_state(appid);
    }

    // مراقب الانهيار: يقرأ تقرير winedbg ⇒ يصنّفه ⇒ يجرّب التركيبة التالية
    spawn_crash_watcher(app.clone(), appid, profile.clone(), attempt, game_exe);

    // ننتظر الأمر القصير بالخلفية — خطأ الخروج فقط يُسجَّل (بلا إزعاج)
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        match child.wait().await {
            Ok(st) if !st.success() => {
                let tail = launch_tail();
                log(
                    &app2,
                    "warn",
                    log_keys::STEAM_GAME_LAUNCH_FAILED,
                    json!({ "appid": appid, "error": format!("رمز {}", st.code().unwrap_or(-1)), "output": tail }),
                );
            }
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

/// حالة آخر انهيار مُصنَّف (نعرضها في بطاقة اللعبة)
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CrashState {
    /// متى صار الانهيار
    pub at_ms: u64,
    /// رقم اللعبة
    pub appid: u32,
    /// تصنيف السبب (d3dx_builtin, d3dx_native, d3d9_builtin, vulkan, wow64, missing_dll, overlay, game)
    pub class: String,
    /// جملة عربية مختصرة
    pub headline_ar: String,
    /// الخطوة التالية بالعربي
    pub next_ar: String,
    /// رمز الإجراء (نستعمله لاختيار التركيبة التالية)
    pub action: String,
    /// كم محاولة تلقائية جرت
    pub attempts: u32,
}

/// ملف حالة الانهيار (لكل لعبة) — نخزّنه كـJSON بسيط
fn crash_state_path(appid: u32) -> PathBuf {
    crate::data_dir()
        .join("crashes")
        .join(format!("{appid}.json"))
}

/// اكتب حالة الانهيار (لعرضها في الواجهة)
pub fn write_crash_state(appid: u32, state: &CrashState) {
    let path = crash_state_path(appid);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(json) = serde_json::to_string(state) else {
        return;
    };
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// اقرأ حالة الانهيار (None = ما صار انهيار مسجّل)
pub fn read_crash_state(appid: u32) -> Option<CrashState> {
    let text = std::fs::read_to_string(crash_state_path(appid)).ok()?;
    serde_json::from_str(&text).ok()
}

/// امسح حالة الانهيار (بعد ما نجح التشغيل)
pub fn clear_crash_state(appid: u32) {
    let _ = std::fs::remove_file(crash_state_path(appid));
}

/// ملف مخرجات تشغيل اللعبة — كلام ستيم/اللعبة الذي يمرّ عبر نفس المقابض.
/// قبل هذا كانت المخرجات تذهب إلى null: أي خطأ من اللعبة كان يضيع تماماً.
fn launch_log_path() -> PathBuf {
    crate::data_dir().join("logs").join("game-launch.log")
}

/// مقابض الكتابة: السطران يخرجان لنفس الملف (بلا فقد أي رسالة)
fn launch_stdio(appid: u32) -> (std::process::Stdio, std::process::Stdio) {
    use std::io::Write;
    let path = launch_log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let header = format!(
        "\n===== launch appid={appid} @ {} =====\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return (std::process::Stdio::null(), std::process::Stdio::null());
    };
    let _ = f.write_all(header.as_bytes());
    let _ = f.flush();
    match (f.try_clone(), f.try_clone()) {
        (Ok(a), Ok(b)) => (std::process::Stdio::from(a), std::process::Stdio::from(b)),
        _ => (std::process::Stdio::from(f), std::process::Stdio::null()),
    }
}

/// آخر أسطر من مخرجات التشغيل — تُرفق برسالة الفشل (تشخيص ميداني)
fn launch_tail() -> String {
    std::fs::read(launch_log_path())
        .map(|b| crate::tail_lines(&b, 8))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ملف التعريف يترجم لقيم الريجستري بشكل صحيح
    #[test]
    fn profile_maps_to_registry_overrides() {
        use crate::combo::{Compiler, D3d9Layer, D3dx9Layer, GameProfile, Renderer};
        let mut p = GameProfile::default_for(48010);
        let ov = profile_overrides(&p);
        // **مطابقة الواقع:** ما نقول native إلا لو الملف موجود فعلاً
        // (على جهاز التطوير ما فيه DXVK ولا D3DX9 مثبّتين ⇒ builtin)
        let dxvk_ready = graphics::is_installed() && graphics::d3d9_available();
        let d3dx_ready = crate::d3dx::is_installed();
        assert_eq!(ov.d3d9, if dxvk_ready { "native" } else { "builtin" });
        assert_eq!(ov.d3dx9_43, if d3dx_ready { "native" } else { "builtin" });
        assert_eq!(
            ov.d3dcompiler_43,
            if d3dx_ready { "native" } else { "builtin" }
        );

        p.d3d9 = D3d9Layer::WineD3D;
        p.d3dx9 = D3dx9Layer::Builtin;
        p.compiler = Compiler::Builtin;
        let ov2 = profile_overrides(&p);
        assert_eq!(ov2.d3d9, "builtin");
        assert_eq!(ov2.d3dx9_43, "builtin");
        assert_eq!(ov2.d3dcompiler_43, "builtin");
        // d3d10core/d3d11 يتبعان DXVK نفسه
        assert_eq!(ov2.d3d11, "builtin");
        // والقيم تتطابق مع سجل Gimme (الفرق الوحيد d3d10core/d3d11 يتبعان DXVK)
        let ov3 = profile_overrides(&GameProfile::default_for(48010));
        assert_eq!(ov3.d3d9, ov.d3d9);
        let _ = p.renderer == Renderer::Vulkan;
    }

    /// نستخرج تقرير الانهيار من مخرجات مختلطة (فيها كلام ستيم قبل التقرير)
    #[test]
    fn extracts_crash_report_from_noisy_output() {
        let text = "0188:fixme:foo something\n\
                    Unhandled exception: page fault on read access to 0x00000000 in wow64 32-bit code (0x1).\n\
                    Backtrace:\n\
                    =>0 0x1 in d3dcompiler_43 (+0x1) (0x2)\n\
                    System information:\n\
                    Wine build: wine-11.18\n";
        let report = extract_crash_report(text).expect("لازم نلقى التقرير");
        assert!(report.starts_with("Unhandled exception:"));
        assert!(report.contains("d3dcompiler_43"));

        // ما فيه انهيار ⇒ None (ومو panic)
        assert!(extract_crash_report("0188:fixme:foo something\n").is_none());
    }

    /// الحلقة المغلقة: انهيار D3DX الأصلي ⇒ نبدّل **المترجم فقط**
    /// (D3DX9 يبقى أصلياً) — الجراحة الدقيقة.
    #[test]
    fn crash_verdict_drives_next_candidate() {
        let profile = crate::combo::GameProfile::default_for(48010);
        let report = "Unhandled exception: page fault on read access to 0x0 in wow64 32-bit code (0x1).\n\
                      Backtrace:\n\
                      =>0 0x1 in d3dcompiler_43 (+0x1) (0x2)\n\
                      Modules:\n\
                      PE  1000-2000  --none--  d3dcompiler_43\n\
                      PE  3000-4000  --none--  d3dx9_43\n\
                      System information:\n\
                      Wine build: wine-11.18\n";
        let verdict = crate::triage::triage(report).expect("لازم نصنّف");
        assert_eq!(verdict.action, "try-builtin-compiler");
        let next = profile
            .next_candidate(&verdict.action)
            .expect("لازم تكون هناك تركيبة تالية");
        assert_eq!(next.d3dx9, crate::combo::D3dx9Layer::Native);
        assert_eq!(next.compiler, crate::combo::Compiler::Builtin);
        assert_eq!(next.attempts, profile.attempts + 1);
    }

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

    #[test]
    fn patch_windowed_flips_the_flag_only() {
        let text = "-identifiers-\r\nwindowedmode = false\r\nbackbufferwidth = 1280\r\n";
        let out = patch_windowed(text, true).expect("المفتاح موجود");
        assert!(out.contains("windowedmode = true"));
        assert!(!out.contains("false"));
        assert!(out.contains("backbufferwidth = 1280"));
        // نمط نهاية السطر محفوظ (محرك اللعبة يقرأه بنفس الطريقة)
        assert!(out.contains("\r\n"));
        assert!(!out.contains("\n\r"));
    }

    #[test]
    fn patch_windowed_is_case_insensitive_and_ignores_prefix_keys() {
        let text = "WindowedMode=false\nwindowedmodeExtra=1\nfoo=2\n";
        let out = patch_windowed(text, true).unwrap();
        assert!(out.contains("WindowedMode = true"));
        assert!(out.contains("windowedmodeExtra=1"), "مفتاح مشابه ما ينمس");
        assert!(out.contains("foo=2"));
    }

    #[test]
    fn patch_windowed_returns_none_when_unsupported() {
        assert!(patch_windowed("fullscreen=1\n", true).is_none());
        assert!(patch_windowed("", true).is_none());
    }
}
