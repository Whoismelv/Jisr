// ============================================================
//  جسر — القوارير (بيئات ويندوز المصغّرة)
//  • «القارورة الرئيسية» تُجهّز بضغطة: wineboot ينشئ بيئة ويندوز كاملة
//  • قاعدة أمنية: نزيل Z: بعد كل تجهيز (لا وصول لكل ملفات الماك)
//  • قوارير خاصة اختيارية — الحذف يمسح كل شي داخلها (تحذير بالواجهة)
//  • الحالة تُخزَّن بجانب القارورة (jisr.json) — لا فهرس منفصل ينكسر
// ============================================================
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
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
/// • ذاكرة أحجام مؤقتة: حساب حجم القارورة يمشي على كل ملفاتها (ستيم = آلاف
///   الملفات)، فما نكرره مع كل تحديث واجهة — نعيد الحساب بعد دقيقة على الأكثر.
#[derive(Default)]
pub struct BottlesState(
    pub Mutex<HashSet<String>>,
    pub Mutex<HashMap<String, (u64, Instant)>>,
);

/// عمر ذاكرة الحجم — يوازن بين دقة العرض وسرعة الواجهة
const SIZE_CACHE_TTL: Duration = Duration::from_secs(60);

// ============================================================
//  المسارات والملفات
// ============================================================

pub(crate) fn bottles_dir() -> PathBuf {
    data_dir().join("bottles")
}

pub(crate) fn bottle_dir(id: &str) -> PathBuf {
    bottles_dir().join(id)
}

/// حارس المعرّف: المعرّف يُركَّب في مسار على القرص، فأي حرف غير
/// (a-z، 0-9، -، _) = ممنوع. هذا يمنع `..` و`/` و`\` والمسافات
/// وبالتالي أي خروج عن مجلد القوارير (path traversal).
/// المعرّفات المشروعة: "main" و"private-<ms>".
pub(crate) fn valid_bottle_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
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
    let id = download::catalog()
        .into_iter()
        .find(|a| a.kind == "engine")?
        .id;
    engine::wine_bin(id)
}

/// متغيّرات Wine الموحّدة لكل تشغيل:
/// • `winemenubuilder.exe=d` يمنع Wine من إنشاء اختصارات .app خارج القارورة
///   (وعد «كل شيء داخل القارورة» + ما نلوّث ~/Applications)
/// • `steamservice=d` يمنع أخطاء خدمة ستيم داخل Wine (لعميل ستيم فقط)
/// • `WINEMSYNC=1` يفعّل msync (مزامنة Mach) إن كانت مبنية في المحرك —
///   أكبر مكسب أداء مجاني على Apple Silicon لدى Whisky/CrossOver
/// • ستيم فقط: لغة UTF-8 إنجليزية (CEF ينهار مع بعض اللغات) + تعطيل sandbox
pub(crate) fn apply_wine_env(cmd: &mut tokio::process::Command, steam: bool) {
    let overrides = if steam {
        "winemenubuilder.exe=d;steamservice=d"
    } else {
        "winemenubuilder.exe=d"
    };
    cmd.env("WINEDLLOVERRIDES", overrides).env("WINEMSYNC", "1");
    if steam {
        cmd.env("LC_ALL", "en_US.UTF-8")
            .env("LANG", "en_US.UTF-8")
            .env("STEAM_DISABLE_CEF_SANDBOX", "1");
    }
}

/// حزام أمان للعرض على macOS — مرة واحدة لكل قارورة (علامة داخل المجلد).
///
/// السبب الميداني: ألعاب قديمة (DirectX 9) ممكن تظلم **شاشة الماك كلها**
/// أو تاخذ الشاشة بالكامل في وضع windowed-fullscreen → المستخدم يشوف شاشة
/// سودا ويظن إن الماك تعطّل. Wine يخلي ذلك ممكناً عبر:
///   • `AllowSetGamma=n`  — يمنع اللعبة من تظليل الشاشة عبر gamma ramp
///   • `CaptureDisplaysForFullscreen=n` — يمنع احتكار الشاشة بالوضع الكامل
/// القيمتان موجودتان في `winemac.drv` (مصدر Wine: dlls/winemac.drv/macdrv_main.c).
/// غير حرج: أي فشل يُتجاهل (ما نمنع اللعبة من العمل بسبب إعداد).
pub(crate) async fn ensure_mac_safety(wine: &Path, prefix: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    let marker = prefix.join(".jisr-mac-safety-v1");
    if marker.is_file() {
        return true;
    }
    let run = |value: &'static str, setting: &'static str| {
        let mut cmd = tokio::process::Command::new(wine);
        cmd.env("WINEPREFIX", prefix)
            .arg("reg")
            .arg("add")
            .arg("HKCU\\Software\\Wine\\Mac Driver")
            .arg("/v")
            .arg(value)
            .arg("/t")
            .arg("REG_SZ")
            .arg("/d")
            .arg(setting)
            .arg("/f")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        tokio::time::timeout(Duration::from_secs(20), cmd.status())
    };

    // الأمران متوازيان — نوفّر على المستخدم ثواني الانتظار
    let (r1, r2) = tokio::join!(
        run("AllowSetGamma", "n"),
        run("CaptureDisplaysForFullscreen", "n")
    );
    let ok = matches!(r1, Ok(Ok(st)) if st.success()) && matches!(r2, Ok(Ok(st)) if st.success());

    // العلامة تُكتب دائماً — حتى لو فشل الأمر، ما نعيد المحاولة (قد تتأخر التشغيل)
    let _ = std::fs::write(&marker, if ok { "1" } else { "0" });
    ok
}

/// ننسخ جذر الشهادات الرسمي من macOS إلى القارورة (تحسين TLS في Wine/CEF).
/// غير حرج: أي فشل يُتجاهل بهدوء، وما نعيد النسخ لو الحجم مطابق.
pub(crate) fn sync_ca_bundle(prefix: &Path) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let src = Path::new("/etc/ssl/cert.pem");
    let dst = prefix.join("drive_c").join("windows").join("cacert.pem");
    let Ok(src_md) = std::fs::metadata(src) else {
        return;
    };
    if !src_md.is_file() {
        return;
    }
    if let Ok(dst_md) = std::fs::metadata(&dst)
        && dst_md.len() == src_md.len()
    {
        return;
    }
    if let Some(parent) = dst.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::copy(src, &dst);
}

/// نبلّغ الواجهة إن القوارير تغيّرت (بدون حمولة — الواجهة تعيد القراءة)
fn emit(app: &AppHandle) {
    let _ = app.emit(EVENT, ());
}

// ============================================================
//  المسح على القرص
// ============================================================

/// كل القوارير — نصلّح أثناء المرور أي حالة عالقة (انغلاق وسط التجهيز...)
fn scan_bottles(
    running: &HashSet<String>,
    sizes: &mut HashMap<String, (u64, Instant)>,
) -> Vec<Bottle> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let Ok(rd) = std::fs::read_dir(bottles_dir()) else {
        return out;
    };

    for e in rd.flatten() {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let id = e.file_name().to_string_lossy().to_string();
        // مجلدات غريبة (أُنشئت يدوياً أو من برنامج آخر) ما تُعرض ولا تُلمس
        if !valid_bottle_id(&id) {
            continue;
        }
        seen.insert(id.clone());
        let dir = bottle_dir(&id);

        let mut meta = read_meta(&id).unwrap_or_else(|| BottleMeta {
            id: id.clone(),
            name: if id == MAIN_ID {
                "القارورة الرئيسية".into()
            } else {
                id.clone()
            },
            kind: if id == MAIN_ID {
                "main".into()
            } else {
                "private".into()
            },
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

        let size_bytes = match sizes.get(&id) {
            Some((bytes, at)) if at.elapsed() < SIZE_CACHE_TTL => *bytes,
            _ => {
                let bytes = dir_size(&dir);
                sizes.insert(id.clone(), (bytes, Instant::now()));
                bytes
            }
        };

        out.push(Bottle {
            id,
            name: meta.name.clone(),
            kind: meta.kind.clone(),
            state: meta.state.clone(),
            error: meta.error.clone(),
            path: dir.display().to_string(),
            size_bytes,
            created_ms: meta.created_ms,
        });
    }

    // نطرد من الذاكرة أحجام القوارير اللي انحذفت
    sizes.retain(|k, _| seen.contains(k));

    // الرئيسية أولاً ثم الأقدم فالأحدث
    out.sort_by_key(|a| (a.kind != "main", a.created_ms));
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
        log(
            app,
            "info",
            log_keys::BOTTLE_READY,
            json!({ "kind": meta.kind }),
        );
        emit(app);
        return Ok(());
    }

    meta.state = "creating".into();
    meta.error = None;
    write_meta(&meta).map_err(|e| format!("تعذّر حفظ حالة القارورة: {e}"))?;
    log(
        app,
        "info",
        log_keys::BOTTLE_STARTED,
        json!({ "kind": meta.kind }),
    );
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
                log(
                    &app2,
                    "info",
                    log_keys::BOTTLE_READY,
                    json!({ "kind": kind2 }),
                );
            }
            Err(msg) => {
                m.state = "error".into();
                m.error = Some(msg.clone());
                let _ = write_meta(&m);
                log(
                    &app2,
                    "error",
                    log_keys::BOTTLE_FAILED,
                    json!({ "kind": kind2, "error": msg }),
                );
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
    cmd.env("WINEPREFIX", &prefix);
    apply_wine_env(&mut cmd, false);
    cmd.arg("wineboot")
        .arg("-u")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let out = match tokio::time::timeout(Duration::from_secs(BOOT_TIMEOUT_SECS), cmd.output()).await
    {
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
        let detail = if tail.is_empty() {
            "بلا تفاصيل".to_string()
        } else {
            tail.join(" | ")
        };
        return Err(format!(
            "فشل تجهيز ويندوز (رمز {}): {}",
            out.status.code().unwrap_or(-1),
            detail
        ));
    }

    // قاعدة أمنية: Z: تعطي وصولاً لكل ملفات الماك — تُزال دائماً
    remove_z_drive(&prefix);
    // شهادات النظام الرسمية → جلسات TLS داخل القارورة (تحسين غير حرج)
    sync_ca_bundle(&prefix);
    // طبقة DXVK داخل القارورة (system32/syswow64) — لازم تكون موجودة
    // قبل أول تشغيل لعبة، وإلا Wine ما حمّل DXVK أصلاً
    if let Err(e) = crate::graphics::deploy_to_prefix(&prefix) {
        crate::debuglog::write("debug", "bottle", &format!("dxvk deploy skipped: {e}"));
    }
    Ok(())
}

// ============================================================
//  أوامر Tauri
// ============================================================

/// كل القوارير مع أحجامها على القرص
#[tauri::command]
pub async fn bottles_list(state: State<'_, BottlesState>) -> Result<Vec<Bottle>, String> {
    let running: HashSet<String> = state.0.lock().map(|s| s.clone()).unwrap_or_default();
    let cache = state.1.lock().map(|c| c.clone()).unwrap_or_default();
    let (list, new_cache) = tauri::async_runtime::spawn_blocking(move || {
        let mut cache = cache;
        let list = scan_bottles(&running, &mut cache);
        (list, cache)
    })
    .await
    .unwrap_or_default();
    if let Ok(mut c) = state.1.lock() {
        *c = new_cache;
    }
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
    let _trace = crate::debuglog::trace(
        "bottle",
        "bottle_create",
        format!("name={name:?} kind={kind:?} simulate={simulate:?}"),
    );
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
    let _trace = crate::debuglog::trace("bottle", "bottle_retry", format!("id={id}"));
    if !valid_bottle_id(&id) {
        return Err("معرّف قارورة غير صالح".into());
    }
    let dir = bottle_dir(&id);
    if !dir.is_dir() {
        return Err("القارورة غير موجودة".into());
    }
    if state.0.lock().map(|s| s.contains(&id)).unwrap_or(false) {
        return Err("التجهيز شغّال حالياً".into());
    }

    let meta = read_meta(&id).unwrap_or_else(|| BottleMeta {
        id: id.clone(),
        name: if id == MAIN_ID {
            "القارورة الرئيسية".into()
        } else {
            id.clone()
        },
        kind: if id == MAIN_ID {
            "main".into()
        } else {
            "private".into()
        },
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
    let _trace = crate::debuglog::trace("bottle", "bottle_delete", format!("id={id}"));
    if id == MAIN_ID {
        return Err("ما نحذف القارورة الرئيسية".into());
    }
    // حارس المسار: بدون هذا الفحص، id = "../../.." يحذف مجلدات خارج القوارير
    if !valid_bottle_id(&id) {
        return Err("معرّف قارورة غير صالح".into());
    }
    if state.0.lock().map(|s| s.contains(&id)).unwrap_or(false) {
        return Err("التجهيز شغّال حالياً — انتظر يخلص".into());
    }

    let dir = bottle_dir(&id);
    if !dir.is_dir() {
        return Err("القارورة غير موجودة".into());
    }

    // اسم القارورة مو نص مخزّن — نوعها يكفي والواجهة تترجمها حسب اللغة
    let kind = read_meta(&id).map(|m| m.kind).unwrap_or_else(|| {
        if id == MAIN_ID {
            "main".into()
        } else {
            "private".into()
        }
    });
    std::fs::remove_dir_all(&dir).map_err(|e| format!("تعذّر حذف القارورة: {e}"))?;
    if let Ok(mut c) = state.1.lock() {
        c.remove(&id);
    }
    log(
        &app,
        "warn",
        log_keys::BOTTLE_DELETED,
        json!({ "kind": kind }),
    );
    emit(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottle_ids_reject_path_traversal() {
        // المشروع منها
        assert!(valid_bottle_id("main"));
        assert!(valid_bottle_id("private-1699999999999"));
        // مُدخلات خبيثة — كلها مرفوضة قبل أي عملية ملفات
        assert!(!valid_bottle_id(""));
        assert!(!valid_bottle_id("."));
        assert!(!valid_bottle_id(".."));
        assert!(!valid_bottle_id("../evil"));
        assert!(!valid_bottle_id("../../Library"));
        assert!(!valid_bottle_id("a/b"));
        assert!(!valid_bottle_id("a\\b"));
        assert!(!valid_bottle_id("C:"));
        assert!(!valid_bottle_id("a b"));
        assert!(!valid_bottle_id("MAIN"));
        assert!(!valid_bottle_id("main\0"));
        assert!(!valid_bottle_id(&"a".repeat(65)));
    }
}
