// ============================================================
//  جسر — تدفّق ستيم (المثبّت الرسمي → القارورة الرئيسية → التشغيل)
//  • المثبّت: من سيرفرات Valve الرسمية حصراً + بصمة SHA-256 مثبّتة،
//    ويُتحقق منها مرة ثانية قبل كل تشغيل (ثقة صفرية)
//  • ننسخ المثبّت داخل القارورة (C:) قبل تشغيله — Wine ما يقدر
//    يشغّل ملفات من نظام الماك مباشرة (وبلا أي ربط لملفات الماك)
//  • واجهة ستيم (الشاشة السوداء — درس التجربة #5):
//    CEF يطلب سياق GLES 3.0 لعملية GPU منفصلة، وWine على الماك يوصلها لسقف
//    GLES 2.0 → نافذة سوداء. الحل المعتمد عند مجتمع Wine/ماك (Gcenx نفسه):
//    غلاف صغير يستبدل `bin/cef/cef.win64/steamwebhelper.exe` ويشغّل الأصل
//    (المحفوظ بجانبه باسم steamwebhelper_real.exe) مع
//    `--in-process-gpu --use-angle=vulkan --ignore-gpu-blocklist`
//    → الرسم داخل نفس العملية عبر ANGLE → Vulkan → MoltenVK → Metal.
//    الغلاف مدمج بالظهر (tools/steamwebhelper-shim) ويُعاد تثبيته قبل كل تشغيل،
//    لأن تحديث ستيم ممكن يرجّع الملف الأصلي.
//  • على ويندوز (بيئة تطوير): أخطاء واضحة وآمنة فقط — نفس نمط الألعاب
// ============================================================
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{bottle, download, engine, graphics, log, log_keys};

/// تغيّرت حالة ستيم (تنزيل جاهز/تثبيت/تشغيل...)
const EVENT: &str = "jisr-steam";

/// معرّف مثبّت ستيم في الكتالوج الرسمي (download.rs)
pub const INSTALLER_ID: &str = "steam-setup";

/// مكان المثبّت داخل القارورة قبل تشغيله
const DROP_IN_BOTTLE: &str = "users/Public/SteamSetup.exe";

/// حالة ستيم الوظيفية (قفل واحد يمنع تعارض الأوامر)
#[derive(Clone, Copy, PartialEq, Default)]
enum Phase {
    #[default]
    Idle,
    Installing,
    Running,
}

#[derive(Default)]
pub struct SteamState {
    phase: Mutex<Phase>,
    stopped: AtomicBool, // إيقاف بطلب المستخدم (نميّزه عن الفشل الحقيقي)
}

/// حالة آخر تشغيل لعبة — نعرضها في الواجهة بدل ما يفتح المستخدم السجل
#[derive(Serialize, serde::Deserialize, Clone, Default)]
pub struct LaunchState {
    pub at_ms: u64,
    pub game: String,
    pub dx: String,
    pub is_64: bool,
    /// هل ملفات DXVK انتشرت داخل القارورة؟
    pub dxvk_deployed: bool,
    /// هل تجاوزات الريجستري (native) انكتبت؟
    pub registry_ok: bool,
    /// d3d9.dll موجود في system32 (64-بت)؟
    pub d3d9_64: bool,
    /// d3d9.dll موجود في syswow64 (32-بت)؟
    pub d3d9_32: bool,
    /// vulkan-1.dll للـ32-بت موجود في المحرك؟ (بدونه DXVK 32-بت ما يحمّل)
    pub vulkan_32: bool,
}

fn launch_state_path() -> std::path::PathBuf {
    crate::data_dir().join("last-launch.json")
}

pub(crate) fn write_launch_state(state: &LaunchState) {
    if let Ok(text) = serde_json::to_string(state) {
        let _ = std::fs::write(launch_state_path(), text);
    }
}

fn read_launch_state() -> Option<LaunchState> {
    serde_json::from_str(&std::fs::read_to_string(launch_state_path()).ok()?).ok()
}

#[derive(Serialize, Clone)]
pub struct SteamStatus {
    pub installer_ready: bool,
    pub installed: bool,
    pub steam_exe: Option<String>,
    pub installing: bool,
    pub running: bool,
    pub bottle_ready: bool,
    /// رسم واجهة ستيم بالـGPU (أحدّ) بدل الرسم البرمجي (أكثر ثباتاً)
    pub ui_gpu: bool,
    /// معامل تحجيم الواجهة: 2 على شاشات Retina (حاد) و1 على غيرها
    pub desktop_scale: u8,
    /// آخر لعبة شغّلناها: هل الطبقة انتشرت فعلاً؟
    pub last_launch: Option<LaunchState>,
}

/// ملف المثبّت المنزّل على الماك (تحققنا من بصمته وقت التنزيل — ونعيد التحقق قبله)
fn installer_path() -> Option<PathBuf> {
    let asset = download::find_asset(INSTALLER_ID)?;
    engine::archive_path(asset.url)
}

/// ملف تسجيل كلام ستيم الكامل (stdout + stderr) للآخر تشغيل —
/// تشخيص ميداني: أي مشكلة تبان من هالملف مباشرة بدون تخمين
fn run_log_path() -> PathBuf {
    crate::data_dir().join("logs").join("steam-last-run.log")
}

/// نوجّه كلام ستيم كامل لملف (بدل الأنابيب اللي تضيع عند الإيقاف اليدوي)
fn stdio_to_log() -> (Stdio, Stdio) {
    let path = run_log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::File::create(&path).and_then(|f| f.try_clone().map(|c| (f, c))) {
        Ok((out, err)) => (Stdio::from(out), Stdio::from(err)),
        Err(_) => (Stdio::null(), Stdio::null()),
    }
}

/// آخر سطور من كلام ستيم — تُضاف لرسائل الفشل السريع
fn log_tail(n: usize) -> String {
    std::fs::read(run_log_path())
        .map(|b| crate::tail_lines(&b, n))
        .unwrap_or_default()
}

/// steam.exe داخل القارورة الرئيسية؟ يرجع (مسار الماك، مسار ويندوز داخل C:)
pub(crate) fn steam_exe() -> Option<(PathBuf, String)> {
    let dc = bottle::bottle_dir(bottle::MAIN_ID).join("drive_c");
    for dir in ["Program Files (x86)", "Program Files"] {
        let p = dc.join(dir).join("Steam").join("steam.exe");
        if p.is_file() {
            return Some((p, format!("C:\\{dir}\\Steam\\steam.exe")));
        }
    }
    None
}

/// غلاف CEF المدمج (يُبنى من tools/steamwebhelper-shim → assets/steamwebhelper.exe)
static CEF_SHIM: &[u8] = include_bytes!("../assets/steamwebhelper.exe");

/// بصمة داخل الغلاف نفسه — نتعرف بها على أن الملف الحالي غلافنا لا الأصل.
/// نقبل V1 (نسخ قديمة منشورة) وV2 (الدور: المتصفح فقط) حتى نستبدلها بأمان.
const CEF_SHIM_MARKERS: [&[u8]; 2] = [b"JISR_CEF_SHIM_V1", b"JISR_CEF_SHIM_V2"];

/// هل الملف الحالي هو غلاف Jisr؟ (البصمة داخل أول ميغابايت)
pub(crate) fn is_cef_shim(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = vec![0u8; 1 << 20];
    let n = f.read(&mut buf).unwrap_or(0);
    CEF_SHIM_MARKERS
        .iter()
        .any(|m| buf[..n].windows(m.len()).any(|w| w == *m))
}

/// هل الملف نسخة أصلية سليمة من Valve؟ (حجم غلاف CEF الحقيقي أكبر من ميغابايت)
fn looks_like_real_helper(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 1_000_000)
        .unwrap_or(false)
}

/// كتابة ذرّية: ملف مؤقت ثم تبديل — ما نترك ملفاً نصف مكتوب أبداً
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("jisr-tmp");
    std::fs::write(&tmp, bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            // ويندوز ما يستبدل موجود — نمسح ثم نبدّل (داخل مجلد المستخدم فقط)
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path)
        }
    }
}

/// نستعيد الأصل من النسخة الاحتياطية الدائمة
fn restore_cef_backup(real: &Path, backup_dir: &Path) -> bool {
    let backup = backup_dir.join("steamwebhelper.exe");
    looks_like_real_helper(&backup) && std::fs::copy(&backup, real).is_ok()
}

/// مجلدات CEF داخل ستيم (عادة cef.win64 — نغطي أي cef.win* أخرى بأمان)
fn cef_dirs(steam_dir: &Path) -> Vec<PathBuf> {
    let root = steam_dir.join("bin").join("cef");
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.starts_with("cef.win") && e.path().is_dir()
        })
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// يضمن الغلاف في مجلد CEF واحد.
/// `Some(true)` = ثبّتنا/حدّثنا الآن، `Some(false)` = جاهز، `None` = تعذّر.
fn ensure_shim_in(dir: &Path, backup_dir: &Path) -> Option<bool> {
    let helper = dir.join("steamwebhelper.exe");
    let real = dir.join("steamwebhelper_real.exe");
    if !helper.is_file() {
        crate::debuglog::raw(
            "warn",
            "cef",
            &format!("helper missing: {}", helper.display()),
        );
        return None;
    }

    if is_cef_shim(&helper) {
        // الغلاف مثبّت — نتأكد أن الأصل بجانبه سليم (حجم Valve لا حجم غلاف)
        if looks_like_real_helper(&real) {
            crate::debuglog::raw("debug", "cef", &format!("shim ready: {}", dir.display()));
            return Some(false);
        }
        // الأصل مفقود/تالف/غلاف بالغلط → نستعيده من النسخة الاحتياطية
        let restored = restore_cef_backup(&real, backup_dir);
        crate::debuglog::raw(
            if restored { "info" } else { "error" },
            "cef",
            &format!(
                "restore real helper in {}: {}",
                dir.display(),
                if restored { "ok" } else { "FAILED (no backup)" }
            ),
        );
        return restored.then_some(false);
    }

    // الملف الحالي أصلي (أو نسخة ستيم محدّثة) → نحفظه بجانبه ثم نضع الغلاف
    if std::fs::copy(&helper, &real).is_err() {
        crate::debuglog::raw(
            "error",
            "cef",
            &format!("copy real helper failed: {}", real.display()),
        );
        return None;
    }
    // نسخة احتياطية دائمة (نحدّثها لو القديمة تالفة)
    let backup = backup_dir.join("steamwebhelper.exe");
    if std::fs::create_dir_all(backup_dir).is_ok() && !looks_like_real_helper(&backup) {
        let _ = std::fs::copy(&helper, &backup);
    }
    let installed = write_atomic(&helper, CEF_SHIM).is_ok();
    crate::debuglog::raw(
        if installed { "info" } else { "error" },
        "cef",
        &format!(
            "install shim in {}: {}",
            dir.display(),
            if installed {
                "ok (real helper saved)"
            } else {
                "FAILED"
            }
        ),
    );
    installed.then_some(true)
}

/// نضمن أن ستيم يستخدم غلافنا مع الأصل المحفوظ بجانبه (قبل كل تشغيل —
/// تحديث ستيم ممكن يرجّع الأصل).
pub(crate) fn ensure_cef_shim(steam_dir: &Path, backup_dir: &Path) -> Option<bool> {
    let dirs = cef_dirs(steam_dir);
    if dirs.is_empty() {
        return None;
    }

    let mut handled = false;
    let mut installed = false;
    for dir in dirs {
        if let Some(changed) = ensure_shim_in(&dir, backup_dir) {
            handled = true;
            installed |= changed;
        }
    }
    if !handled {
        None
    } else if installed {
        Some(true)
    } else {
        Some(false)
    }
}

/// إصلاح تنزيلات ستيم على Wine: تعطيل HTTP/2 في عميل ويندوز
/// (سبب شائع لتوقّف التنزيل عند 0 بايت). ملف إعدادات رسمي يقرأه ستيم.
fn ensure_steam_dev_cfg(steam_dir: &Path) {
    const KEY: &str = "@nClientDownloadEnableHTTP2PlatformWindows 0";
    let path = steam_dir.join("steam_dev.cfg");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == KEY) {
        return;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(KEY);
    text.push('\n');
    let _ = write_atomic(&path, text.as_bytes());
}

/// هل ستيم شغّال الآن على مستوى العمليات؟ (حارس أمان قبل لمس ملفات ستيم)
/// pgrep يرجع كود 1 لما مالقى — والمخرج الفارغ يعني ما فيه ستيم.
/// لو فشل pgrep نفسه نعتبره «شغّال» ونمتنع عن التنظيف (أفضل نكون متحفظين).
fn steam_processes_running() -> bool {
    if !cfg!(unix) {
        return false;
    }
    std::process::Command::new("pgrep")
        .arg("-f")
        .arg("steamwebhelper.exe|steam.exe")
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(true)
}

/// إعادة تسمية بقايا كاش/أقفال داخل مجلد واحد (بدون حذف)
fn rename_stale_items(base: &Path, stamp: u64, renamed: &mut Vec<String>) {
    // ذاكرة CEF (htmlcache) — نبنيها من الصفر لو كانت عالقة
    let cache = base.join("htmlcache");
    if cache.is_dir() {
        let bak = base.join(format!("htmlcache.jisr-bak-{stamp}"));
        if std::fs::rename(&cache, &bak).is_ok() {
            renamed.push(cache.display().to_string());
        }
    }

    // أقفال Chromium (Singleton*) — ملفات قديمة ما عادت لازمة
    if let Ok(rd) = std::fs::read_dir(base) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("Singleton") {
                continue;
            }
            let path = entry.path();
            let bak = path.with_file_name(format!("{name}.jisr-bak-{stamp}"));
            if std::fs::rename(&path, &bak).is_ok() {
                renamed.push(path.display().to_string());
            }
        }
    }
}

/// تنظيف بقايا الكاش والأقفال القديمة — سبب معروف موثّق لمشكلة
/// «ستيم يفتح بدون نافذة» (htmlcache/Singleton* عالقة من جلسة سابقة).
/// قواعد الأمان:
///  1. ما نلمس شي إذا فيه أي عملية ستيم حيّة (pgrep).
///  2. نعيد **التسمية** بدل الحذف — كل شي قابل للتراجع (`.jisr-bak-<وقت>`).
///  3. داخل مجلد ستيم وبروفايل مستخدم البottle فقط — ما نخرج منه أبداً.
/// ملاحظة: كاش CEF الحقيقي يعيش في بروفايل مستخدم ويندوز داخل البottle:
/// `drive_c/users/<user>/AppData/Local/Steam/htmlcache` (وليس داخل مجلد ستيم).
pub(crate) fn cleanup_stale_steam_state(steam_dir: &Path) {
    cleanup_stale_steam_state_with(steam_dir, steam_processes_running())
}

/// نفس الدالة لكن مع «فيه ستيم شغّال» كمعلَم صريح.
/// السبب: الاختبار ما يعتمد على pgrep (نتائج الجهاز بتختلف) — والقرار
/// يبقى قابلاً للاختبار deterministically.
pub(crate) fn cleanup_stale_steam_state_with(steam_dir: &Path, steam_running: bool) {
    if steam_running {
        crate::debuglog::raw(
            "debug",
            "steam",
            "cleanup skipped — a steam process is still running",
        );
        return;
    }

    let stamp = crate::now_ms();
    let mut renamed: Vec<String> = Vec::new();

    // داخل مجلد ستيم (احتمال ثانوي — بعض الإصدارات تضع الكاش هنا)
    rename_stale_items(steam_dir, stamp, &mut renamed);
    rename_stale_items(&steam_dir.join("config"), stamp, &mut renamed);

    // بروفايل مستخدم البottle — المسار الحقيقي لكاش CEF وأقفال Singleton
    if let Some(drive_c) = steam_dir.parent().and_then(|p| p.parent()) {
        let users = drive_c.join("users");
        if let Ok(rd) = std::fs::read_dir(&users) {
            for entry in rd.flatten() {
                let user_dir = entry.path();
                if !user_dir.is_dir() {
                    continue;
                }
                let local = user_dir.join("AppData").join("Local").join("Steam");
                if local.is_dir() {
                    rename_stale_items(&local, stamp, &mut renamed);
                }
            }
        }
    }

    if !renamed.is_empty() {
        crate::debuglog::raw(
            "info",
            "steam",
            &format!(
                "cleaned stale steam state (renamed, recoverable): {}",
                renamed.join(", ")
            ),
        );
    }
}

/// فلاقين إضافية لعميل ستيم نفسه (وليس للـwebhelper) — طوارئ ميدانية
/// بدون إعادة بناء: السطر الثاني في `cef-flags.txt` بصيغة:
///   `steam:-cef-single-process`
/// نمرّرها على قائمة بيضاء معروفة (بلا shell وبلا حقن) — أي شيء ثاني يُتجاهل.
const STEAM_ARG_ALLOW: [&str; 6] = [
    "-no-cef-sandbox",
    "-cef-single-process",
    "-noverifyfiles",
    "-forcedesktopscaling",
    "-skipinitialbootstrap",
    "-norepairfiles",
];

/// يقرأ سطر `steam:` ويرجّع قائمة آمنة من وسائط ستيم
fn parse_steam_args(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut expect_value = false;
    for token in line.split_whitespace().take(24) {
        if token.len() > 40 || !token.is_ascii() {
            continue;
        }
        if expect_value {
            expect_value = false;
            // القيمة المسموحة بعد -forcedesktopscaling: رقم واحد فقط
            if token.len() == 1 && token.chars().all(|c| c.is_ascii_digit()) {
                out.push(token.to_string());
            }
            continue;
        }
        if STEAM_ARG_ALLOW.contains(&token) {
            expect_value = token == "-forcedesktopscaling";
            out.push(token.to_string());
        }
    }
    out.truncate(8);
    out
}

/// وسائط ستيم الإضافية (اختيارية) — تُقرأ من `~/Library/Application Support/Jisr/cef-flags.txt`
pub(crate) fn steam_args_override() -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(crate::data_dir().join("cef-flags.txt")) else {
        return Vec::new();
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("steam:") {
            return parse_steam_args(rest);
        }
    }
    Vec::new()
}

/// ملف اختياري لتجربة فلاقين CEF بدون بناء جديد:
/// `~/Library/Application Support/Jisr/cef-flags.txt` (أول سطر، بحدود آمنة)
pub(crate) fn cef_flags_override() -> Option<String> {
    let text = std::fs::read_to_string(crate::data_dir().join("cef-flags.txt")).ok()?;
    let line = text.lines().next()?.trim().to_string();
    if line.is_empty() || line.len() > 512 {
        return None;
    }
    if !line
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_=., :".contains(c))
    {
        return None;
    }
    Some(line)
}

/// هل الشاشة Retina (2x)؟
///
/// السبب: واجهة ستيم (CEF) داخل Wine ترسم بدقة 1x مهما كانت الشاشة ⇒
/// **تظهر ضبابية** على شاشات Retina. الحل: `-forcedesktopscaling 2` (يعني 2x).
///
/// نستخدم `NSScreen.backingScaleFactor` (الأدق: 2.0 على Retina، 1.0 على غيرها)
/// عبر JXA، ونرجع لـ`system_profiler` لو فشل. **بلا كاش دائم** — كان يخزّن
/// قيمة خاطئة مرة واحدة ويبقى عليها للأبد (سبب بقاء الضبابية عند بعض الأجهزة).
pub(crate) fn retina_display() -> bool {
    if let Some(v) = display_scale_override() {
        return v >= 2;
    }
    // الأدق: مقياس الشاشة الحقيقي من AppKit
    if let Ok(out) = std::process::Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg("ObjC.import('AppKit'); $.NSScreen.mainScreen.backingScaleFactor.js")
        .output()
    {
        if let Ok(text) = String::from_utf8(out.stdout) {
            if let Ok(v) = text.trim().parse::<f64>() {
                if v >= 1.5 {
                    return true;
                }
                if v > 0.0 {
                    return false;
                }
            }
        }
    }
    // احتياط: وصف الشاشة
    let out = std::process::Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .unwrap_or_default();
    out.contains("retina")
}

/// تجاوز يدوي لمقياس الواجهة: `~/Library/Application Support/Jisr/display-scale.txt`
/// (اكتب `1` أو `2`) — لمن عنده شاشة غريبة أو يبي تجربة قيمة مختلفة.
fn display_scale_override() -> Option<u8> {
    let text = std::fs::read_to_string(crate::data_dir().join("display-scale.txt")).ok()?;
    match text.trim() {
        "1" => Some(1),
        "2" => Some(2),
        _ => None,
    }
}

/// معامل تحجيم الواجهة: 2 على شاشات Retina (حاد)، 1 على غيرها
pub(crate) fn desktop_scaling() -> &'static str {
    if retina_display() { "2" } else { "1" }
}

/// (الافتراضي صار داخل الغلاف نفسه `tools/steamwebhelper-shim`:
///  المتصفح يحصل `--disable-gpu --single-process`، والأطفال `--disable-gpu` فقط.
///  المفتاح هنا للتجاوز الاختياري — رسم GPU عبر ANGLE/Vulkan.)

fn set_phase(state: &SteamState, phase: Phase) {
    if let Ok(mut p) = state.phase.lock() {
        *p = phase;
    }
}

/// هل ستيم شغّال الآن؟ (تحتاجه مكتبة ستيم قبل إرسال أمر تشغيل)
pub(crate) fn is_running(state: &SteamState) -> bool {
    state
        .phase
        .lock()
        .map(|p| *p == Phase::Running)
        .unwrap_or(false)
}

/// إصلاحات واجهة ستيم قبل أي تشغيل (الغلاف + HTTP/2). مشتركة بين تشغيل
/// ستيم العادي وتشغيل لعبة من مكتبة ستيم.
pub(crate) fn prepare_steam_ui(app: &AppHandle, steam_dir: &Path) {
    let backup_dir = crate::data_dir().join("steam-cef-backup");
    crate::debuglog::raw(
        "debug",
        "steam",
        &format!(
            "prepare ui: steam_dir={} backup={}",
            steam_dir.display(),
            backup_dir.display()
        ),
    );
    match ensure_cef_shim(steam_dir, &backup_dir) {
        Some(true) => log(app, "info", log_keys::STEAM_UI_FIX, json!({})),
        Some(false) => {}
        None => log(app, "warn", log_keys::STEAM_UI_FIX_FAILED, json!({})),
    }
    ensure_steam_dev_cfg(steam_dir);
}

fn emit(app: &AppHandle) {
    let _ = app.emit(EVENT, ());
}

/// حالة ستيم كاملة للواجهة (من الملفات + الحالة الحية)
/// المثبّت المنزّل ما يُحسب «جاهز» إلا بعد التحقق من بصمته (ثقة صفرية)
#[tauri::command]
pub async fn steam_status(state: State<'_, SteamState>) -> Result<SteamStatus, String> {
    let phase = state.phase.lock().map(|p| *p).unwrap_or(Phase::Idle);
    let exe = steam_exe();

    let installer_ready = match installer_path().filter(|p| p.is_file()) {
        Some(p) => match download::find_asset(INSTALLER_ID) {
            Some(a) => download::sha256_file(&p)
                .await
                .map(|h| h.eq_ignore_ascii_case(a.sha256))
                .unwrap_or(false),
            None => false,
        },
        None => false,
    };

    Ok(SteamStatus {
        installer_ready,
        installed: exe.is_some(),
        steam_exe: exe.map(|(_, win)| win),
        installing: phase == Phase::Installing,
        running: phase == Phase::Running,
        bottle_ready: bottle::bottle_ready(bottle::MAIN_ID),
        ui_gpu: cef_flags_override().is_some(),
        desktop_scale: if retina_display() { 2 } else { 1 },
        last_launch: read_launch_state(),
    })
}

/// تبديل رسم واجهة ستيم:
/// • `false` (الافتراضي): رسم برمجي — الأكثر ثباتاً (مهم مع CEF على Wine)
/// • `true`: رسم عبر ANGLE → Vulkan → MoltenVK → Metal — **أحدّ وأسرع**
///   (يعيد كتابة الفلاقين ⇒ لازم تعيد تشغيل ستيم)
#[tauri::command]
pub fn steam_ui_gpu(enabled: bool) -> Result<bool, String> {
    let path = crate::data_dir().join("cef-flags.txt");
    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("تعذّر تجهيز مجلد الإعدادات: {e}"))?;
        }
        std::fs::write(
            &path,
            "--in-process-gpu --use-angle=vulkan --ignore-gpu-blocklist\n",
        )
        .map_err(|e| format!("تعذّر حفظ إعدادات الرسم: {e}"))?;
    } else {
        let _ = std::fs::remove_file(&path);
    }
    crate::debuglog::raw(
        "info",
        "steam",
        &format!("ui_gpu_rendering = {enabled} (restart steam to apply)"),
    );
    Ok(enabled)
}

/// التحقق النهائي + النسخ داخل القارورة + تشغيل المثبّت (يرجع العملية للمراقبة)
async fn prepare_and_spawn(
    app: &AppHandle,
    wine: &Path,
    src: &Path,
    asset: &download::Asset,
) -> Result<tokio::process::Child, String> {
    // ثقة صفرية: نتحقق من البصمة مرة ثانية قبل أي تشغيل
    match download::sha256_file(src).await {
        Ok(h) if h.eq_ignore_ascii_case(asset.sha256) => {}
        Ok(_) => {
            let msg = "ملف المثبّت تغيّر — ما يطابق البصمة الرسمية. احذفه ونزّله من جديد".to_string();
            log(
                app,
                "error",
                log_keys::STEAM_INSTALL_FAILED,
                json!({ "error": msg }),
            );
            return Err(msg);
        }
        Err(e) => {
            let msg = format!("تعذّر التحقق من المثبّت: {e}");
            log(
                app,
                "error",
                log_keys::STEAM_INSTALL_FAILED,
                json!({ "error": msg }),
            );
            return Err(msg);
        }
    }

    // ننسخ المثبّت داخل القارورة — Wine ما يقدر يشغّل ملفات من نظام الماك مباشرة
    let drop = bottle::bottle_dir(bottle::MAIN_ID)
        .join("drive_c")
        .join(DROP_IN_BOTTLE);
    if let Some(parent) = drop.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            let msg = format!("تعذّر تجهيز مجلد المثبّت: {e}");
            log(
                app,
                "error",
                log_keys::STEAM_INSTALL_FAILED,
                json!({ "error": msg }),
            );
            return Err(msg);
        }
    }
    if let Err(e) = std::fs::copy(src, &drop) {
        let msg = format!("تعذّر نسخ المثبّت داخل القارورة: {e}");
        log(
            app,
            "error",
            log_keys::STEAM_INSTALL_FAILED,
            json!({ "error": msg }),
        );
        return Err(msg);
    }

    let dos = format!("C:\\{}", DROP_IN_BOTTLE.replace('/', "\\"));
    let prefix = bottle::bottle_dir(bottle::MAIN_ID);
    // جذور الشهادات الرسمية من macOS → تحسين TLS داخل Wine/CEF (غير حرج)
    bottle::sync_ca_bundle(&prefix);
    let (out, err) = stdio_to_log();
    let mut cmd = tokio::process::Command::new(wine);
    cmd.env("WINEPREFIX", &prefix);
    bottle::apply_wine_env(&mut cmd, true);
    graphics::apply_env(&mut cmd, true);
    cmd.arg(&dos).stdin(Stdio::null()).stdout(out).stderr(err);

    crate::debuglog::raw(
        "info",
        "steam",
        &format!(
            "spawn installer wine={} exe={} prefix={} sha256={}",
            wine.display(),
            dos,
            prefix.display(),
            asset.sha256
        ),
    );

    cmd.spawn().map_err(|e| {
        let msg = format!("تعذّر تشغيل المثبّت: {e}");
        log(
            app,
            "error",
            log_keys::STEAM_INSTALL_FAILED,
            json!({ "error": msg }),
        );
        msg
    })
}

/// ثبّت ستيم داخل القارورة الرئيسية (المثبّت الرسمي — المستخدم يكمّله بنفسه)
#[tauri::command]
pub async fn steam_install(app: AppHandle, state: State<'_, SteamState>) -> Result<(), String> {
    let _trace = crate::debuglog::trace("steam", "steam_install", "");
    {
        let ph = state.phase.lock().map_err(|_| "خطأ داخلي".to_string())?;
        match *ph {
            Phase::Installing => return Err("التثبيت شغّال أصلاً".into()),
            Phase::Running => return Err("ستيم شغّال الآن — أوقفه ثم ثبّت".into()),
            Phase::Idle => {}
        }
    }

    // على ويندوز ما فيه Wine — نفس نمط الألعاب: خطأ واضح وآمن
    if cfg!(windows) {
        let msg = "تثبيت ستيم يحتاج Wine — وهذي بيئة تطوير ويندوز (التثبيت الحقيقي على الماك)";
        log(
            &app,
            "error",
            log_keys::STEAM_INSTALL_FAILED,
            json!({ "error": msg }),
        );
        return Err(msg.into());
    }

    if !bottle::bottle_ready(bottle::MAIN_ID) {
        return Err("جهّز القارورة الرئيسية أول — من صفحة القوارير".into());
    }
    let Some(wine) = bottle::wine_bin() else {
        return Err("ما لقينا محرك Wine — ثبّت المحرك أول".into());
    };
    let Some(asset) = download::find_asset(INSTALLER_ID) else {
        return Err("خطأ داخلي: مثبّت ستيم مو بالكتالوج".into());
    };
    let Some(src) = installer_path().filter(|p| p.is_file()) else {
        return Err("نزّل مثبّت ستيم أول".into());
    };

    // من هنا أي محاولة ثانية تتوقف (قفل المرحلة قبل العمل الطويل)
    set_phase(&state, Phase::Installing);
    state.stopped.store(false, Ordering::Relaxed);

    let child = match prepare_and_spawn(&app, &wine, &src, &asset).await {
        Ok(c) => c,
        Err(e) => {
            set_phase(&state, Phase::Idle);
            emit(&app);
            return Err(e);
        }
    };

    log(&app, "info", log_keys::STEAM_INSTALLING, json!({}));
    emit(&app);

    // المراقب: ننتظر نهاية المثبّت → نتحقق من ظهور ستيم → نبلّغ الواجهة
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let res = child.wait_with_output().await;
        let code = res.as_ref().ok().and_then(|o| o.status.code());
        let stderr_tail = log_tail(3);

        crate::debuglog::raw(
            "info",
            "steam",
            &format!(
                "installer exit code={:?} | log={}",
                code,
                run_log_path().display()
            ),
        );
        crate::debuglog::raw(
            "debug",
            "steam",
            &format!(
                "installer output tail:\n{}",
                crate::debuglog::tail_file(&run_log_path(), 80, 24_000)
            ),
        );

        let stopped = match app2.try_state::<SteamState>() {
            Some(st) => {
                let s = st.stopped.load(Ordering::Relaxed);
                set_phase(&st, Phase::Idle);
                st.stopped.store(false, Ordering::Relaxed);
                s
            }
            None => false,
        };

        let installed = steam_exe().is_some();
        if stopped {
            log(&app2, "warn", log_keys::STEAM_INSTALL_STOPPED, json!({}));
        } else if installed {
            log(&app2, "info", log_keys::STEAM_INSTALLED, json!({}));
        } else if code.unwrap_or(0) != 0 {
            let err = if stderr_tail.is_empty() {
                format!("رمز {}", code.unwrap_or(-1))
            } else {
                format!("رمز {} — {}", code.unwrap_or(-1), stderr_tail)
            };
            log(
                &app2,
                "error",
                log_keys::STEAM_INSTALL_FAILED,
                json!({ "error": err }),
            );
        } else {
            log(&app2, "warn", log_keys::STEAM_INSTALL_EXITED, json!({}));
        }

        // نظافة: نمسح المثبّت من داخل القارورة (انتهى دوره)
        let _ = std::fs::remove_file(
            bottle::bottle_dir(bottle::MAIN_ID)
                .join("drive_c")
                .join(DROP_IN_BOTTLE),
        );
        emit(&app2);
    });

    Ok(())
}

/// عمليات steam.exe الشغّالة الآن (لو فيه ستيم من جلسة سابقة)
fn steam_pids() -> Vec<u32> {
    let Ok(out) = std::process::Command::new("pgrep")
        .arg("-f")
        .arg("steam.exe")
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// شغّل ستيم داخل القارورة الرئيسية
#[tauri::command]
pub async fn steam_run(app: AppHandle, state: State<'_, SteamState>) -> Result<(), String> {
    let _trace = crate::debuglog::trace("steam", "steam_run", "");
    // على ويندوز ما فيه Wine — خطأ واضح وآمن
    if cfg!(windows) {
        let msg = "تشغيل ستيم يحتاج Wine — وهذي بيئة تطوير ويندوز (التشغيل الحقيقي على الماك)";
        log(
            &app,
            "error",
            log_keys::STEAM_LAUNCH_FAILED,
            json!({ "error": msg }),
        );
        return Err(msg.into());
    }
    if !bottle::bottle_ready(bottle::MAIN_ID) {
        return Err("جهّز القارورة الرئيسية أول — من صفحة القوارير".into());
    }
    let Some(wine) = bottle::wine_bin() else {
        return Err("ما لقينا محرك Wine — ثبّت المحرك أول".into());
    };
    let Some((mac_exe, dos_exe)) = steam_exe() else {
        return Err("ستيم مو مثبّت بعد — ثبّته أول".into());
    };

    let prefix = bottle::bottle_dir(bottle::MAIN_ID);
    bottle::sync_ca_bundle(&prefix);

    // ستيم شغّال أصلاً (من جلسة سابقة أو نافذة تانية)؟ نتبنّاه بدل ما نفتح
    // نسخة ثانية — وهذا كان سبب «نافذتين ستيم» لما التطبيق ينسى الحالة.
    if !steam_pids().is_empty() {
        set_phase(&state, Phase::Running);
        state.stopped.store(false, Ordering::Relaxed);
        emit(&app);
        crate::debuglog::raw(
            "info",
            "steam",
            "steam already running — adopting instead of spawning",
        );
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                if steam_pids().is_empty() {
                    if let Some(st) = app2.try_state::<SteamState>() {
                        set_phase(&st, Phase::Idle);
                    }
                    emit(&app2);
                    break;
                }
            }
        });
        return Ok(());
    }

    // قفل التشغيل: نتحقق أولاً، ثم نثبّت الغلاف/الإعدادات ونشغّل — بلا await
    let child = {
        let mut ph = state.phase.lock().map_err(|_| "خطأ داخلي".to_string())?;
        if *ph != Phase::Idle {
            return Err("ستيم شغّال أصلاً".into());
        }

        // إصلاح واجهة ستيم: نضمن غلاف CEF قبل كل تشغيل (تحديث ستيم يرجع الأصل)
        // + إصلاح تنزيلات ستيم (HTTP/2) — ملف رسمي داخل مجلد ستيم
        if let Some(dir) = mac_exe.parent() {
            // بقايا الكاش والأقفال القديمة — سبب موثّق لـ«ستيم يفتح بدون نافذة»
            cleanup_stale_steam_state(dir);
            prepare_steam_ui(&app, dir);
        }

        let (out, err) = stdio_to_log();
        let mut cmd = tokio::process::Command::new(&wine);
        cmd.env("WINEPREFIX", &prefix);
        bottle::apply_wine_env(&mut cmd, true);
        graphics::apply_env(&mut cmd, true);
        // فلاقين CEF: الغلاف نفسه يطبّق الافتراضي حسب دور العملية.
        // نمرّر تجاوز المستخدم فقط لمّا يفعّل «رسم GPU» من الواجهة.
        if let Some(flags) = cef_flags_override() {
            cmd.env("JISR_CEF_FLAGS", flags);
        }
        cmd.arg(&dos_exe)
            // معيار ستيم على Wine: بند Chromium الداخلي ما يشتغل صح تحت Wine
            .arg("-no-cef-sandbox")
            // ما نخلي مدقق الملفات يرجّع steamwebhelper.exe الأصلي فوق غلافنا
            .arg("-noverifyfiles")
            // مقياس واجهة صحيح: 2x على شاشات Retina (sharp) و1x على غيرها
            .arg("-forcedesktopscaling")
            .arg(desktop_scaling());
        // وسائط طوارئ اختيارية من الملف النصي (قائمة بيضاء فقط)
        let extra = steam_args_override();
        for a in &extra {
            cmd.arg(a);
        }
        cmd.stdin(Stdio::null()).stdout(out).stderr(err);
        if let Some(dir) = mac_exe.parent() {
            cmd.current_dir(dir);
        }

        crate::debuglog::raw(
            "info",
            "steam",
            &format!(
                "spawn steam wine={} exe={} prefix={} args=-no-cef-sandbox -noverifyfiles -forcedesktopscaling {}{} graphics={}",
                wine.display(),
                dos_exe,
                prefix.display(),
                desktop_scaling(),
                if extra.is_empty() {
                    String::new()
                } else {
                    format!(" extra={extra:?}")
                },
                graphics::active_layer()
            ),
        );

        let child = cmd.spawn().map_err(|e| {
            let msg = format!("تعذّر تشغيل ستيم: {e}");
            log(
                &app,
                "error",
                log_keys::STEAM_LAUNCH_FAILED,
                json!({ "error": msg }),
            );
            msg
        })?;
        *ph = Phase::Running;
        child
    };

    state.stopped.store(false, Ordering::Relaxed);
    let started = crate::now_ms();
    log(&app, "info", log_keys::STEAM_LAUNCHING, json!({}));
    emit(&app);

    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let res = child.wait_with_output().await;
        let played = crate::now_ms().saturating_sub(started);
        let code = res.as_ref().ok().and_then(|o| o.status.code());
        let stderr_tail = log_tail(3);

        crate::debuglog::raw(
            "info",
            "steam",
            &format!(
                "steam exit code={:?} played_ms={} | log={}",
                code,
                played,
                run_log_path().display()
            ),
        );
        crate::debuglog::raw(
            "debug",
            "steam",
            &format!(
                "steam output tail:\n{}",
                crate::debuglog::tail_file(&run_log_path(), 120, 32_000)
            ),
        );

        let stopped = match app2.try_state::<SteamState>() {
            Some(st) => {
                let s = st.stopped.load(Ordering::Relaxed);
                set_phase(&st, Phase::Idle);
                st.stopped.store(false, Ordering::Relaxed);
                s
            }
            None => false,
        };

        let quick_fail = !stopped && code.unwrap_or(0) != 0 && played < 5_000;
        if stopped {
            log(&app2, "info", log_keys::STEAM_STOPPED, json!({}));
        } else if quick_fail {
            let err = if stderr_tail.is_empty() {
                format!("رمز {}", code.unwrap_or(-1))
            } else {
                format!("رمز {} — {}", code.unwrap_or(-1), stderr_tail)
            };
            log(
                &app2,
                "error",
                log_keys::STEAM_LAUNCH_FAILED,
                json!({ "error": err }),
            );
        } else {
            log(&app2, "info", log_keys::STEAM_EXITED, json!({}));
        }
        emit(&app2);
    });

    Ok(())
}

/// أوقف ستيم (أو التثبيت الجاري) — نكيل جلسة القارورة بأمان (wineserver -k)
#[tauri::command]
pub async fn steam_stop(state: State<'_, SteamState>) -> Result<(), String> {
    let _trace = crate::debuglog::trace("steam", "steam_stop", "");
    {
        let ph = state.phase.lock().map_err(|_| "خطأ داخلي".to_string())?;
        if *ph == Phase::Idle {
            return Err("ستيم مو شغّال".into());
        }
    }
    state.stopped.store(true, Ordering::Relaxed);

    if cfg!(unix) {
        if let Some(wine) = bottle::wine_bin() {
            if let Some(wineserver) = wine.parent().map(|p| p.join("wineserver")) {
                let mut c = tokio::process::Command::new(&wineserver);
                c.env("WINEPREFIX", bottle::bottle_dir(bottle::MAIN_ID))
                    .arg("-k")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _ = tokio::time::timeout(Duration::from_secs(15), c.status()).await;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "jisr-test-{tag}-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn cef_dir(root: &Path) -> PathBuf {
        let d = root.join("Steam").join("bin").join("cef").join("cef.win64");
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_valve(path: &Path) {
        let mut data = vec![0x41u8; 1_100_000];
        data[0] = b'M';
        data[1] = b'Z';
        fs::write(path, data).unwrap();
    }

    #[test]
    fn install_is_atomic_and_idempotent() {
        let root = temp_root("install");
        let cef = cef_dir(&root);
        fake_valve(&cef.join("steamwebhelper.exe"));
        let backup = root.join("backup");
        let steam = root.join("Steam");

        assert_eq!(ensure_cef_shim(&steam, &backup), Some(true));
        assert!(is_cef_shim(&cef.join("steamwebhelper.exe")));
        assert!(looks_like_real_helper(&cef.join("steamwebhelper_real.exe")));
        assert!(looks_like_real_helper(&backup.join("steamwebhelper.exe")));
        assert_eq!(ensure_cef_shim(&steam, &backup), Some(false));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restores_missing_or_corrupt_real_helper() {
        let root = temp_root("restore");
        let cef = cef_dir(&root);
        let backup = root.join("backup");
        let steam = root.join("Steam");
        fake_valve(&cef.join("steamwebhelper.exe"));
        assert_eq!(ensure_cef_shim(&steam, &backup), Some(true));

        let real = cef.join("steamwebhelper_real.exe");
        fs::remove_file(&real).unwrap();
        assert_eq!(ensure_cef_shim(&steam, &backup), Some(false));
        assert!(looks_like_real_helper(&real));

        fs::write(&real, b"broken").unwrap();
        assert_eq!(ensure_cef_shim(&steam, &backup), Some(false));
        assert!(looks_like_real_helper(&real));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_loop_when_both_files_are_shims() {
        let root = temp_root("recursion");
        let cef = cef_dir(&root);
        fs::write(cef.join("steamwebhelper.exe"), CEF_SHIM).unwrap();
        fs::write(cef.join("steamwebhelper_real.exe"), CEF_SHIM).unwrap();
        assert_eq!(
            ensure_cef_shim(&root.join("Steam"), &root.join("backup")),
            None
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn steam_args_whitelist_keeps_only_known_flags() {
        assert_eq!(
            parse_steam_args("-cef-single-process -no-cef-sandbox"),
            vec![
                "-cef-single-process".to_string(),
                "-no-cef-sandbox".to_string()
            ]
        );
        // أي شيء غير معروف يُتجاهل — ما فيه تمرير أوامر عشوائية
        assert!(parse_steam_args("-rm -rf / ; whoami").is_empty());
        // القيمة بعد -forcedesktopscaling: رقم واحد فقط
        assert_eq!(
            parse_steam_args("-forcedesktopscaling 2"),
            vec!["-forcedesktopscaling".to_string(), "2".to_string()]
        );
        assert_eq!(parse_steam_args("-forcedesktopscaling abc").len(), 1);
    }

    #[test]
    fn cleanup_renames_stale_cache_instead_of_deleting() {
        let root = temp_root("cleanup");
        // بروفايل مستخدم البottle: drive_c/users/<user>/AppData/Local/Steam
        let drive_c = root.join("drive_c");
        let steam = drive_c.join("Program Files (x86)").join("Steam");
        let user_local = drive_c
            .join("users")
            .join("tester")
            .join("AppData")
            .join("Local")
            .join("Steam");
        fs::create_dir_all(user_local.join("htmlcache")).unwrap();
        fs::create_dir_all(steam.join("config").join("htmlcache")).unwrap();
        fs::write(steam.join("SingletonLock"), b"x").unwrap();
        fs::write(user_local.join("SingletonCookie"), b"y").unwrap();
        // ملف ما له علاقة — لازم ما ينمس
        fs::write(steam.join("config").join("config.vdf"), b"z").unwrap();

        cleanup_stale_steam_state_with(&steam, false);

        // الأصول ما عادت موجودة (أُعيدت تسميتها) وما حذفنا شي
        assert!(!user_local.join("htmlcache").exists());
        assert!(!steam.join("config").join("htmlcache").exists());
        assert!(!steam.join("SingletonLock").exists());
        assert!(!user_local.join("SingletonCookie").exists());
        assert!(steam.join("config").join("config.vdf").exists());

        // النسخ الاحتياطية موجودة (قابلة للتراجع)
        let backups: Vec<PathBuf> = fs::read_dir(&user_local)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("jisr-bak")
            })
            .collect();
        assert!(!backups.is_empty(), "expected a recoverable backup copy");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dev_cfg_keeps_user_lines_and_avoids_duplicates() {
        let root = temp_root("cfg");
        fs::write(root.join("steam_dev.cfg"), "user_setting 1\n").unwrap();
        ensure_steam_dev_cfg(&root);
        ensure_steam_dev_cfg(&root);
        let text = fs::read_to_string(root.join("steam_dev.cfg")).unwrap();
        assert!(text.contains("user_setting 1"));
        assert_eq!(
            text.matches("@nClientDownloadEnableHTTP2PlatformWindows 0")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    /// حارس سلسلة التوريد: الغلاف المدمج (ملف Windows ثنائي) مثبّت ببصمة.
    /// أي تغيير غير مقصود في assets/steamwebhelper.exe يفشل الاختبار.
    #[test]
    fn cef_shim_hash_is_pinned() {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(CEF_SHIM);
        let got = format!("{:x}", h.finalize());
        assert_eq!(
            got, "437203bad808f4038c2cb8425dafd99471b8b12f3a3d8425eb6e4417e5925b59",
            "تغيّر غلاف CEF — أعد بناءه من tools/steamwebhelper-shim ثم حدّث البصمة هنا"
        );
    }
}
