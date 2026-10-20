// ============================================================
//  جسر — متطلبات ويندوز: Visual C++ 2015‑2022 (x86 + x64)
// ============================================================
//! **المشكلة:** كثير من ألعاب ويندوز (حتى القديمة) تعتمد على
//! `msvcp140.dll` / `vcruntime140.dll` (Visual C++ 14). Wine يوفّر نسخاً
//! مدمجة ناقصة، وبعض الألعاب ترفض الاشتغال عليها.
//!
//! **الحل (نفس ما يفعله winetricks/Bottles):** ننزّل حزمة مايكروسوفت
//! الرسمية `VC_redist.{x86,x64}.exe` (بصمة SHA‑256 مثبّتة)، نثبّتها
//! بصمت **بالترتيب** (x86 ثم x64 — قفل MSI عالمي)، نتحقق من الملفات
//! فعلياً، ولو فشل مثبّت Wine نستخرج الـDLLs من الحمولة داخل الحزمة
//! ونصلح الناقص (Wine bug 57518: مثبّت VC++ ينسى msvcp140_2).
//!
//! • لا نعيد توزيع أي ملف من مايكروسوفت — التنزيل يصير عند المستخدم.
//! • كل ملف نكتبه مُتحقَّق من حجمه وبصمته.
//! • `native,builtin` لعائلة VC14 فقط — **لا** نلمس `ucrtbase` ولا
//!   `api-ms-win-crt-*` (هذه يجب أن تبقى من Wine/النظام).

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::process::Command;

/// إصدار الحزمة المثبّت (قناة VS 18) — نثبّته للتحقق وللعرض.
pub const VERSION: &str = "14.51.36247.0";

/// أسماء DLLs التي نضعها `native,builtin`.
/// (لا `ucrtbase`، لا `api-ms-win-crt-*` — هذه ليست جزءاً من الحمولة.)
const OVERRIDES: &[&str] = &[
    "concrt140",
    "msvcp140",
    "msvcp140_1",
    "msvcp140_2",
    "msvcp140_atomic_wait",
    "msvcp140_codecvt_ids",
    "vcamp140",
    "vccorlib140",
    "vcomp140",
    "vcruntime140",
    "vcruntime140_1",
    "vcruntime140_threads",
];

struct Bundle {
    arch: &'static str,
    sys_dir: &'static str,
    file: &'static str,
    url: &'static str,
    sha256: &'static str,
    size: u64,
    /// (اسم الملف، الحجم، البصمة) — من حمولة الحزمة الرسمية 14.51.36247.0
    dlls: &'static [(&'static str, u64, &'static str)],
}

const X86_DLLS: &[(&str, u64, &str)] = &[
    (
        "concrt140.dll",
        314_816,
        "63de7cc3187265b1b1871b84a11b8a91d8f249018dcfb34e910bf3eb2a193d11",
    ),
    (
        "msvcp140.dll",
        618_944,
        "f0cda2a0cf1fe6fbbf579b9098462329d3aaa7513a207af2e0f33b01456e388a",
    ),
    (
        "msvcp140_1.dll",
        33_728,
        "b08edd7954ae9954d24eb344a8b0116a980c60179ff1b92d5219639e21028cc2",
    ),
    (
        "msvcp140_2.dll",
        261_056,
        "5e3c52721f03d5f546d64dfdfa38302b1fc3eb79dc44a1ec7a3af69abb9653fe",
    ),
    (
        "msvcp140_atomic_wait.dll",
        57_272,
        "e1142584f337b9b650d6d888652cb24f7b16a7dd93476840a050047bb00f0463",
    ),
    (
        "msvcp140_codecvt_ids.dll",
        30_648,
        "d4874db10c63990323696984eb2e2e799d952a1846437924c92153065b7259e4",
    ),
    (
        "vcamp140.dll",
        379_320,
        "80ce7e8ef6a31b4beb725b408104da91f7db53891ce024554e3b21196492958f",
    ),
    (
        "vccorlib140.dll",
        287_160,
        "d50aab1e201dfb268d6010d4c3b5bf7a3fb8dd1f38296135b43c9e29821e5586",
    ),
    (
        "vcomp140.dll",
        186_304,
        "da9157373ab71f97f8920688f3d6f4057e3eb2ddd32895379ab9c15e69d598cd",
    ),
    (
        "vcruntime140.dll",
        123_328,
        "2fa6efc053203460a23d3a25158f227d895d2dadc63acc1a372da97c3a4281c3",
    ),
    (
        "vcruntime140_threads.dll",
        36_280,
        "7c372b0547fb45abd06e4bbdf5475cc99c3a915c998ef7e5c263de4762944e3b",
    ),
];

const X64_DLLS: &[(&str, u64, &str)] = &[
    (
        "concrt140.dll",
        374_200,
        "54716f0738af891f283d213b5c8d11b25896bb8ee3097d301eae718560cf974e",
    ),
    (
        "msvcp140.dll",
        643_512,
        "7c26614e1d733892c2deac7e245ce115504b1d80592dd0a01b08e3e5a55f89ca",
    ),
    (
        "msvcp140_1.dll",
        35_768,
        "206c931bf90fdad8816de3b5e2ef80b2bcaa9406c89ecc05fe6fddffe251e982",
    ),
    (
        "msvcp140_2.dll",
        274_872,
        "d50d7883f20d1dc6191768d3746f52dd9cac89c346ffaed5be1f110c2f34a838",
    ),
    (
        "msvcp140_atomic_wait.dll",
        57_792,
        "3d0cbfaa1bf3eecf5a3f4491d2960ee803cb994f30292c6adc4a07c498f60e2b",
    ),
    (
        "msvcp140_codecvt_ids.dll",
        31_160,
        "8a65c7596ef2e6938731f5a1058e7e40145b6d97967cc649231a076b9a608d78",
    ),
    (
        "vcamp140.dll",
        417_728,
        "dc283ffb87e9b82e687ed4e899ae062a209aa338207abfc9a89d61407d678bd4",
    ),
    (
        "vccorlib140.dll",
        350_648,
        "09f93d5ff96ae09767f3e1af3cdd43a5a58f2598654fc63089429bb6f9f4737f",
    ),
    (
        "vcomp140.dll",
        212_920,
        "95d4ce4a6802d1e18b5e0e1722cc30ea72ca7e033f83828f05c0b7b993fe7cbf",
    ),
    (
        "vcruntime140.dll",
        178_616,
        "d1f4225df2cd877dbf130d5668a021dce3f94118455ff5ec952061c30afc9ce7",
    ),
    (
        "vcruntime140_1.dll",
        50_112,
        "a7146c08f89fe5b04541ab507cdb59ff7b44534d4ba3c668a426c6450a03434e",
    ),
    (
        "vcruntime140_threads.dll",
        38_840,
        "40f39e8cee5f5a531b4010e18b807e8fe265b908dc58f959ecb7ebc6ea6cb11a",
    ),
];

const BUNDLES: [Bundle; 2] = [
    Bundle {
        arch: "x86",
        sys_dir: "syswow64",
        file: "VC_redist.x86.exe",
        url: "https://download.visualstudio.microsoft.com/download/pr/7cf9e46d-7b59-448e-80f1-988e1cb3c0c2/F0BAB33A302B3CDB2E11113760D016F54FD3D2632C65BA7834FAC4F0ABD7F1A3/VC_redist.x86.exe",
        sha256: "f0bab33a302b3cdb2e11113760d016f54fd3d2632c65ba7834fac4f0abd7f1a3",
        size: 6_941_536,
        dlls: X86_DLLS,
    },
    Bundle {
        arch: "x64",
        sys_dir: "system32",
        file: "VC_redist.x64.exe",
        url: "https://download.visualstudio.microsoft.com/download/pr/ebdab8e5-1d7b-4d9f-a11b-cbb1720c3b12/843068991DAAA1F73AD9F6239BCE4D0F6A07A51F18C37EA2A867E9BECA71295C/VC_redist.x64.exe",
        sha256: "843068991daaa1f73ad9f6239bce4d0f6a07a51f18c37ea2a867e9beca71295c",
        size: 18_731_856,
        dlls: X64_DLLS,
    },
];

fn install_dir() -> PathBuf {
    crate::data_dir().join("vcredist")
}

fn bundle_path(b: &Bundle) -> PathBuf {
    install_dir().join(b.arch).join(b.file)
}

fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// الحزمة موجودة ومطابقة للحجم/البصمة المثبّتة؟
fn bundle_ready(b: &Bundle) -> bool {
    let p = bundle_path(b);
    std::fs::metadata(&p)
        .map(|m| m.len() == b.size)
        .unwrap_or(false)
        && std::fs::read(&p)
            .map(|d| sha256(&d) == b.sha256)
            .unwrap_or(false)
}

// ============================================================
//  الحالة
// ============================================================

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct VcredistStatus {
    /// كل الملفات الأساسية موجودة في المعماريتين؟
    pub installed: bool,
    pub x86: bool,
    pub x64: bool,
    pub version: String,
    /// أول الملفات الناقصة (للعرض والتشخيص)
    pub missing: Vec<String>,
}

/// يفحص القارورة فعلياً (حجم كل ملف) — **ما يثق بالريجستري** لأن Wine
/// يسبق أنه يكتب `Installed=1` لحزمة قديمة.
pub fn verify(prefix: &Path) -> VcredistStatus {
    let win = prefix.join("drive_c").join("windows");
    let mut missing: Vec<String> = Vec::new();
    let mut x86 = true;
    let mut x64 = true;
    for b in &BUNDLES {
        let dir = win.join(b.sys_dir);
        for (name, size, _) in b.dlls {
            let ok = std::fs::metadata(dir.join(name))
                .map(|m| m.len() == *size)
                .unwrap_or(false);
            if !ok {
                if b.arch == "x86" {
                    x86 = false;
                } else {
                    x64 = false;
                }
                if missing.len() < 12 {
                    missing.push(format!("{}/{}", b.arch, name));
                }
            }
        }
    }
    VcredistStatus {
        installed: x86 && x64,
        x86,
        x64,
        version: VERSION.to_string(),
        missing,
    }
}

// ============================================================
//  التنزيل (نفس سياسة download.rs: HTTPS + نطاق مسموح + بصمة)
// ============================================================

async fn ensure_bundle(b: &Bundle) -> Result<PathBuf, String> {
    let dst = bundle_path(b);
    if bundle_ready(b) {
        return Ok(dst);
    }
    if !crate::download::url_allowed(b.url) {
        return Err("رابط حزمة VC++ غير مسموح".into());
    }
    let resp = reqwest::get(b.url)
        .await
        .map_err(|e| format!("تعذّر تنزيل {}: {e}", b.file))?;
    if !resp.status().is_success() {
        return Err(format!("تنزيل {} رجع {}", b.file, resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("تعذّر قراءة {}: {e}", b.file))?;
    if bytes.len() as u64 != b.size {
        return Err(format!(
            "حجم {} غير متوقع ({} بدل {}) — رفضناه",
            b.file,
            bytes.len(),
            b.size
        ));
    }
    if sha256(&bytes) != b.sha256 {
        return Err(format!("بصمة {} لا تطابق الموثّقة — رفضناه", b.file));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("تعذّر تجهيز مجلد VC++: {e}"))?;
    }
    let tmp = dst.with_extension("exe.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("تعذّر كتابة {}: {e}", b.file))?;
    std::fs::rename(&tmp, &dst).map_err(|e| format!("تعذّر تثبيت {}: {e}", b.file))?;
    Ok(dst)
}

// ============================================================
//  التثبيت الصامت
// ============================================================

/// رمز خروج مثبّت VC++ مقبول؟
fn exit_ok(code: Option<i32>) -> bool {
    matches!(code, Some(0 | 1638 | 3010 | 1641 | 67 | 216))
}

async fn run_installer(wine: &Path, prefix: &Path, b: &Bundle, exe: &Path) -> Result<(), String> {
    // نسخة داخل القارورة: المسار ويندوز (العملية تشتغل داخل Wine)
    let tmp_dir = prefix.join("drive_c").join("jisr-tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("تعذّر تجهيز مجلد مؤقت: {e}"))?;
    let dst = tmp_dir.join(b.file);
    std::fs::copy(exe, &dst).map_err(|e| format!("تعذّر نسخ {}: {e}", b.file))?;
    let win_exe = format!(r"C:\jisr-tmp\{}", b.file);
    let log = format!(r"C:\jisr-tmp\vc-{}.log", b.arch);

    let mut last = String::new();
    for attempt in 0..3 {
        let mut cmd = Command::new(wine);
        cmd.env("WINEPREFIX", prefix)
            .env("WINEMSYNC", "1")
            .env("WINEDEBUG", "-all")
            .arg(&win_exe)
            .arg("/install")
            .arg("/quiet")
            .arg("/norestart")
            .arg("/log")
            .arg(&log)
            .stdin(std::process::Stdio::null());
        let out =
            match tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output()).await {
                Ok(Ok(o)) => o,
                Ok(Err(e)) => return Err(format!("تعذّر تشغيل مثبّت {}: {e}", b.file)),
                Err(_) => return Err(format!("مثبّت {} تجاوز 5 دقائق", b.file)),
            };
        let code = out.status.code();
        if exit_ok(code) {
            crate::debuglog::raw(
                "info",
                "vcredist",
                &format!(
                    "{} installer exit={:?} (attempt {})",
                    b.arch,
                    code,
                    attempt + 1
                ),
            );
            return Ok(());
        }
        last = format!(
            "{} (code={:?}) {}",
            b.file,
            code,
            String::from_utf8_lossy(&out.stderr).trim()
        );
        // 1618 = MSI مشغول — ننتظر ونعيد (التركيب متسلسل، نادراً يصير)
        if code == Some(1618) && attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }
        break;
    }
    Err(format!("مثبّت VC++ فشل: {last}"))
}

// ============================================================
//  إصلاح من حمولة الحزمة (Wine bug 57518)
// ============================================================

fn cab_files<R: Read + std::io::Seek>(cab: &mut cab::Cabinet<R>) -> Vec<String> {
    let mut out = Vec::new();
    for folder in cab.folder_entries() {
        for file in folder.file_entries() {
            out.push(file.name().to_string());
        }
    }
    out
}

/// كل حزم CAB الظاهرة في ملف (بحد أقصى معقول)
fn find_cabs(data: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 16 <= data.len() && out.len() < 32 {
        if &data[i..i + 4] == b"MSCF" {
            let size =
                u32::from_le_bytes([data[i + 8], data[i + 9], data[i + 10], data[i + 11]]) as usize;
            if size > 64 && i + size <= data.len() {
                out.push((i, size));
                i += size;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// حمولة DLLs (كابينيت) لمعمارية معيّنة داخل حزمة Burn
fn payload_bytes(data: &[u8], arch: &str) -> Option<Vec<u8>> {
    let suffix = if arch == "x86" { "_x86" } else { "_amd64" };
    for (off, size) in find_cabs(data) {
        let block = &data[off..off + size];
        let Ok(mut cab) = cab::Cabinet::new(Cursor::new(block)) else {
            continue;
        };
        let names = cab_files(&mut cab);
        if names
            .iter()
            .any(|n| n.to_ascii_lowercase().ends_with(suffix))
        {
            return Some(block.to_vec());
        }
        // الحمولة ممكن تكون داخل كابينيت ثانية (a2/a4)
        for name in names {
            let Ok(mut r) = cab.read_file(&name) else {
                continue;
            };
            let mut inner = Vec::new();
            if r.read_to_end(&mut inner).is_err() || !inner.starts_with(b"MSCF") {
                continue;
            }
            if let Ok(mut c2) = cab::Cabinet::new(Cursor::new(inner.as_slice())) {
                let n2 = cab_files(&mut c2);
                if n2.iter().any(|n| n.to_ascii_lowercase().ends_with(suffix)) {
                    return Some(inner);
                }
            }
        }
    }
    None
}

/// يصلح الملفات الناقصة من حمولة الحزمة — كل ملف متحقَّق الحجم والبصمة.
fn repair_from_bundle(prefix: &Path, b: &Bundle) -> Result<usize, String> {
    let bytes =
        std::fs::read(bundle_path(b)).map_err(|e| format!("تعذّر قراءة حزمة {}: {e}", b.file))?;
    let payload = payload_bytes(&bytes, b.arch)
        .ok_or_else(|| format!("ما لقينا حمولة DLLs داخل {}", b.file))?;
    let mut cab = cab::Cabinet::new(Cursor::new(payload.as_slice()))
        .map_err(|e| format!("تعذّر فك حمولة {}: {e}", b.file))?;
    let names = cab_files(&mut cab);
    let suffix = if b.arch == "x86" { "x86" } else { "amd64" };
    let dir = prefix.join("drive_c").join("windows").join(b.sys_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("تعذّر تجهيز {}: {e}", b.sys_dir))?;

    let mut wrote = 0usize;
    for (name, want_size, want_sha) in b.dlls {
        let entry = format!("{name}_{suffix}");
        let Some(found) = names.iter().find(|n| n.eq_ignore_ascii_case(&entry)) else {
            continue;
        };
        let mut dll = Vec::new();
        if let Ok(mut r) = cab.read_file(found) {
            if r.read_to_end(&mut dll).is_err() {
                continue;
            }
        }
        if dll.len() as u64 != *want_size || sha256(&dll) != *want_sha {
            crate::debuglog::raw(
                "warn",
                "vcredist",
                &format!("{}: حمولة {name} لا تطابق البصمة — تجاهلناها", b.arch),
            );
            continue;
        }
        let dst = dir.join(name);
        let tmp = dir.join(format!("{name}.jisr-tmp"));
        std::fs::write(&tmp, &dll).map_err(|e| format!("تعذّر كتابة {name}: {e}"))?;
        let _ = std::fs::remove_file(&dst);
        std::fs::rename(&tmp, &dst).map_err(|e| format!("تعذّر تثبيت {name}: {e}"))?;
        wrote += 1;
    }
    Ok(wrote)
}

// ============================================================
//  التجاوزات + التجهيز الكامل
// ============================================================

async fn write_overrides(wine: &Path, prefix: &Path) -> Result<(), String> {
    let tmp_dir = prefix.join("drive_c").join("jisr-tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("تعذّر تجهيز مجلد مؤقت: {e}"))?;
    let reg_path = tmp_dir.join("jisr-vc14.reg");
    let mut body =
        String::from("REGEDIT4\r\n\r\n[HKEY_CURRENT_USER\\Software\\Wine\\DllOverrides]\r\n");
    for name in OVERRIDES {
        body.push_str(&format!("\"{name}\"=\"native,builtin\"\r\n"));
    }
    std::fs::write(&reg_path, body).map_err(|e| format!("تعذّر كتابة تجاوزات VC++: {e}"))?;

    let mut cmd = Command::new(wine);
    cmd.env("WINEPREFIX", prefix)
        .env("WINEMSYNC", "1")
        .env("WINEDEBUG", "-all")
        .arg("regedit")
        .arg("/S")
        .arg(r"C:\jisr-tmp\jisr-vc14.reg")
        .stdin(std::process::Stdio::null());
    match tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output()).await {
        Ok(Ok(o)) if o.status.success() => {}
        Ok(Ok(o)) => {
            return Err(format!(
                "regedit لتجاوزات VC++ فشل: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ));
        }
        Ok(Err(e)) => return Err(format!("تعذّر تشغيل regedit: {e}")),
        Err(_) => return Err("regedit تجاوز الوقت".into()),
    }

    // تحقق فعلي — نقرأ المفتاح كله
    let mut cmd = Command::new(wine);
    cmd.env("WINEPREFIX", prefix)
        .arg("reg")
        .arg("query")
        .arg("HKCU\\Software\\Wine\\DllOverrides")
        .stdin(std::process::Stdio::null());
    let out = match tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(o)) => o,
        _ => return Err("تعذّر التحقق من تجاوزات VC++".into()),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    for name in OVERRIDES {
        if !crate::graphics::registry_value_is(&text, name, "native,builtin") {
            return Err(format!("تجاوز {name} ما انكتب"));
        }
    }
    Ok(())
}

static BUSY: AtomicBool = AtomicBool::new(false);

struct BusyGuard;
impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

/// يجهّز VC++ 2015‑2022 x86 + x64 في القارورة الرئيسية:
/// تنزيل (لو ناقص) → تثبيت صامت x86 ثم x64 → إصلاح من الحمولة → تجاوزات.
pub async fn prepare(wine: &Path, prefix: &Path) -> Result<VcredistStatus, String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("تجهيز متطلبات ويندوز شغّال بالفعل".into());
    }
    let _guard = BusyGuard;

    // تنزيل الحزمتين أولاً (قبل أي تشغيل لستيم)
    for b in &BUNDLES {
        ensure_bundle(b).await?;
    }

    let status = verify(prefix);
    if status.installed {
        // مثبّتة فعلاً — نتأكد فقط من التجاوزات
        if let Err(e) = write_overrides(wine, prefix).await {
            crate::debuglog::raw("warn", "vcredist", &format!("overrides: {e}"));
        }
        return Ok(status);
    }

    // x86 ثم x64 — بالترتيب (قفل MSI عالمي)
    let mut first_error: Option<String> = None;
    for b in &BUNDLES {
        if let Err(e) = run_installer(wine, prefix, b, &bundle_path(b)).await {
            crate::debuglog::raw("warn", "vcredist", &format!("install {}: {e}", b.arch));
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
        // مهما كانت نتيجة المثبّت: نصلح الناقص من الحمولة
        match repair_from_bundle(prefix, b) {
            Ok(n) if n > 0 => crate::debuglog::raw(
                "info",
                "vcredist",
                &format!("{} repaired {n} DLLs from payload", b.arch),
            ),
            Ok(_) => {}
            Err(e) => crate::debuglog::raw("warn", "vcredist", &format!("repair {}: {e}", b.arch)),
        }
    }

    if let Err(e) = write_overrides(wine, prefix).await {
        crate::debuglog::raw("warn", "vcredist", &format!("overrides: {e}"));
    }

    let status = verify(prefix);
    if status.installed {
        Ok(status)
    } else {
        let missing = if status.missing.is_empty() {
            "ملفات ناقصة".to_string()
        } else {
            status.missing.join(", ")
        };
        Err(first_error.unwrap_or_else(|| format!("ناقص: {missing}")))
    }
}

// ============================================================
//  أوامر Tauri
// ============================================================

#[tauri::command]
pub fn vcredist_status() -> VcredistStatus {
    verify(&crate::bottle::bottle_dir(crate::bottle::MAIN_ID))
}

/// يجهّز الحزمتين داخل القارورة. الواجهة تعرض «جاري التجهيز» أثناءه.
#[tauri::command]
pub async fn vcredist_prepare(app: tauri::AppHandle) -> Result<VcredistStatus, String> {
    let _t = crate::debuglog::trace("vcredist", "vcredist_prepare", "");
    let prefix = crate::bottle::bottle_dir(crate::bottle::MAIN_ID);
    let wine = crate::bottle::wine_bin().ok_or_else(|| "ما لقينا محرّك Wine".to_string())?;
    let st = prepare(&wine, &prefix).await?;
    crate::debuglog::raw(
        "info",
        "vcredist",
        &format!(
            "vc++ ready version={} x86={} x64={} missing={:?}",
            st.version, st.x86, st.x64, st.missing
        ),
    );
    crate::log(
        &app,
        "info",
        crate::log_keys::GRAPHICS_READY,
        serde_json::json!({ "vcredist": st.version, "x86": st.x86, "x64": st.x64 }),
    );
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_are_pinned_sanely() {
        for b in &BUNDLES {
            assert!(b.size > 1_000_000);
            assert_eq!(b.sha256.len(), 64);
            assert!(b.url.starts_with("https://"));
            assert!(b.file.ends_with(".exe"));
            assert!(!b.dlls.is_empty());
            for (name, size, sha) in b.dlls {
                assert!(name.ends_with(".dll"));
                assert!(*size > 10_000);
                assert_eq!(sha.len(), 64);
            }
        }
    }

    #[test]
    fn overrides_never_touch_ucrt() {
        assert!(!OVERRIDES.iter().any(|n| n.contains("ucrt")));
        assert!(OVERRIDES.contains(&"msvcp140"));
        assert!(OVERRIDES.contains(&"vcruntime140"));
        assert!(OVERRIDES.contains(&"vcruntime140_1"));
    }

    #[test]
    fn verify_reports_missing_on_empty_prefix() {
        let dir = std::env::temp_dir().join(format!("jisr-vc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("drive_c/windows/syswow64")).unwrap();
        std::fs::create_dir_all(dir.join("drive_c/windows/system32")).unwrap();
        let st = verify(&dir);
        assert!(!st.installed);
        assert!(!st.x86);
        assert!(!st.x64);
        assert!(st.missing.iter().any(|m| m.starts_with("x86/")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exit_codes_match_the_msi_contract() {
        for code in [0, 1638, 3010, 1641, 67, 216] {
            assert!(exit_ok(Some(code)), "{code}");
        }
        assert!(!exit_ok(Some(1603)));
        assert!(!exit_ok(None));
    }
}
