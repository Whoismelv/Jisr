// ============================================================
//  جسر — محرّك التنزيل الآمن + سلسلة التركيب
//  • HTTPS فقط، ومن نطاقات موثوقة حصراً (GitHub) — وفحص *كل* تحويلة
//  • تحقق SHA-256 من بصمة رسمية مثبّتة — ملف مخترق/تالف = يُرفض ويُحذف
//  • استكمال بعد الانقطاع (HTTP Range) + إلغاء بدون فقدان المنزّل
//  • تقدّم لحظي (نسبة + سرعة) يروح للواجهة كأحداث
//  • للمحركات: بعد التحقق نكمل تلقائياً → فك ضغط آمن → تركيب ذرّي → جاهز
// ============================================================
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

use crate::{engine, engines_dir, graphics, log, log_keys};

/// النطاقات المسموحة — بنية GitHub الرسمية + سيرفر ستيم الرسمي (Valve).
/// تُفحص عند بدء الطلب *وعند كل تحويلة* (redirect) — لا استثناءات.
const ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
    "codeload.github.com",
    "cdn.akamai.steamstatic.com",
    // حزمة DirectX الرسمية من مايكروسوفت (D3DX9) — بصمتها مثبّتة في الكتالوج
    "download.microsoft.com",
];

/// سقف الحجم — حماية من رد ضخم غير منطقي (2GB)
const MAX_SIZE: u64 = 2 * 1024 * 1024 * 1024;

fn host_allowed(host: &str) -> bool {
    ALLOWED_HOSTS.iter().any(|h| host.eq_ignore_ascii_case(h))
}

/// فحص رابط كامل: HTTPS إجباري + نطاق من القائمة البيضاء
fn url_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@') // أي userinfo خبيث يُرفض فعلياً (نتجاهله وأمامنا المضيف)
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    host_allowed(host)
}

// ============================================================
//  الكتالوج الرسمي — الإصدارات والبصمات موثّقة من GitHub API
// ============================================================
#[derive(Serialize, Clone)]
pub struct Asset {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub kind: &'static str, // engine | graphics | tool
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub fn catalog() -> Vec<Asset> {
    vec![
        Asset {
            id: "wine-11.18",
            name: "Wine",
            version: "11.18",
            kind: "engine",
            url: "https://github.com/Gcenx/macOS_Wine_builds/releases/download/11.18/wine-devel-11.18-osx64.tar.xz",
            sha256: "aa0ea4c82e636ae7bca2076387cb0a5affa26509ad13f119ecd0d62bd7ba6f82",
            size: 190_974_384,
        },
        // ⚠️ محرّك بديل: 11.6_1 — آخر نسخة قبل ما Gcenx يوقف حزمة
        // «HACK: Steam» (11.7+ became pure upstream Wine ⇒ واجهة ستيم ما تظهر
        // عند بعض الأجهزة). ن offerه كخيار للمستخدم يجرّب لو النافذة ما ظهرت.
        // ملاحظة: 11.6_1 ما فيه MoltenVK_icd.json بنفس المسار ⇒ DXVK/Vulkan
        // ممكن ما تشتغل معه — فنستخدمه لواجهة ستيم فقط.
        Asset {
            id: "wine-11.6_1",
            name: "Wine 11.6.1 (بديل — واجهة ستيم)",
            version: "11.6.1",
            kind: "engine",
            url: "https://github.com/Gcenx/macOS_Wine_builds/releases/download/11.6_1/wine-devel-11.6_1-osx64.tar.xz",
            sha256: "737c5bbcef4dab626e6dcb58f3736bf910780de25d53e52e7d7c01e521da725a",
            size: 319_775_820,
        },
        // طبقة DXVK-macOS (مفتوحة المصدر) — DirectX 9/10/11 داخل Wine.
        // الحزمة: d3d10core + d3d11 من repack Gcenx (مبني للماك)
        //        + d3d9 من DXVK الرسمي 1.10.3 (لأن repack Gcenx ما يجي معه)
        // بدون d3d9 تسقط ألعاب DirectX 9 على WineD3D → ترجمة GLSL → Apple
        // يرفضها (Pixel shader error E5000) → شاشة سوداء.
        // تُحمَّل عبر WINEDLLPATH_PREPEND بلا لمس ملفات القارورة.
        Asset {
            id: graphics::ASSET_ID,
            name: "DXVK",
            version: "1.10.3+d3d9",
            kind: "graphics",
            url: "https://github.com/Whoismelv/Jisr/releases/download/dxvk-d3d9-1.10.3/dxvk-macos-d3d9-1.10.3.tar.gz",
            sha256: "b99a3865a995a60fc0af7b58bc703be3c3e41e755ad1afcd8240637e8c6f3b9c",
            size: 5_043_067,
        },
        // D3DX9 الأصلي من مايكروسوفت — ألعاب DirectX 9 القديمة (LIMBO وأمثالها)
        // تنهار في نسخة Wine المدمجة من d3dx9_43/d3dcompiler_43:
        // ucrtbase → wined3d → d3dcompiler_43 → d3dx9_43 → اللعبة.
        // ننزّل الحزمة الرسمية (بصمة مثبّتة) ونستخرج منها DLLs موثّقة البصمة.
        // ⚠️ لا نعيد توزيع أي ملف من مايكروسوفت — التنزيل يصير عند المستخدم.
        Asset {
            id: crate::d3dx::ASSET_ID,
            name: "DirectX 9 (D3DX9)",
            version: "Jun2010",
            kind: "d3dx9",
            url: "https://download.microsoft.com/download/8/4/A/84A35BF1-DAFE-4AE8-82AF-AD2AE20B6B14/directx_Jun2010_redist.exe",
            sha256: "053f76dcbb28802e23341b6a787e3b0791c0fa5c8d4d011b1044172dbf89c73b",
            size: 100_275_120,
        },
        // مثبّت ستيم الرسمي من Valve.
        // ⚠️ البصمة مربوطة بملف اليوم — Valve يحدّث الملف دورياً؛ لو تغيّر،
        // التنزيل يُرفض بأمان حتى نحدّث البصمة هنا (ننزّل الملف يدوياً ونتحقق).
        Asset {
            id: "steam-setup",
            name: "Steam Setup",
            version: "official",
            kind: "installer",
            url: "https://cdn.akamai.steamstatic.com/client/installer/SteamSetup.exe",
            sha256: "7d3654531c32d941b8cae81c4137fc542172bfa9635f169cb392f245a0a12bcb",
            size: 2_380_800,
        },
    ]
}

/// عنصر من الكتالوج بمعرّفه (المحركات + المثبّتات)
pub fn find_asset(id: &str) -> Option<Asset> {
    catalog().into_iter().find(|a| a.id == id)
}

// ============================================================
//  حالة التنزيلات
// ============================================================
#[derive(Serialize, Clone)]
pub struct Progress {
    pub id: String,
    pub received: u64,
    pub total: u64, // 0 = غير معروف
    pub speed_bps: u64,
    pub files: u64,    // عدد الملفات المفكوكة (مرحلة الفك)
    pub op: String,    // download | install — لأي مرحلة ترجع رسالة الخطأ
    pub phase: String, // starting|downloading|verifying|extracting|done|error|cancelled
    pub error: Option<String>,
}

impl Progress {
    /// حالة أساسية بمصفّرات — نكمل عليها بحقول محددة
    fn p(id: &str, phase: &str) -> Progress {
        Progress {
            id: id.to_string(),
            received: 0,
            total: 0,
            speed_bps: 0,
            files: 0,
            op: "download".to_string(),
            phase: phase.to_string(),
            error: None,
        }
    }
}

pub struct DownloadHandle {
    cancel: Arc<AtomicBool>,
    running: bool,
    last: Progress,
}

#[derive(Default)]
pub struct DownloadsState(pub Mutex<HashMap<String, DownloadHandle>>);

fn downloads_dir() -> PathBuf {
    engines_dir().join("downloads")
}

/// حجم بصيغة مقروءة للبشر (للسجل)
fn human_size(n: u64) -> String {
    let mb = n as f64 / 1_048_576.0;
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else if mb >= 100.0 {
        format!("{:.0} MB", mb)
    } else {
        format!("{:.1} MB", mb)
    }
}

/// ننشر الحالة للواجهة فوراً + نخزّن آخر حالة (للعرض بعد إعادة الفتح)
fn publish(app: &AppHandle, p: Progress) {
    if let Some(st) = app.try_state::<DownloadsState>() {
        if let Ok(mut m) = st.0.lock() {
            if let Some(h) = m.get_mut(&p.id) {
                h.last = p.clone();
            }
        }
    }
    let _ = app.emit("jisr-download", p);
}

fn set_running(app: &AppHandle, id: &str, running: bool) {
    if let Some(st) = app.try_state::<DownloadsState>() {
        if let Ok(mut m) = st.0.lock() {
            if let Some(h) = m.get_mut(id) {
                h.running = running;
            }
        }
    }
}

/// فشل مؤدّب: يكتب في السجل + ينشر حالة الخطأ + يحرّر الحالة
/// op: "download" أو "install" — عشان الواجهة تعرض العنوان الصحيح
fn fail(app: &AppHandle, asset: &Asset, key: &str, op: &str, msg: String) {
    // DXVK اختياري: فشله لا يوقف الألعاب (ترجع WineD3D) — تحذير لا خطأ
    let level = if asset.kind == "graphics" {
        "warn"
    } else {
        "error"
    };
    log(
        app,
        level,
        key,
        json!({ "name": asset.name, "error": msg.clone() }),
    );

    // نبقى على آخر قيم معروفة (نسبة/حجم) بدل ما نصفّرها
    let (received, total) = app
        .try_state::<DownloadsState>()
        .and_then(|st| {
            st.0.lock()
                .ok()
                .and_then(|m| m.get(asset.id).map(|h| (h.last.received, h.last.total)))
        })
        .unwrap_or((0, 0));

    publish(
        app,
        Progress {
            received,
            total,
            error: Some(msg),
            op: op.to_string(),
            ..Progress::p(asset.id, "error")
        },
    );
    set_running(app, asset.id, false);
}

/// بصمة SHA-256 للملف — قراءة متدفقة (ما نحمّل الملف كامل في الذاكرة)
pub(crate) async fn sha256_file(path: &Path) -> std::io::Result<String> {
    use tokio::io::AsyncReadExt;
    let mut f = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 262_144];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

// ============================================================
//  التنزيل نفسه (يعمل في الخلفية — الواجهة ما تتجمّد)
// ============================================================
/// إرسال الطلب مع **إعادة محاولة** (٣ محاولات + فترات انتظار متزايدة).
/// الدرس الميداني: فشل أول تنزيل بسبب خطأ شبكة عابر، ونجح الثاني بلا أي
/// تدخل من المستخدم — اللهجة: «لازم تدوس مرتين» ⇒ نعيد المحاولة بأنفسنا.
async fn send_with_retry(
    client: &reqwest::Client,
    url: &str,
    range_from: u64,
) -> Result<reqwest::Response, String> {
    let waits_ms = [0u64, 1500, 5000];
    let mut last = String::new();
    for (i, wait) in waits_ms.iter().enumerate() {
        if *wait > 0 {
            tokio::time::sleep(Duration::from_millis(*wait)).await;
        }
        let mut req = client.get(url);
        if range_from > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={range_from}-"));
        }
        match req.send().await {
            Ok(r) => {
                // 5xx/429 = خطأ مؤقّت في السيرفر ⇒ نعيد المحاولة
                let status = r.status();
                if status.is_server_error() || status.as_u16() == 429 {
                    last = format!("السيرفر رد بحالة {}", status.as_u16());
                    crate::debuglog::raw(
                        "warn",
                        "download",
                        &format!("attempt {} got HTTP {} — retrying", i + 1, status.as_u16()),
                    );
                    continue;
                }
                return Ok(r);
            }
            Err(e) => {
                last = e.to_string();
                crate::debuglog::raw(
                    "warn",
                    "download",
                    &format!("send attempt {} failed: {e}", i + 1),
                );
            }
        }
    }
    Err(last)
}

async fn run_download(app: AppHandle, asset: Asset, cancel: Arc<AtomicBool>) {
    // ٣ محاولات: انقطاع أثناء النقل (السبب الحقيقي لـ«لازم تدوس مرتين»)
    run_download_attempt(app, asset, cancel, 0).await
}

async fn run_download_attempt(app: AppHandle, asset: Asset, cancel: Arc<AtomicBool>, attempt: u8) {
    let id = asset.id;

    let dir = downloads_dir();
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        fail(
            &app,
            &asset,
            log_keys::DOWNLOAD_FAILED,
            "download",
            format!("تعذّر إنشاء مجلد التنزيل: {e}"),
        );
        return;
    }

    // اسم الملف من الرابط + فحص أمان (ما فيه مسارات ولا رموز غريبة)
    let Some(file_name) = engine::safe_file_name(asset.url) else {
        fail(
            &app,
            &asset,
            log_keys::DOWNLOAD_FAILED,
            "download",
            "اسم ملف غير صالح في الرابط".into(),
        );
        return;
    };

    let part = dir.join(format!("{file_name}.part"));
    let final_path = dir.join(&file_name);

    // منزّل وموثّق من قبل؟ نتحقق ونكمل — بدون إعادة تحميل
    if tokio::fs::metadata(&final_path)
        .await
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        let sz = tokio::fs::metadata(&final_path)
            .await
            .map(|m| m.len())
            .unwrap_or(asset.size);
        publish(
            &app,
            Progress {
                received: sz,
                total: asset.size,
                ..Progress::p(id, "verifying")
            },
        );
        match sha256_file(&final_path).await {
            Ok(h) if h.eq_ignore_ascii_case(asset.sha256) => {
                log(
                    &app,
                    "info",
                    log_keys::DOWNLOAD_DONE,
                    json!({ "name": asset.name, "version": asset.version }),
                );
                // محرك/رسومات؟ نكمل سلسلة التركيب تلقائياً
                match asset.kind {
                    "engine" => {
                        install_engine(&app, &asset, Arc::clone(&cancel)).await;
                        return;
                    }
                    "graphics" => {
                        install_graphics(&app, &asset, Arc::clone(&cancel)).await;
                        return;
                    }
                    "d3dx9" => {
                        install_d3dx9(&app, &asset, &final_path).await;
                        return;
                    }
                    _ => {}
                }
                publish(
                    &app,
                    Progress {
                        received: sz,
                        total: sz,
                        ..Progress::p(id, "done")
                    },
                );
                set_running(&app, id, false);
                return;
            }
            // ملف قديم تالف/جزء ناقص — نمسحه ونحمّل من جديد
            _ => {
                let _ = tokio::fs::remove_file(&final_path).await;
            }
        }
    }

    // عندنا جزء منزّل سابق؟ نكمّل من وين وقفنا
    let mut existing = tokio::fs::metadata(&part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    // العميل: تتبّع تحويلات GitHub فقط + HTTPS إجباري
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        // مهلة عامة: سيرفر يوقف الإرسال ما يعلّق التطبيق للأبد
        .timeout(Duration::from_secs(120))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.previous().len() >= 10 {
                return a.stop();
            }
            match a.url().host_str() {
                Some(h) if a.url().scheme() == "https" && host_allowed(h) => a.follow(),
                _ => a.stop(), // تحويلة لنطاق غير مسموح = نقطع فوراً
            }
        }))
        .user_agent("Jisr/0.1 (macOS game launcher)")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            fail(
                &app,
                &asset,
                log_keys::DOWNLOAD_FAILED,
                "download",
                format!("تعذّر تجهيز الاتصال: {e}"),
            );
            return;
        }
    };

    let mut resp = match send_with_retry(&client, &asset.url, existing).await {
        Ok(r) => r,
        Err(e) => {
            fail(
                &app,
                &asset,
                log_keys::DOWNLOAD_FAILED,
                "download",
                format!("تعذّر بدء التنزيل: {e}"),
            );
            return;
        }
    };

    if !resp.status().is_success()
        && !(resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && existing > 0)
    {
        fail(
            &app,
            &asset,
            log_keys::DOWNLOAD_FAILED,
            "download",
            format!("السيرفر رد بحالة غير متوقعة: {}", resp.status().as_u16()),
        );
        return;
    }

    // الجزء اللي عندنا طلع كامل أصلاً (416) → نمسحه ونطلب من جديد من الصفر
    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let _ = tokio::fs::remove_file(&part).await;
        existing = 0;
        // بلا ترويسة Range = طلب كامل جديد
        resp = match send_with_retry(&client, &asset.url, 0).await {
            Ok(r) => r,
            Err(e) => {
                fail(
                    &app,
                    &asset,
                    log_keys::DOWNLOAD_FAILED,
                    "download",
                    format!("تعذّر بدء التنزيل: {e}"),
                );
                return;
            }
        };
        if !resp.status().is_success() {
            fail(
                &app,
                &asset,
                log_keys::DOWNLOAD_FAILED,
                "download",
                format!("السيرفر رد بحالة غير متوقعة: {}", resp.status().as_u16()),
            );
            return;
        }
    }

    // السيرفر دعم الاستكمال؟ (206 = يكمل من حيث توقفنا)
    let resuming = existing > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut received: u64 = if resuming { existing } else { 0 };
    let remaining = resp.content_length().unwrap_or(0);
    let total = if resuming {
        existing.saturating_add(remaining)
    } else {
        remaining
    };

    if total > MAX_SIZE {
        fail(
            &app,
            &asset,
            log_keys::DOWNLOAD_FAILED,
            "download",
            "حجم الملف أكبر من الحد المسموح — رفضناه".into(),
        );
        return;
    }

    // ملف التنزيل: استكمال (إضافة) أو بداية جديدة (يمسح الجزء القديم)
    let mut file = if resuming {
        match tokio::fs::OpenOptions::new().append(true).open(&part).await {
            Ok(f) => f,
            Err(e) => {
                fail(
                    &app,
                    &asset,
                    log_keys::DOWNLOAD_FAILED,
                    "download",
                    format!("تعذّر فتح ملف الجزء: {e}"),
                );
                return;
            }
        }
    } else {
        match tokio::fs::File::create(&part).await {
            Ok(f) => f,
            Err(e) => {
                fail(
                    &app,
                    &asset,
                    log_keys::DOWNLOAD_FAILED,
                    "download",
                    format!("تعذّر إنشاء ملف التنزيل: {e}"),
                );
                return;
            }
        }
    };

    let shown_total = if total > 0 { total } else { asset.size };
    log(
        &app,
        "info",
        log_keys::DOWNLOAD_STARTED,
        json!({
            "name": asset.name,
            "version": asset.version,
            "size": human_size(shown_total),
        }),
    );

    publish(
        &app,
        Progress {
            received,
            total,
            ..Progress::p(id, "downloading")
        },
    );

    // ---------- حلقة الاستقبال ----------
    let mut last_tick = Instant::now();
    let mut last_bytes = received;
    let mut failure: Option<String> = None;
    let mut cancelled = false;

    loop {
        let chunk = tokio::select! {
            r = resp.chunk() => Some(r),
            // نصحى كل 300ms نتأكد من الإلغاء (حتى لو التحميل متجمّد)
            _ = tokio::time::sleep(Duration::from_millis(300)) => None,
        };

        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let Some(chunk) = chunk else { continue };

        match chunk {
            Ok(Some(bytes)) => {
                if let Err(e) = file.write_all(&bytes).await {
                    failure = Some(format!("تعذّر الكتابة على القرص: {e}"));
                    break;
                }
                received += bytes.len() as u64;

                if received > MAX_SIZE {
                    failure = Some("تجاوز الحجم المسموح — أوقفنا التنزيل".into());
                    break;
                }

                // ننشر التقدّم كل ربع ثانية تقريباً (مريح للعين وخفيف على الواجهة)
                let now = Instant::now();
                let dt = now.duration_since(last_tick);
                if dt.as_millis() >= 250 {
                    let speed = ((received - last_bytes) as f64 / dt.as_secs_f64()) as u64;
                    publish(
                        &app,
                        Progress {
                            received,
                            total,
                            speed_bps: speed,
                            ..Progress::p(id, "downloading")
                        },
                    );
                    last_tick = now;
                    last_bytes = received;
                }
            }
            Ok(None) => break, // وصلنا لنهاية الملف
            Err(e) => {
                failure = Some(format!("انقطع الاتصال: {e}"));
                break;
            }
        }
    }

    let _ = file.flush().await;
    drop(file);

    // ---------- الإلغاء: نحفظ الجزء للاستكمال لاحقاً ----------
    if cancelled {
        publish(
            &app,
            Progress {
                received,
                total,
                ..Progress::p(id, "cancelled")
            },
        );
        log(
            &app,
            "warn",
            log_keys::DOWNLOAD_CANCELLED,
            json!({ "name": asset.name }),
        );
        set_running(&app, id, false);
        return;
    }

    if let Some(msg) = failure {
        // انقطاع أثناء النقل ⇒ نعيد المحاولة بنفس الجزء (استكمال تلقائي)
        if attempt < 2 {
            crate::debuglog::raw(
                "warn",
                "download",
                &format!("attempt {} failed: {msg} — retrying", attempt + 1),
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
            Box::pin(run_download_attempt(app, asset, cancel, attempt + 1)).await;
            return;
        }
        fail(&app, &asset, log_keys::DOWNLOAD_FAILED, "download", msg);
        return;
    }

    // ---------- التحقق من البصمة (أهم خطوة أمنية) ----------
    publish(
        &app,
        Progress {
            received,
            total,
            ..Progress::p(id, "verifying")
        },
    );

    let hash = match sha256_file(&part).await {
        Ok(h) => h,
        Err(e) => {
            fail(
                &app,
                &asset,
                log_keys::DOWNLOAD_FAILED,
                "download",
                format!("تعذّر قراءة الملف للتحقق: {e}"),
            );
            return;
        }
    };

    if !hash.eq_ignore_ascii_case(asset.sha256) {
        // لا يطابق البصمة الرسمية → يمسح فوراً، ما نلمسه أبداً
        let _ = tokio::fs::remove_file(&part).await;
        fail(
            &app,
            &asset,
            log_keys::DOWNLOAD_FAILED,
            "download",
            "بصمة الملف لا تطابق البصمة الرسمية — حذفناه".into(),
        );
        return;
    }

    // البصمة صحيحة 100% → نثبّت الاسم النهائي
    if let Err(e) = tokio::fs::rename(&part, &final_path).await {
        fail(
            &app,
            &asset,
            log_keys::DOWNLOAD_FAILED,
            "download",
            format!("تعذّر تثبيت الملف النهائي: {e}"),
        );
        return;
    }

    log(
        &app,
        "info",
        log_keys::DOWNLOAD_DONE,
        json!({ "name": asset.name, "version": asset.version }),
    );

    // ---------- سلسلة التركيب: نكمل تلقائياً → فك الضغط والتركيب ----------
    match asset.kind {
        "engine" => {
            install_engine(&app, &asset, Arc::clone(&cancel)).await;
            return;
        }
        "graphics" => {
            install_graphics(&app, &asset, Arc::clone(&cancel)).await;
            return;
        }
        "d3dx9" => {
            install_d3dx9(&app, &asset, &final_path).await;
            return;
        }
        _ => {}
    }

    let size = tokio::fs::metadata(&final_path)
        .await
        .map(|m| m.len())
        .unwrap_or(received);

    publish(
        &app,
        Progress {
            received: size,
            total: if total > 0 { total } else { size },
            ..Progress::p(id, "done")
        },
    );
    set_running(&app, id, false);
}

// ============================================================
//  تركيب المحرك: فك ضغط آمن → تحقق بنية → نقل ذرّي للمكان النهائي
// ============================================================
async fn install_engine(app: &AppHandle, asset: &Asset, cancel: Arc<AtomicBool>) {
    let id = asset.id;

    // مثبّت وجاهز من قبل؟ نعلن الجاهزية على طول
    if engine::is_installed(id) {
        publish(
            app,
            Progress {
                received: asset.size,
                total: asset.size,
                ..Progress::p(id, "done")
            },
        );
        log(
            app,
            "info",
            log_keys::ENGINE_READY,
            json!({ "path": engine::install_dir(id).display().to_string() }),
        );
        set_running(app, id, false);
        ensure_graphics_layer(app);
        return;
    }

    // الأرشيف المنزّل الموثّق (تحققنا منه قبل الوصول هنا)
    let Some(archive) = engine::archive_path(asset.url) else {
        fail(
            app,
            asset,
            log_keys::EXTRACT_FAILED,
            "install",
            "اسم ملف غير صالح في الرابط".into(),
        );
        return;
    };
    if !archive.is_file() {
        fail(
            app,
            asset,
            log_keys::EXTRACT_FAILED,
            "install",
            "ملف المحرك غير موجود — لازم ننزّله أول".into(),
        );
        return;
    }

    let archive_size = std::fs::metadata(&archive)
        .map(|m| m.len())
        .unwrap_or(asset.size);

    log(
        app,
        "info",
        log_keys::EXTRACT_STARTED,
        json!({ "name": asset.name, "version": asset.version }),
    );

    // نبدأ بمجلد فك نظيف دائماً
    let staging = engine::staging_dir(id);
    let _ = std::fs::remove_dir_all(&staging);

    publish(
        app,
        Progress {
            total: archive_size,
            ..Progress::p(id, "extracting")
        },
    );

    // الفك عمل ثقيل ومتزامن → خيط منفصل حتى ما تتجمد الواجهة أبداً
    let app2 = app.clone();
    let id2 = id.to_string();
    let archive2 = archive.clone();
    let staging2 = staging.clone();
    let cancel2 = Arc::clone(&cancel);

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let cb_app = app2.clone();
        let cb_id = id2.clone();
        let cb = move |read: u64, total: u64, files: u64| {
            publish(
                &cb_app,
                Progress {
                    received: read,
                    total,
                    files,
                    ..Progress::p(&cb_id, "extracting")
                },
            );
        };
        engine::install_pipeline(&id2, &archive2, &staging2, &cancel2, &cb)
    })
    .await;

    let result: Result<engine::Installed, engine::ExtErr> = match joined {
        Ok(r) => r,
        Err(e) => Err(engine::ExtErr::Failed(format!(
            "خطأ غير متوقع في خيط الفك: {e}"
        ))),
    };

    match result {
        Ok(done) => {
            // نمسح الأرشيف (ما نحتاجه بعد التركيب) + بقايا مجلد الفك
            let _ = tokio::fs::remove_file(&archive).await;
            let _ = tokio::fs::remove_dir_all(&staging).await;

            publish(
                app,
                Progress {
                    received: archive_size,
                    total: archive_size,
                    files: done.files,
                    ..Progress::p(id, "done")
                },
            );
            log(
                app,
                "info",
                log_keys::EXTRACT_DONE,
                json!({ "files": done.files, "size": human_size(done.bytes) }),
            );
            // روابط تخطّيناها أثناء الفك (خبيثة أو غير مدعومة على ويندوز)؟
            if done.skipped > 0 {
                log(
                    app,
                    "warn",
                    log_keys::EXTRACT_SKIPPED,
                    json!({ "count": done.skipped }),
                );
            }
            log(
                app,
                "info",
                log_keys::ENGINE_READY,
                json!({ "path": done.path }),
            );
            set_running(app, id, false);
            // طبقة الأداء تلقائياً بعد المحرك (فشلها لا يمنع تشغيل الألعاب)
            ensure_graphics_layer(app);
        }
        Err(engine::ExtErr::Cancelled) => {
            // إلغاء نظيف: نمحي مجلد الفك — الأرشيف يظل (الفك الثاني بلا تنزيل جديد)
            let _ = tokio::fs::remove_dir_all(&staging).await;
            publish(app, Progress::p(id, "cancelled"));
            log(
                app,
                "warn",
                log_keys::DOWNLOAD_CANCELLED,
                json!({ "name": asset.name }),
            );
            set_running(app, id, false);
        }
        Err(engine::ExtErr::Failed(msg)) => {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            fail(app, asset, log_keys::EXTRACT_FAILED, "install", msg);
        }
    }
}

// ============================================================
//  تركيب طبقة الرسومات (DXVK): فك آمن → تحقق بنية → نقل ذرّي
// ============================================================
async fn install_graphics(app: &AppHandle, asset: &Asset, cancel: Arc<AtomicBool>) {
    let id = asset.id;

    if graphics::is_installed() {
        publish(
            app,
            Progress {
                received: asset.size,
                total: asset.size,
                ..Progress::p(id, "done")
            },
        );
        set_running(app, id, false);
        return;
    }

    let Some(archive) = engine::archive_path(asset.url) else {
        fail(
            app,
            asset,
            log_keys::GRAPHICS_FAILED,
            "install",
            "اسم ملف غير صالح في الرابط".into(),
        );
        return;
    };
    if !archive.is_file() {
        fail(
            app,
            asset,
            log_keys::GRAPHICS_FAILED,
            "install",
            "ملف DXVK غير موجود — لازم ننزّله أول".into(),
        );
        return;
    }

    let archive_size = std::fs::metadata(&archive)
        .map(|m| m.len())
        .unwrap_or(asset.size);

    log(
        app,
        "info",
        log_keys::EXTRACT_STARTED,
        json!({ "name": asset.name, "version": asset.version }),
    );
    publish(
        app,
        Progress {
            total: archive_size,
            ..Progress::p(id, "extracting")
        },
    );

    let app2 = app.clone();
    let id2 = id.to_string();
    let archive2 = archive.clone();
    let cancel2 = Arc::clone(&cancel);

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let cb_app = app2.clone();
        let cb_id = id2.clone();
        let cb = move |read: u64, total: u64, files: u64| {
            publish(
                &cb_app,
                Progress {
                    received: read,
                    total,
                    files,
                    ..Progress::p(&cb_id, "extracting")
                },
            );
        };
        graphics::install(&archive2, &cancel2, &cb)
    })
    .await;

    let result = match joined {
        Ok(r) => r,
        Err(e) => Err(engine::ExtErr::Failed(format!(
            "خطأ غير متوقع في خيط فك DXVK: {e}"
        ))),
    };

    match result {
        Ok((files, bytes)) => {
            let _ = tokio::fs::remove_file(&archive).await;
            publish(
                app,
                Progress {
                    received: archive_size,
                    total: archive_size,
                    files,
                    ..Progress::p(id, "done")
                },
            );
            log(
                app,
                "info",
                log_keys::GRAPHICS_READY,
                json!({ "files": files, "size": human_size(bytes) }),
            );
            set_running(app, id, false);
        }
        Err(engine::ExtErr::Cancelled) => {
            // إلغاء: نمسح بقايا الفك (الأرشيف يبقى للاستكمال)
            graphics::cleanup_staging();
            publish(app, Progress::p(id, "cancelled"));
            set_running(app, id, false);
        }
        Err(engine::ExtErr::Failed(msg)) => {
            graphics::cleanup_staging();
            fail(app, asset, log_keys::GRAPHICS_FAILED, "install", msg);
        }
    }
}

// ============================================================
//  تركيب D3DX9: فك CAB داخلي (بلا أدوات) → تحقق بصمة كل DLL → نشر
// ============================================================
async fn install_d3dx9(app: &AppHandle, asset: &Asset, file: &Path) {
    let id = asset.id;
    publish(
        app,
        Progress {
            ..Progress::p(id, "extracting")
        },
    );

    let src = file.to_path_buf();
    let result = tokio::task::spawn_blocking(move || crate::d3dx::extract_from_redist(&src))
        .await
        .unwrap_or_else(|e| Err(format!("خطأ غير متوقع في خيط فك D3DX9: {e}")));

    let written = match result {
        Ok(n) => n,
        Err(msg) => {
            fail(app, asset, log_keys::GRAPHICS_FAILED, "install", msg);
            return;
        }
    };

    // نشر فوري في القارورة الرئيسية لو جاهزة (وإلا يصير عند تشغيل اللعبة)
    let mut deployed = false;
    if crate::bottle::bottle_ready(crate::bottle::MAIN_ID) {
        if let Some(wine) = crate::bottle::wine_bin() {
            let prefix = crate::bottle::bottle_dir(crate::bottle::MAIN_ID);
            deployed = crate::d3dx::deploy_to_prefix(&wine, &prefix).await.is_ok();
        }
    }

    let size = tokio::fs::metadata(file)
        .await
        .map(|m| m.len())
        .unwrap_or(asset.size);
    publish(
        app,
        Progress {
            received: size,
            total: size,
            files: written as u64,
            ..Progress::p(id, "done")
        },
    );
    log(
        app,
        "info",
        log_keys::GRAPHICS_READY,
        json!({ "name": asset.name, "files": written, "deployed": deployed }),
    );
    set_running(app, id, false);
}

// ============================================================
//  أوامر Tauri
// ============================================================

/// كتالوج المحركات (الواجهة تعرض الأسماء/الإصدارات/الأحجام)
#[tauri::command]
pub fn download_catalog() -> Vec<Asset> {
    catalog()
}

/// يبدأ أي تنزيل (من الواجهة أو تلقائياً من سلسلة التجهيز)
fn begin_download(app: &AppHandle, asset: Asset) -> Result<(), String> {
    let id = asset.id.to_string();
    let cancel = Arc::new(AtomicBool::new(false));

    let state = app
        .try_state::<DownloadsState>()
        .ok_or_else(|| "خطأ داخلي: حالة التنزيل غير جاهزة".to_string())?;
    {
        let mut map = state.0.lock().map_err(|_| "خطأ داخلي".to_string())?;
        if map.get(&id).map(|h| h.running).unwrap_or(false) {
            return Err("التنزيل شغّال أصلاً".into());
        }
        map.insert(
            id.clone(),
            DownloadHandle {
                cancel: cancel.clone(),
                running: true,
                last: Progress {
                    total: asset.size,
                    ..Progress::p(&id, "starting")
                },
            },
        );
    }

    // ننشر حالة البداية فوراً (الواجهة تعرض "جاري التحضير…")
    publish(
        app,
        Progress {
            total: asset.size,
            ..Progress::p(&id, "starting")
        },
    );

    tauri::async_runtime::spawn(run_download(app.clone(), asset, cancel));
    Ok(())
}

/// DXVK يُنزَّل تلقائياً بعد تجهيز Wine (5MB فقط) — وإلا تبقى الألعاب
/// على WineD3D الأبطأ. أي فشل هنا غير قاتل: الألعاب تشتغل بالطبقة الافتراضية.
pub(crate) fn ensure_graphics_layer(app: &AppHandle) {
    if graphics::is_installed() {
        return;
    }
    let Some(asset) = find_asset(graphics::ASSET_ID) else {
        return;
    };
    if !url_allowed(asset.url) {
        return;
    }
    let _ = begin_download(app, asset);
}

/// D3DX9 الأصلي (حزمة DirectX) يُنزَّل تلقائياً لوDetectilu حدّث D3DX9
/// (نفس منطق `ensure_graphics_layer`).
pub(crate) fn ensure_d3dx9_layer(app: &AppHandle) {
    if crate::d3dx::is_installed() {
        return;
    }
    let Some(asset) = find_asset(crate::d3dx::ASSET_ID) else {
        return;
    };
    if !url_allowed(asset.url) {
        return;
    }
    let _ = begin_download(app, asset);
}

/// ابدأ تنزيل عنصر من الكتالوج (id مثل "wine-11.18")
#[tauri::command]
pub async fn download_start(app: AppHandle, id: String) -> Result<(), String> {
    let _trace = crate::debuglog::trace("download", "download_start", format!("id={id}"));
    let asset = find_asset(&id).ok_or_else(|| "معرّف غير معروف".to_string())?;

    // الحارس الأول: الرابط HTTPS ومن نطاق مسموح — قبل أي اتصال
    if !url_allowed(asset.url) {
        return Err("رفضنا التنزيل — الرابط خارج النطاقات المسموحة".into());
    }

    begin_download(&app, asset)
}

/// ألغِ تنزيلاً/تركيباً — الجزء المنزّل والأرشيف يُحفظان (نستكمل أي وقت لاحق)
#[tauri::command]
pub fn download_cancel(state: State<'_, DownloadsState>, id: String) {
    let _trace = crate::debuglog::trace("download", "download_cancel", format!("id={id}"));
    if let Ok(m) = state.0.lock() {
        if let Some(h) = m.get(&id) {
            h.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// آخر حالة معروفة لكل تنزيل (لما تُفتح الواجهة من جديد)
#[tauri::command]
pub fn downloads_snapshot(state: State<'_, DownloadsState>) -> Vec<Progress> {
    state
        .0
        .lock()
        .map(|m| m.values().map(|h| h.last.clone()).collect())
        .unwrap_or_default()
}
