// ============================================================
//  جسر — تدفّق ستيم (المثبّت الرسمي → القارورة الرئيسية → التشغيل)
//  • المثبّت: من سيرفرات Valve الرسمية حصراً + بصمة SHA-256 مثبّتة،
//    ويُتحقق منها مرة ثانية قبل كل تشغيل (ثقة صفرية)
//  • ننسخ المثبّت داخل القارورة (C:) قبل تشغيله — Wine ما يقدر
//    يشغّل ملفات من نظام الماك مباشرة (وبلا أي ربط لملفات الماك)
//  • التشغيل: steam.exe مع -no-cef-sandbox (معيار ستيم على Wine)
//  • على ويندوز (بيئة تطوير): أخطاء واضحة وآمنة فقط — نفس نمط الألعاب
// ============================================================
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{bottle, download, engine, log, log_keys};

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

#[derive(Serialize, Clone)]
pub struct SteamStatus {
    pub installer_ready: bool,
    pub installed: bool,
    pub steam_exe: Option<String>,
    pub installing: bool,
    pub running: bool,
    pub bottle_ready: bool,
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
fn steam_exe() -> Option<(PathBuf, String)> {
    let dc = bottle::bottle_dir(bottle::MAIN_ID).join("drive_c");
    for dir in ["Program Files (x86)", "Program Files"] {
        let p = dc.join(dir).join("Steam").join("steam.exe");
        if p.is_file() {
            return Some((p, format!("C:\\{dir}\\Steam\\steam.exe")));
        }
    }
    None
}

fn set_phase(state: &SteamState, phase: Phase) {
    if let Ok(mut p) = state.phase.lock() {
        *p = phase;
    }
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
    })
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
            let msg =
                "ملف المثبّت تغيّر — ما يطابق البصمة الرسمية. احذفه ونزّله من جديد".to_string();
            log(app, "error", log_keys::STEAM_INSTALL_FAILED, json!({ "error": msg }));
            return Err(msg);
        }
        Err(e) => {
            let msg = format!("تعذّر التحقق من المثبّت: {e}");
            log(app, "error", log_keys::STEAM_INSTALL_FAILED, json!({ "error": msg }));
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
            log(app, "error", log_keys::STEAM_INSTALL_FAILED, json!({ "error": msg }));
            return Err(msg);
        }
    }
    if let Err(e) = std::fs::copy(src, &drop) {
        let msg = format!("تعذّر نسخ المثبّت داخل القارورة: {e}");
        log(app, "error", log_keys::STEAM_INSTALL_FAILED, json!({ "error": msg }));
        return Err(msg);
    }

    let dos = format!("C:\\{}", DROP_IN_BOTTLE.replace('/', "\\"));
    let (out, err) = stdio_to_log();
    let mut cmd = tokio::process::Command::new(wine);
    cmd.env("WINEPREFIX", bottle::bottle_dir(bottle::MAIN_ID))
        .arg(dos)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);

    cmd.spawn().map_err(|e| {
        let msg = format!("تعذّر تشغيل المثبّت: {e}");
        log(app, "error", log_keys::STEAM_INSTALL_FAILED, json!({ "error": msg }));
        msg
    })
}

/// ثبّت ستيم داخل القارورة الرئيسية (المثبّت الرسمي — المستخدم يكمّله بنفسه)
#[tauri::command]
pub async fn steam_install(app: AppHandle, state: State<'_, SteamState>) -> Result<(), String> {
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
        log(&app, "error", log_keys::STEAM_INSTALL_FAILED, json!({ "error": msg }));
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
            log(&app2, "error", log_keys::STEAM_INSTALL_FAILED, json!({ "error": err }));
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

/// شغّل ستيم داخل القارورة الرئيسية
#[tauri::command]
pub async fn steam_run(app: AppHandle, state: State<'_, SteamState>) -> Result<(), String> {
    // على ويندوز ما فيه Wine — خطأ واضح وآمن
    if cfg!(windows) {
        let msg = "تشغيل ستيم يحتاج Wine — وهذي بيئة تطوير ويندوز (التشغيل الحقيقي على الماك)";
        log(&app, "error", log_keys::STEAM_LAUNCH_FAILED, json!({ "error": msg }));
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

    let (out, err) = stdio_to_log();
    let mut cmd = tokio::process::Command::new(&wine);
    cmd.env("WINEPREFIX", bottle::bottle_dir(bottle::MAIN_ID))
        .arg(&dos_exe)
        // معيار ستيم على Wine: بند Chromium الداخلي ما يشتغل صح تحت Wine
        .arg("-no-cef-sandbox")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    if let Some(dir) = mac_exe.parent() {
        cmd.current_dir(dir);
    }

    // قفل قصير: نضمن ما فيه أمر ثاني يبدأ بنفس اللحظة
    let child = {
        let mut ph = state.phase.lock().map_err(|_| "خطأ داخلي".to_string())?;
        if *ph != Phase::Idle {
            return Err("ستيم شغّال أصلاً".into());
        }
        let child = cmd.spawn().map_err(|e| {
            let msg = format!("تعذّر تشغيل ستيم: {e}");
            log(&app, "error", log_keys::STEAM_LAUNCH_FAILED, json!({ "error": msg }));
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
            log(&app2, "error", log_keys::STEAM_LAUNCH_FAILED, json!({ "error": err }));
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
