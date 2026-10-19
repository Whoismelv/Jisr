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

use crate::{bottle, data_dir, log, log_keys};

/// تغيّرت قائمة الألعاب (أضفنا/حذفنا/بدأت/انتهت)
const EVENT: &str = "jisr-games";
/// لعبة انتهت — حمولة كاملة للتنبيه
const EVENT_EXIT: &str = "jisr-game-exit";

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
fn path_in_bottle(exe: &Path) -> Option<String> {
    let exe_c = std::fs::canonicalize(exe).ok()?;
    let rd = std::fs::read_dir(bottle::bottles_dir()).ok()?;
    for e in rd.flatten() {
        let Ok(dc) = std::fs::canonicalize(e.path().join("drive_c")) else {
            continue;
        };
        if let Ok(rel) = exe_c.strip_prefix(&dc) {
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            return Some(format!("C:\\{}", parts.join("\\")));
        }
    }
    None
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
                if std::fs::canonicalize(e.path()).map(|t| t == want).unwrap_or(false) {
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
    if let Some(win) = path_in_bottle(&exe) {
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
    Ok(list.iter().map(|m| to_game(m, running.contains(&m.id))).collect())
}

/// إضافة لعبة من ملف .exe (المسار يجي من نافذة الاختيار الأصلية)
#[tauri::command]
pub async fn game_add(
    app: AppHandle,
    state: State<'_, GamesState>,
    exe: String,
) -> Result<Game, String> {
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

    log(&app, "info", log_keys::GAME_ADDED, json!({ "name": meta.name }));
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

    log(&app, "warn", log_keys::GAME_REMOVED, json!({ "name": name }));
    let _ = app.emit(EVENT, ());
    Ok(())
}

/// تشغيل لعبة داخل قارورتها — نرجع فوراً والمراقبة تكمل بالخلفية
#[tauri::command]
pub async fn game_launch(
    app: AppHandle,
    state: State<'_, GamesState>,
    id: String,
) -> Result<(), String> {
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

    if !bottle::bottle_ready(&meta.bottle) {
        return Err("جهّز القارورة الرئيسية أول — من صفحة القوارير".into());
    }

    let wine = bottle::wine_bin().ok_or_else(|| "ما لقينا محرك Wine — ثبّت المحرك أول".to_string())?;
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
    let (out, err) = stdio_to_log(&meta.id);
    let mut cmd = tokio::process::Command::new(&wine);
    cmd.env("WINEPREFIX", bottle::bottle_dir(&meta.bottle))
        .arg(&dos_exe)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    for a in meta.args.split_whitespace() {
        cmd.arg(a);
    }

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
    if let Ok(mut run) = state.run.lock() {
        run.insert(
            id.clone(),
            RunInfo {
                stop_requested: false,
            },
        );
    }
    log(&app, "info", log_keys::GAME_LAUNCHING, json!({ "name": meta.name }));
    let _ = app.emit(EVENT, ());

    // المراقب: ننتظر نهاية اللعبة → نجمع وقت اللعب + نبلّغ الواجهة
    let app2 = app.clone();
    let name2 = meta.name.clone();
    let id2 = id.clone();
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
    });

    Ok(())
}

/// إيقاف لعبة شغّالة — نكيل جلسة القارورة كلها بأمان (wineserver -k)
#[tauri::command]
pub async fn game_stop(state: State<'_, GamesState>, id: String) -> Result<(), String> {
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
