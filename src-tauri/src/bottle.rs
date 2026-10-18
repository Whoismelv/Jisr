// ============================================================
//  جسر — القوارير (بيئات ويندوز المصغّرة)
//  • «القارورة الرئيسية» تُجهّز بضغطة: wineboot ينشئ بيئة ويندوز كاملة
//  • قاعدة أمنية: نزيل Z: بعد كل تجهيز (لا وصول لكل ملفات الماك)
//  • قوارير خاصة اختيارية — الحذف يمسح كل شي داخلها (تحذير بالواجهة)
//  • الحالة تُخزَّن بجانب القارورة (jisr.json) — لا فهرس منفصل ينكسر
// ============================================================
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{data_dir, download, engine, log, log_keys};

pub const MAIN_ID: &str = "main";
const EVENT: &str = "jisr-bottles";
/// مهلة تجهيز القارورة (wineboot) — أول تجهيز على الماك قد يأخذ دقيقة أو أكثر
const BOOT_TIMEOUT_SECS: u64 = 300;

/// بيانات القارورة — تُخزَّن في jisr.json داخل القارورة نفسها
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct BottleMeta {
    pub id: String,
    pub name: String,
    pub kind: String,  // main | private
    pub state: String, // creating | ready | error
    pub error: Option<String>,
    pub created_ms: u64,
}

impl Default for BottleMeta {
    fn default() -> Self {
        BottleMeta {
            id: String::new(),
            name: String::new(),
            kind: "private".into(),
            state: "error".into(),
            error: None,
            created_ms: 0,
        }
    }
}

/// نسخة القارورة اللي تروح للواجهة
#[derive(Serialize, Clone)]
pub struct Bottle {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub state: String,
    pub error: Option<String>,
    pub path: String,
    pub size_bytes: u64,
    pub created_ms: u64,
}

/// قوارير شغّال تجهيزها الآن (في الذاكرة فقط — للحالات العالقة بعد إغلاق التطبيق)
#[derive(Default)]
pub struct BottlesState(pub Mutex<HashSet<String>>);

// ============================================================
//  المسارات والملفات
// ============================================================

pub(crate) fn bottles_dir() -> PathBuf {
    data_dir().join("bottles")
}

pub(crate) fn bottle_dir(id: &str) -> PathBuf {
    bottles_dir().join(id)
}

/// هل القارورة جاهزة فعلاً؟ (الحالة + وجود ملفات ويندوز)
pub(crate) fn bottle_ready(id: &str) -> bool {
    read_meta(id).map(|m| m.state == "ready").unwrap_or(false)
        && bottle_dir(id).join("drive_c").is_dir()
}

fn meta_path(id: &str) -> PathBuf {
    bottle_dir(id).join("jisr.json")
}

fn read_meta(id: &str) -> Option<BottleMeta> {
    let text = std::fs::read_to_string(meta_path(id)).ok()?;
    serde_json::from_str::<BottleMeta>(&text).ok()
}

fn write_meta(meta: &BottleMeta) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(meta).map_err(std::io::Error::other)?;
    std::fs::write(meta_path(&meta.id), text)
}

/// حجم مجلد — ما نتبع الروابط الرمزية (نتجنب الدوران والحجم الغلط)
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else {
                continue; // metadata هنا ما تتبع الروابط
            };
            let ft = md.file_type();
            if ft.is_symlink() {
                continue;
            } else if ft.is_dir() {
                stack.push(e.path());
            } else {
                total += md.len();
            }
        }
    }
    total
}

/// قاعدة أمنية: Z: يفتح كل ملفات الماك — نزيله بعد كل تجهيز وأي فحص
fn remove_z_drive(prefix: &Path) {
    if cfg!(windows) {
        return;
    }
    let dev = prefix.join("dosdevices");
    for name in ["z:", "z"] {
        let p = dev.join(name);
        if std::fs::symlink_metadata(&p).is_ok() {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// ملف تشغيل Wine من أول محرك بنوع engine في الكتالوج
pub(crate) fn wine_bin() -> Option<PathBuf> {
    let id = download::catalog().into_iter().find(|a| a.kind == "engine")?.id;
    engine::wine_bin(id)
}

/// نبلّغ الواجهة إن القوارير تغيّرت (بدون حمولة — الواجهة تعيد القراءة)
fn emit(app: &AppHandle) {
    let _ = app.emit(EVENT, ());
}

// ============================================================
//  المسح على القرص
// ============================================================

/// كل القوارير — نصلّح أثناء المرور أي حالة عالقة (انغلاق وسط التجهيز...)
fn scan_bottles(running: &HashSet<String>) -> Vec<Bottle> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(bottles_dir()) else {
        return out;
    };

    for e in rd.flatten() {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let id = e.file_name().to_string_lossy().to_string();
        let dir = bottle_dir(&id);

        let mut meta = read_meta(&id).unwrap_or_else(|| BottleMeta {
            id: id.clone(),
            name: if id == MAIN_ID {
                "القارورة الرئيسية".into()
            } else {
                id.clone()
            },
            kind: if id == MAIN_ID { "main".into() } else { "private".into() },
            error: Some("ملف التعريف مفقود — أعد التجهيز".into()),
            ..BottleMeta::default()
        });
        meta.id = id.clone();

        // "جاري التجهيز" وما فيه مهمة حية = التطبيق انغلق وهو نص الطريق
        if meta.state == "creating" && !running.contains(&id) {
            meta.state = "error".into();
            meta.error = Some("انقطع التجهيز قبل ما يخلص — أعد المحاولة".into());
            let _ = write_meta(&meta);
        }

        if meta.state == "ready" {
            if !dir.join("drive_c").is_dir() {
                meta.state = "error".into();
                meta.error = Some("ملفات القارورة ناقصة — أعد التجهيز".into());
                let _ = write_meta(&meta);
            } else {
                remove_z_drive(&dir); // Z: ما ترجع أبداً
            }
        }

        out.push(Bottle {
            id,
            name: meta.name.clone(),
            kind: meta.kind.clone(),
            state: meta.state.clone(),
            error: meta.error.clone(),
            path: dir.display().to_string(),
            size_bytes: dir_size(&dir),
            created_ms: meta.created_ms,
        });
    }

    // الرئيسية أولاً ثم الأقدم فالأحدث
    out.sort_by(|a, b| (a.kind != "main", a.created_ms).cmp(&(b.kind != "main", b.created_ms)));
    out
}

// ============================================================
//  التجهيز
// ============================================================

/// يبدأ التجهيز: ينشئ المجلد، يعلّمها "جاري التجهيز"، ويشغّل المهمة بالخلفية
async fn begin_provision(
    app: &AppHandle,
    st: &BottlesState,
    mut meta: BottleMeta,
    simulate: bool,
) -> Result<(), String> {
    let dir = bottle_dir(&meta.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("تعذّر إنشاء مجلد القارورة: {e}"))?;

    // وضع التطوير (ويندوز فقط): نتخطى تشغيل Wine لأنه مستحيل على ويندوز
    if simulate {
        std::fs::create_dir_all(dir.join("drive_c"))
            .map_err(|e| format!("تعذّر إنشاء ملفات القارورة: {e}"))?;
        meta.state = "ready".into();
        meta.error = None;
        write_meta(&meta).map_err(|e| format!("تعذّر حفظ حالة القارورة: {e}"))?;
        log(app, "info", log_keys::BOTTLE_READY, json!({ "kind": meta.kind }));
        emit(app);
        return Ok(());
    }

    meta.state = "creating".into();
    meta.error = None;
    write_meta(&meta).map_err(|e| format!("تعذّر حفظ حالة القارورة: {e}"))?;
    log(app, "info", log_keys::BOTTLE_STARTED, json!({ "kind": meta.kind }));
    emit(app);

    if let Ok(mut s) = st.0.lock() {
        s.insert(meta.id.clone());
    }

    let app2 = app.clone();
    let id2 = meta.id.clone();
    let kind2 = meta.kind.clone();
    tauri::async_runtime::spawn(async move {
        let res = run_wineboot(&id2).await;

        if let Some(st2) = app2.try_state::<BottlesState>() {
            if let Ok(mut s) = st2.0.lock() {
                s.remove(&id2);
            }
        }

        let mut m = read_meta(&id2).unwrap_or_default();
        m.id = id2.clone();
        match res {
            Ok(()) => {
                m.state = "ready".into();
                m.error = None;
                let _ = write_meta(&m);
                log(&app2, "info", log_keys::BOTTLE_READY, json!({ "kind": kind2 }));
            }
            Err(msg) => {
                m.state = "error".into();
                m.error = Some(msg.clone());
                let _ = write_meta(&m);
                log(&app2, "error", log_keys::BOTTLE_FAILED, json!({ "kind": kind2, "error": msg }));
            }
        }
        emit(&app2);
    });

    Ok(())
}

/// يشغّل wineboot داخل القارورة → بيئة ويندوز كاملة (C:، الريجستري...)
async fn run_wineboot(id: &str) -> Result<(), String> {
    let wine = wine_bin().ok_or_else(|| "ما لقينا محرك Wine — ثبّت المحرك أول".to_string())?;
    let prefix = bottle_dir(id);

    let mut cmd = tokio::process::Command::new(&wine);
    cmd.env("WINEPREFIX", &prefix)
        .arg("wineboot")
        .arg("-u")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let out = match tokio::time::timeout(Duration::from_secs(BOOT_TIMEOUT_SECS), cmd.output()).await {
        Err(_) => return Err("التجهيز أخذ وقتاً طويلاً — أعد المحاولة".into()),
        Ok(Err(e)) => {
            let mut msg = format!("تعذّر تشغيل Wine: {e}");
            if cfg!(windows) {
                msg.push_str(" — طبيعي على ويندوز (بيئة تطوير): اضغط «تخطَّ الفحص (وضع التطوير)»");
            }
            return Err(msg);
        }
        Ok(Ok(o)) => o,
    };

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let mut tail: Vec<String> = stderr
            .lines()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(3)
            .map(|s| s.trim().to_string())
            .collect();
        tail.reverse();
        let detail = if tail.is_empty() { "بلا تفاصيل".to_string() } else { tail.join(" | ") };
        return Err(format!(
            "فشل تجهيز ويندوز (رمز {}): {}",
            out.status.code().unwrap_or(-1),
            detail
        ));
    }

    // قاعدة أمنية: Z: تعطي وصولاً لكل ملفات الماك — تُزال دائماً
    remove_z_drive(&prefix);
    Ok(())
}

// ============================================================
//  أوامر Tauri
// ============================================================

/// كل القوارير مع أحجامها على القرص
#[tauri::command]
pub async fn bottles_list(state: State<'_, BottlesState>) -> Result<Vec<Bottle>, String> {
    let running: HashSet<String> = state.0.lock().map(|s| s.clone()).unwrap_or_default();
    let list = tauri::async_runtime::spawn_blocking(move || scan_bottles(&running))
        .await
        .unwrap_or_default();
    Ok(list)
}

/// إنشاء قارورة: "main" (الرئيسية) أو "private" (خاصة)
#[tauri::command]
pub async fn bottle_create(
    app: AppHandle,
    state: State<'_, BottlesState>,
    name: Option<String>,
    kind: Option<String>,
    simulate: Option<bool>,
) -> Result<(), String> {
    // وضع التطوير مسموح على ويندوز فقط — على الماك دائماً تجهيز حقيقي
    let simulate = simulate.unwrap_or(false) && cfg!(windows);
    let kind = match kind.as_deref() {
        Some("main") => "main",
        _ => "private",
    };

    let (id, fallback_name) = if kind == "main" {
        (MAIN_ID.to_string(), "القارورة الرئيسية")
    } else {
        (format!("private-{}", crate::now_ms()), "قارورة خاصة")
    };

    if bottle_dir(&id).exists() {
        return Err(if kind == "main" {
            "القارورة الرئيسية موجودة أصلاً".into()
        } else {
            "القارورة موجودة أصلاً".into()
        });
    }

    let meta = BottleMeta {
        id,
        name: name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| fallback_name.to_string()),
        kind: kind.to_string(),
        state: "creating".to_string(),
        error: None,
        created_ms: crate::now_ms(),
    };

    begin_provision(&app, &state, meta, simulate).await
}

/// إعادة محاولة التجهيز لقارورة فشلت (أو توقفت)
#[tauri::command]
pub async fn bottle_retry(
    app: AppHandle,
    state: State<'_, BottlesState>,
    id: String,
    simulate: Option<bool>,
) -> Result<(), String> {
    let dir = bottle_dir(&id);
    if !dir.is_dir() {
        return Err("القارورة غير موجودة".into());
    }
    if state.0.lock().map(|s| s.contains(&id)).unwrap_or(false) {
        return Err("التجهيز شغّال حالياً".into());
    }

    let meta = read_meta(&id).unwrap_or_else(|| BottleMeta {
        id: id.clone(),
        name: if id == MAIN_ID { "القارورة الرئيسية".into() } else { id.clone() },
        kind: if id == MAIN_ID { "main".into() } else { "private".into() },
        ..BottleMeta::default()
    });

    let simulate = simulate.unwrap_or(false) && cfg!(windows);
    begin_provision(&app, &state, meta, simulate).await
}

/// حذف قارورة خاصة (الرئيسية محميّة — عشان ما تنمسح ألعابك بالغلط)
#[tauri::command]
pub async fn bottle_delete(
    app: AppHandle,
    state: State<'_, BottlesState>,
    id: String,
) -> Result<(), String> {
    if id == MAIN_ID {
        return Err("ما نحذف القارورة الرئيسية".into());
    }
    if state.0.lock().map(|s| s.contains(&id)).unwrap_or(false) {
        return Err("التجهيز شغّال حالياً — انتظر يخلص".into());
    }

    let dir = bottle_dir(&id);
    if !dir.is_dir() {
        return Err("القارورة غير موجودة".into());
    }

    // اسم القارورة مو نص مخزّن — نوعها يكفي والواجهة تترجمها حسب اللغة
    let kind = read_meta(&id)
        .map(|m| m.kind)
        .unwrap_or_else(|| if id == MAIN_ID { "main".into() } else { "private".into() });
    std::fs::remove_dir_all(&dir).map_err(|e| format!("تعذّر حذف القارورة: {e}"))?;
    log(&app, "warn", log_keys::BOTTLE_DELETED, json!({ "kind": kind }));
    emit(&app);
    Ok(())
}
