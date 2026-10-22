// ============================================================
//  جسر — مكتبة الألعاب + التشغيل
//  • إضافة لعبة: ملف .exe من أي مكان — تُحفظ في games.json (كتابة ذرّية)
//  • التشغيل: نربط «قرصاً مخصصاً» لمجلد اللعبة فقط (البديل الآمن عن Z:)
//    ثم Wine يشغّل الملف داخل القارورة — بلا shell، args مباشرة
//  • تتبع حي: شغالة/انتهت + مدة اللعب تُجمع تلقائياً + إيقاف آمن (wineserver -k)
//  • ملاحظة صادقة: التشغيل الحقيقي على الماك — على ويندوز (بيئة التطوير)
//    نجرّب كل المسار ونعطي خطأ واضح وآمن فقط
// ============================================================
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{bottle, data_dir, graphics, log, log_keys, runner};

/// تغيّرت قائمة الألعاب (أضفنا/حذفنا/بدأت/انتهت)
const EVENT: &str = "jisr-games";
/// لعبة انتهت — حمولة كاملة للتنبيه
const EVENT_EXIT: &str = "jisr-game-exit";

/// أقصى عدد إعادات تشغيل تلقائية بعد انهيار مُصنَّف.
/// ثلاثة تكفي لتغطية المسارات المنطقية (DXVK↔WineD3D، مترجم أصلي↔مدمج،
/// renderer) وبعدها أي محاولة إضافية تكون تخميناً بلا معلومة جديدة.
const MAX_AUTO_ATTEMPTS: u8 = 3;

/// لعبة محفوظة
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct GameMeta {
    pub id: String,
    pub name: String,
    pub kind: String,   // exe | steam (ستيم: مرحلة لاحقة)
    pub exe: String,    // مسار ملف التشغيل على الماك
    pub dir: String,    // مجلد اللعبة (مكان التشغيل)
    pub bottle: String, // أي قارورة — "main" افتراضياً
    pub args: String,   // وسائط إضافية (مستقبلاً: شاشة كاملة...)
    pub played_ms: u64, // مجموع وقت اللعب
    pub added_ms: u64,
    /// تركيبة الطبقات المحفوظة لهذه اللعبة (combo::GameProfile كـJSON).
    /// تُحدَّث تلقائياً بعد كل انهيار مُصنَّف — الحلقة المغلقة تحفظ التعلّم.
    #[serde(default)]
    pub profile: Option<serde_json::Value>,
}

/// نسخة اللعبة اللي تروح للواجهة
#[derive(Serialize, Clone)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub exe: String,
    pub dir: String,
    pub bottle: String,
    pub played_ms: u64,
    pub added_ms: u64,
    pub running: bool,
}

/// معلومات لعبة شغّالة الآن
struct RunInfo {
    stop_requested: bool,
}

#[derive(Default)]
pub struct GamesState {
    list: Mutex<Vec<GameMeta>>,
    run: Mutex<HashMap<String, RunInfo>>,
}

// ============================================================
//  الحفظ — games.json (كتابة ذرّية: tmp ثم تبديل)
// ============================================================
fn games_path() -> PathBuf {
    data_dir().join("games.json")
}

fn load_games() -> Vec<GameMeta> {
    std::fs::read_to_string(games_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_games(list: &[GameMeta]) -> Result<(), String> {
    let path = games_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("تعذّر تجهيز مجلد الحفظ: {e}"))?;
    }
    let text =
        serde_json::to_string_pretty(list).map_err(|e| format!("تعذّر تجهيز البيانات: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("تعذّر الحفظ: {e}"))?;
    if std::fs::rename(&tmp, &path).is_err() {
        // ويندوز ما يستبدل ملفاً موجوداً بالتبديل — نمسح ثم نبدّل
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path).map_err(|e| format!("تعذّر إتمام الحفظ: {e}"))?;
    }
    Ok(())
}

/// يُحمّل الألعاب المحفوظة عند بدء التطبيق
pub fn init(app: &AppHandle) {
    if let Some(st) = app.try_state::<GamesState>() {
        if let Ok(mut list) = st.list.lock() {
            *list = load_games();
        }
    }
}

fn to_game(m: &GameMeta, running: bool) -> Game {
    Game {
        id: m.id.clone(),
        name: m.name.clone(),
        kind: m.kind.clone(),
        exe: m.exe.clone(),
        dir: m.dir.clone(),
        bottle: m.bottle.clone(),
        played_ms: m.played_ms,
        added_ms: m.added_ms,
        running,
    }
}

/// مدة لطيفة للسجل: 12:34 أو 1:02:03
fn fmt_dur(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

/// ملف تسجيل كلام اللعبة الكامل (stdout + stderr) لآخر تشغيل — لكل لعبة ملفها —
/// تشخيص ميداني بلا نمو ذاكرة على الجلسات الطويلة (نفس أسلوب ستيم)
fn run_log_path(id: &str) -> PathBuf {
    data_dir().join("logs").join(format!("run-{id}.log"))
}

/// نوجّه كلام اللعبة كامل لملف (بدل الأنابيب اللي تضيع عند الإيقاف اليدوي)
fn stdio_to_log(id: &str) -> (Stdio, Stdio) {
    let path = run_log_path(id);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::File::create(&path).and_then(|f| f.try_clone().map(|c| (f, c))) {
        Ok((out, err)) => (Stdio::from(out), Stdio::from(err)),
        Err(_) => (Stdio::null(), Stdio::null()),
    }
}

/// آخر سطور من كلام اللعبة — تُضاف لرسائل الفشل السريع
fn log_tail(id: &str, n: usize) -> String {
    std::fs::read(run_log_path(id))
        .map(|b| crate::tail_lines(&b, n))
        .unwrap_or_default()
}

// ============================================================
//  المسارات — البديل الآمن عن Z:
// ============================================================

/// هل الملف داخل قارورة؟ يرجع مسار ويندوز C:\... مباشرة (بلا أي ربط إضافي)
/// نفحص **قارورة اللعبة نفسها فقط** — ملف في قارورة ثانية ما ينفع لمساره C:
fn path_in_bottle(exe: &Path, bottle_id: &str) -> Option<String> {
    let exe_c = std::fs::canonicalize(exe).ok()?;
    let dc = std::fs::canonicalize(bottle::bottle_dir(bottle_id).join("drive_c")).ok()?;
    let rel = exe_c.strip_prefix(&dc).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    Some(format!("C:\\{}", parts.join("\\")))
}

/// يضمن «قرصاً مخصصاً» لمجلد اللعبة فقط داخل القارورة (لا وصول لبقية الماك)
/// يرجع حرف القرص مثل "d:" — يُنشأ مرة ويُعاد استخدامه
fn ensure_game_drive(bottle_id: &str, game_dir: &Path) -> Result<String, String> {
    if cfg!(windows) {
        // على ويندوز ما فيه روابط قرص ولا Wine أصلاً — التشغيل الحقيقي على الماك
        return Err(
            "تشغيل الألعاب يحتاج Wine — وهذي بيئة تطوير ويندوز (التشغيل الحقيقي على الماك)".into(),
        );
    }

    let dev = bottle::bottle_dir(bottle_id).join("dosdevices");
    if !dev.is_dir() {
        return Err("القارورة مو جاهزة بعد — جهّزها من صفحة القوارير".into());
    }
    let want = std::fs::canonicalize(game_dir).map_err(|_| "مجلد اللعبة مو موجود".to_string())?;

    // موجود مسبقاً؟ (حرف يشير لنفس المجلد) → نعيد استخدامه بلا تكرار
    let mut used: Vec<u8> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dev) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let b = name.as_bytes();
            if b.len() == 2 && b[1] == b':' && b[0].is_ascii_lowercase() {
                used.push(b[0]);
                if std::fs::canonicalize(e.path())
                    .map(|t| t == want)
                    .unwrap_or(false)
                {
                    return Ok(name);
                }
            }
        }
    }

    // أول حرف فاضي (نتفادى a:/b:/c:/z: المحجوزة)
    for letter in b'd'..=b'y' {
        if used.contains(&letter) {
            continue;
        }
        #[cfg(unix)]
        {
            let link = dev.join(format!("{}:", letter as char));
            std::os::unix::fs::symlink(&want, &link)
                .map_err(|e| format!("تعذّر ربط مجلد اللعبة بأمان: {e}"))?;
        }
        return Ok(format!("{}:", letter as char));
    }
    Err("ما فيه حرف قرص فاضي".into())
}

/// مسار التشغيل داخل ويندوز + مجلد التشغيل على الماك
fn game_dos_paths(meta: &GameMeta) -> Result<(String, PathBuf), String> {
    let exe = PathBuf::from(&meta.exe);
    let dir = PathBuf::from(&meta.dir);
    if !exe.is_file() {
        return Err("ملف اللعبة مو موجود — يمكن اننقل أو انحذف".into());
    }
    // داخل قارورة؟ → C: مباشرة (بلا ربط)
    if let Some(win) = path_in_bottle(&exe, &meta.bottle) {
        return Ok((win, dir));
    }
    // برا القارورة → قرص مخصص لمجلد اللعبة فقط
    let letter = ensure_game_drive(&meta.bottle, &dir)?;
    let fname = exe
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok((format!("{letter}\\{fname}"), dir))
}

// ============================================================
//  أوامر Tauri
// ============================================================

/// كل الألعاب (مع حالة «شغالة الآن» ووقت اللعب)
#[tauri::command]
pub async fn games_list(state: State<'_, GamesState>) -> Result<Vec<Game>, String> {
    let running: HashSet<String> = state
        .run
        .lock()
        .map(|r| r.keys().cloned().collect())
        .unwrap_or_default();
    let list = state.list.lock().map(|l| l.clone()).unwrap_or_default();
    Ok(list
        .iter()
        .map(|m| to_game(m, running.contains(&m.id)))
        .collect())
}

/// إضافة لعبة من ملف .exe (المسار يجي من نافذة الاختيار الأصلية)
#[tauri::command]
pub async fn game_add(
    app: AppHandle,
    state: State<'_, GamesState>,
    exe: String,
) -> Result<Game, String> {
    let _trace = crate::debuglog::trace("game", "game_add", format!("exe={exe}"));
    let path = PathBuf::from(exe.trim());
    if !path.is_absolute() {
        return Err("اختر الملف من نافذة الاختيار — المسار غير صالح".into());
    }
    if !path.is_file() {
        return Err("ما لقينا الملف — جرّب تختاره مرة ثانية".into());
    }
    if !path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
    {
        return Err("نحتاج ملف تشغيل ويندوز (.exe) — اختر مثل game.exe".into());
    }

    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "اسم الملف غير صالح".to_string())?;
    let dir = path
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "مجلد غير صالح".to_string())?;

    let meta = GameMeta {
        id: format!("g-{}", crate::now_ms()),
        name,
        kind: "exe".into(),
        exe: path.to_string_lossy().to_string(),
        dir: dir.to_string_lossy().to_string(),
        bottle: bottle::MAIN_ID.to_string(),
        args: String::new(),
        played_ms: 0,
        added_ms: crate::now_ms(),
        profile: None,
    };

    let snapshot = {
        let mut list = state.list.lock().map_err(|_| "خطأ داخلي".to_string())?;
        if list.iter().any(|g| g.exe == meta.exe) {
            return Err("هذي اللعبة مضافة مسبقاً".into());
        }
        list.push(meta.clone());
        list.clone()
    };
    save_games(&snapshot)?;

    log(
        &app,
        "info",
        log_keys::GAME_ADDED,
        json!({ "name": meta.name }),
    );
    let _ = app.emit(EVENT, ());
    Ok(to_game(&meta, false))
}

/// حذف لعبة من المكتبة (ملفات اللعبة نفسها ما تُمس — بلا مفاجآت)
#[tauri::command]
pub async fn game_remove(
    app: AppHandle,
    state: State<'_, GamesState>,
    id: String,
) -> Result<(), String> {
    let _trace = crate::debuglog::trace("game", "game_remove", format!("id={id}"));
    if state
        .run
        .lock()
        .map(|r| r.contains_key(&id))
        .unwrap_or(false)
    {
        return Err("أوقف اللعبة أول ثم احذفها".into());
    }

    let (snapshot, name) = {
        let mut list = state.list.lock().map_err(|_| "خطأ داخلي".to_string())?;
        let Some(pos) = list.iter().position(|g| g.id == id) else {
            return Err("اللعبة مو موجودة".into());
        };
        let removed = list.remove(pos);
        (list.clone(), removed.name)
    };
    save_games(&snapshot)?;

    log(
        &app,
        "warn",
        log_keys::GAME_REMOVED,
        json!({ "name": name }),
    );
    let _ = app.emit(EVENT, ());
    Ok(())
}

/// يحفظ ملف التعريف داخل `games.json` نفسه — اللعبة وقرارها في مكان واحد،
/// بلا ملفات جانبية تتفرّق أو تُحذف وحدها.
fn persist_profile(app: &AppHandle, id: &str, profile: &crate::combo::GameProfile) {
    let Some(st) = app.try_state::<GamesState>() else {
        return;
    };
    let Ok(mut list) = st.list.lock() else {
        return;
    };
    let Some(game) = list.iter_mut().find(|g| g.id == id) else {
        return;
    };
    game.profile = Some(profile.to_json());
    let snapshot = list.clone();
    drop(list);
    let _ = save_games(&snapshot);
}

/// يعيد التشغيل في مهمة مستقلة.
///
/// الانتظار المباشر لدالة `launch` من داخل المهمة التي أنشأتها يجعل نوع
/// الـfuture يشير إلى نفسه (دورة في الاستدلال + شرط `Send`). مهمة جديدة
/// تقطع الدورة: نوع ما يُرجَع هنا `JoinHandle<()>` محدَّد سلفاً.
fn spawn_retry(app: AppHandle, id: String, attempt: u8) {
    tauri::async_runtime::spawn(async move {
        let _ = launch(app, id, attempt).await;
    });
}

/// تشغيل لعبة داخل قارورتها — نرجع فوراً والمراقبة تكمل بالخلفية
#[tauri::command]
pub async fn game_launch(
    app: AppHandle,
    _state: State<'_, GamesState>,
    id: String,
) -> Result<(), String> {
    launch(app, id, 0).await
}

/// المحاولة رقم `attempt` من تشغيل لعبة (0 = تشغيل المستخدم الأول).
///
/// كل محاولة تعيد قراءة ملف التعريف من القرص، فالحلقة المغلقة تبدأ من
/// آخر تركيبة تعلّمناها لا من الصفر.
pub(crate) async fn launch(app: AppHandle, id: String, attempt: u8) -> Result<(), String> {
    let state = app.state::<GamesState>();
    let _trace = crate::debuglog::trace("game", "game_launch", format!("id={id}"));
    let meta = {
        let list = state.list.lock().map_err(|_| "خطأ داخلي".to_string())?;
        list.iter()
            .find(|g| g.id == id)
            .cloned()
            .ok_or_else(|| "اللعبة مو موجودة".to_string())?
    };

    {
        let run = state.run.lock().map_err(|_| "خطأ داخلي".to_string())?;
        if run.contains_key(&id) {
            return Err("اللعبة شغّالة أصلاً".into());
        }
    }

    // دفاعياً: معرّف القارورة يُستخدم في مسار على القرص — نفس حراسة bottle.rs
    if !bottle::valid_bottle_id(&meta.bottle) {
        return Err("معرّف القارورة غير صالح".into());
    }

    if !bottle::bottle_ready(&meta.bottle) {
        return Err("جهّز القارورة الرئيسية أول — من صفحة القوارير".into());
    }

    let wine =
        bottle::wine_bin().ok_or_else(|| "ما لقينا محرك Wine — ثبّت المحرك أول".to_string())?;
    let (dos_exe, cwd) = game_dos_paths(&meta).map_err(|e| {
        log(
            &app,
            "error",
            log_keys::GAME_LAUNCH_FAILED,
            json!({ "name": meta.name, "error": e }),
        );
        e
    })?;

    // نبني الأمر — بلا shell إطلاقاً (args مباشرة، ما فيه حقن)
    let prefix = bottle::bottle_dir(&meta.bottle);
    bottle::sync_ca_bundle(&prefix);
    // نشر طبقة DXVK داخل القارورة (لازم قبل التشغيل: بدونها Wine يتجاهل DXVK)
    let _ = graphics::deploy_to_prefix(&prefix);
    // D3DX9 الأصلي + الحارس داخل القارورة: ألعاب DX9 القديمة تستدعي
    // `d3dx9_43.dll` بالاسم. بلا نشر الملفات، Wine يرجّع نسخته المدمجة
    // الناقصة وتنهار اللعبة عند الإقلاع قبل أن ترسم أول إطار.
    let d3dx_files = crate::d3dx::deploy_files(&prefix).is_ok();

    // ============================================================
    //  خط الأنابيب الكامل: فحص بايتات اللعبة → خطة الطبقات → تطبيق
    //  القرار من محتوى الملف لا من اسمه، وكل طبقة تُفعَّل فقط لو
    //  ملفاتها موجودة فعلاً — نفس منهج محرك التوافق التجاري.
    // ============================================================
    let info = runner::probe(Path::new(&meta.exe));
    let profile = {
        let mut loaded = meta
            .profile
            .as_ref()
            .and_then(|v| crate::combo::GameProfile::from_json_for(0, v))
            .unwrap_or_else(|| runner::initial_profile(info.as_ref()));
        // مفتاح ثابت لهذه اللعبة: علامة الطبقة والتجاوزات الخاصة بالملف
        // تُكتب لكل لعبة، لا لكل قارورة.
        loaded.appid = runner::appid_for(&id);
        loaded
    };
    let plan = runner::plan(&profile, info.as_ref());
    // الموانع تُسجَّل ولا تمنع: نفس سلوك محرك التوافق التجاري — المستخدم
    // يجرّب، والتقرير يقول له الحقيقة بدل ما نقفل الزر من البداية.
    if !plan.is_launchable() {
        let why: Vec<String> = plan
            .blockers
            .iter()
            .map(|b| b.label_ar().to_string())
            .collect();
        crate::debuglog::raw(
            "warn",
            "game",
            &format!("expected blockers for {}: {}", meta.name, why.join(" · ")),
        );
    }
    if let Some(info) = &info {
        crate::debuglog::raw(
            "info",
            "game",
            &format!("probe {} | {}", meta.exe, info.summary()),
        );
    }
    let layers: Vec<&str> = plan.layers.iter().map(|l| l.as_str()).collect();
    let actions: Vec<String> = plan
        .actions
        .iter()
        .map(|a| match a.arg() {
            Some(arg) => format!("{}:{}", a.kind(), arg),
            None => a.kind().to_string(),
        })
        .collect();
    crate::debuglog::raw(
        "info",
        "game",
        &format!(
            "plan attempt={attempt} layers=[{}] actions=[{}] profile={}",
            layers.join(","),
            actions.join(","),
            profile.describe_ar()
        ),
    );
    if !d3dx_files {
        crate::debuglog::raw(
            "warn",
            "game",
            "d3dx9 files not deployed — DX9 titles will fall back to Wine's incomplete builtin",
        );
    }
    if let Err(e) = runner::apply(&wine, &prefix, &profile, Some(&dos_exe)).await {
        crate::debuglog::raw("warn", "game", &format!("profile apply: {e}"));
    }

    // محرّك الرسوميات: إعداد القارورة + قواعد لكل لعبة
    let mut backend = crate::graphics_backend::read_backend(&prefix);
    let rules = crate::compat_rules::RulesDb::load_dir(&crate::data_dir().join("rules"))
        .unwrap_or_default();
    let resolution = rules.resolve(&crate::compat_rules::Context {
        steam_game_id: meta.id.parse::<u32>().ok(),
        exe_path: Some(dos_exe.clone()),
        product: Some("jisr".to_string()),
        platform: Some("Mac".to_string()),
        cxversion: None,
    });
    if let Some(forced) = resolution.backend {
        backend = forced;
    }
    let _ = crate::graphics_backend::apply(&wine, &prefix, backend).await;
    if !resolution.dll_overrides.is_empty() {
        let _ =
            crate::graphics_backend::apply_dll_overrides(&wine, &prefix, &resolution.dll_overrides)
                .await;
    }
    let (out, err) = stdio_to_log(&meta.id);
    let mut cmd = tokio::process::Command::new(&wine);
    cmd.env("WINEPREFIX", &prefix);
    bottle::apply_wine_env(&mut cmd, false);
    // DXVK (إن كان مفعّلاً): DirectX 10/11 → Vulkan → Metal بدل WineD3D
    graphics::apply_env(&mut cmd, false);
    // متغيّرات محرّك الرسوميات المختار + أي متغيّرات من قواعد اللعبة
    for (key, value) in crate::graphics_backend::env(backend) {
        cmd.env(key, value);
    }
    for (key, value) in &resolution.env {
        cmd.env(key, value);
    }
    // تجاوزات الطبقات من ملف التعريف (native/builtin لكل DLL)
    for (key, value) in runner::env(&profile) {
        cmd.env(key, value);
    }
    cmd.arg(&dos_exe)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    for a in meta.args.split_whitespace() {
        cmd.arg(a);
    }

    crate::debuglog::raw(
        "info",
        "game",
        &format!(
            "spawn wine={} exe={} prefix={} args={:?} graphics={} backend={}",
            wine.display(),
            dos_exe,
            prefix.display(),
            meta.args,
            graphics::active_layer(),
            backend.as_str()
        ),
    );

    let child = cmd.spawn().map_err(|e| {
        let msg = format!("تعذّر تشغيل اللعبة: {e}");
        log(
            &app,
            "error",
            log_keys::GAME_LAUNCH_FAILED,
            json!({ "name": meta.name, "error": msg }),
        );
        msg
    })?;

    let started = crate::now_ms();
    // نثبّت التركيبة المستخدمة حتى لو انهارت اللعبة في الثانية الأولى —
    // الحلقة المغلقة تحتاج نقطة بداية محفوظة.
    persist_profile(&app, &id, &profile);
    if let Ok(mut run) = state.run.lock() {
        run.insert(
            id.clone(),
            RunInfo {
                stop_requested: false,
            },
        );
    }
    log(
        &app,
        "info",
        log_keys::GAME_LAUNCHING,
        json!({ "name": meta.name }),
    );
    let _ = app.emit(EVENT, ());

    // المراقب: ننتظر نهاية اللعبة → نجمع وقت اللعب + نبلّغ الواجهة
    let app2 = app.clone();
    let name2 = meta.name.clone();
    let id2 = id.clone();
    let profile2 = profile.clone();
    tauri::async_runtime::spawn(async move {
        let res = child.wait_with_output().await;
        let played = crate::now_ms().saturating_sub(started);
        let code = res.as_ref().ok().and_then(|o| o.status.code());
        let stderr_tail = log_tail(&id2, 3);

        // نجمع وقت اللعب + ننظف حالة الشغل
        let (stop_requested, snapshot) = match app2.try_state::<GamesState>() {
            Some(st) => {
                let mut stop = false;
                if let Ok(mut run) = st.run.lock() {
                    if let Some(info) = run.remove(&id2) {
                        stop = info.stop_requested;
                    }
                }
                let snap = st.list.lock().ok().map(|mut list| {
                    if let Some(g) = list.iter_mut().find(|g| g.id == id2) {
                        g.played_ms = g.played_ms.saturating_add(played);
                    }
                    list.clone()
                });
                (stop, snap)
            }
            None => (false, None),
        };
        if let Some(snap) = snapshot {
            let _ = save_games(&snap);
        }

        crate::debuglog::raw(
            "info",
            "game",
            &format!(
                "exit game={} code={:?} played_ms={} stopped={} log={}",
                name2,
                code,
                played,
                stop_requested,
                run_log_path(&id2).display()
            ),
        );
        if !stop_requested && code.unwrap_or(0) != 0 {
            crate::debuglog::raw(
                "debug",
                "game",
                &format!(
                    "process output tail:\n{}",
                    crate::debuglog::tail_file(&run_log_path(&id2), 80, 20_000)
                ),
            );
        }

        let quick_fail = !stop_requested && code.unwrap_or(0) != 0 && played < 5_000;
        if stop_requested {
            log(
                &app2,
                "info",
                log_keys::GAME_STOPPED,
                json!({ "name": name2, "dur": fmt_dur(played) }),
            );
        } else if quick_fail {
            let err = if stderr_tail.is_empty() {
                format!("رمز {}", code.unwrap_or(-1))
            } else {
                format!("رمز {} — {}", code.unwrap_or(-1), stderr_tail)
            };
            log(
                &app2,
                "error",
                log_keys::GAME_LAUNCH_FAILED,
                json!({ "name": name2, "error": err }),
            );
        } else {
            log(
                &app2,
                "info",
                log_keys::GAME_EXITED,
                json!({ "name": name2, "dur": fmt_dur(played) }),
            );
        }

        let _ = app2.emit(EVENT, ());
        let _ = app2.emit(
            EVENT_EXIT,
            json!({
                "id": id2,
                "name": name2,
                "code": code,
                "played_ms": played,
                "stopped": stop_requested,
                "quick_fail": quick_fail,
            }),
        );

        // ============================================================
        //  الحلقة المغلقة: تقرير انهيار ⇒ تصنيف ⇒ تركيبة تالية ⇒ إعادة تشغيل
        //
        //  السجل هنا خاص بهذه اللعبة (stdout/stderr ملكنا)، فما نحتاج
        //  فلترة «هل الانهيار لهذه اللعبة» مثل مسار ستيم المشترك.
        // ============================================================
        if stop_requested || attempt >= MAX_AUTO_ATTEMPTS {
            return;
        }
        let Ok(text) = std::fs::read_to_string(run_log_path(&id2)) else {
            return;
        };
        let Some(report) = runner::extract_crash_report(&text) else {
            return;
        };
        let Some(verdict) = crate::triage::triage(&report) else {
            return;
        };
        crate::debuglog::raw(
            "warn",
            "game",
            &format!(
                "crash verdict game={} class={} action={} module={:?}",
                name2,
                crate::triage::class_as_str(verdict.class),
                verdict.action,
                verdict.crashed_module
            ),
        );
        let Some(next) = runner::next_after_crash(&profile2, &verdict.action) else {
            // ما فيه تركيبة تالية: المطلوب من المستخدم (ملف ناقص، تحديث لعبة)
            log(
                &app2,
                "warn",
                log_keys::GAME_LAUNCH_FAILED,
                json!({
                    "name": name2,
                    "error": verdict.headline_ar,
                    "next": verdict.next_ar,
                }),
            );
            return;
        };
        persist_profile(&app2, &id2, &next);
        log(
            &app2,
            "info",
            log_keys::GAME_LAUNCHING,
            json!({
                "name": name2,
                "attempt": attempt + 1,
                "fix": verdict.next_ar,
            }),
        );
        tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;
        // إعادة التشغيل عبر spawn مستقل: انتظار `launch` من داخل المهمة
        // التي أنشأها يخلق دالة async تستدعي نفسها (دورة نوع + Send).
        spawn_retry(app2.clone(), id2.clone(), attempt + 1);
    });

    Ok(())
}

/// إيقاف لعبة شغّالة — نكيل جلسة القارورة كلها بأمان (wineserver -k)
#[tauri::command]
pub async fn game_stop(state: State<'_, GamesState>, id: String) -> Result<(), String> {
    let _trace = crate::debuglog::trace("game", "game_stop", format!("id={id}"));
    let bottle_id = {
        let list = state.list.lock().map_err(|_| "خطأ داخلي".to_string())?;
        list.iter()
            .find(|g| g.id == id)
            .map(|g| g.bottle.clone())
            .ok_or_else(|| "اللعبة مو موجودة".to_string())?
    };

    // نعلّم إن الإيقاف بطلب المستخدم (المراقب يميّزها عن النهاية الطبيعية)
    {
        let mut run = state.run.lock().map_err(|_| "خطأ داخلي".to_string())?;
        match run.get_mut(&id) {
            Some(info) => info.stop_requested = true,
            None => return Err("اللعبة مو شغّالة".into()),
        }
    }

    // الإيقاف الحقيقي: wineserver -k على جلسة هذي القارورة (يوقف كل عمليات اللعبة)
    if cfg!(unix) {
        if let Some(wine) = bottle::wine_bin() {
            if let Some(wineserver) = wine.parent().map(|p| p.join("wineserver")) {
                let mut c = tokio::process::Command::new(&wineserver);
                c.env("WINEPREFIX", bottle::bottle_dir(&bottle_id))
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
