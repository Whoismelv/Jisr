// ============================================================
//  جسر — D3DX9 الأصلي (إصلاح ألعاب DirectX 9 القديمة)
// ============================================================
//! **المشكلة الميدانية:** ألعاب D3D9 القديمة (LIMBO مثلًا) تستدعي
//! `d3dx9_43.dll` → `d3dcompiler_43.dll` لتجميع الـshaders وقت التشغيل.
//! نسخ Wine المدمجة من هالمكتبتين تنهار على ماك:
//!
//! ```text
//! ucrtbase → wined3d → d3dcompiler_43 → d3dx9_43 → limbo_demo
//! Unhandled exception: page fault on read access to 0x00000000
//! ```
//!
//! **الحل (نفس ما تفعله الحزم المرجعية):** نستخدم نسخة مايكروسوفت
//! الأصلية. ننزّل حزمة DirectX End-User Runtime (Jun2010) من سيرفر
//! مايكروسوفت الرسمي، نستخرج منها `d3dx9_43.dll` و`D3DCompiler_43.dll`
//! (x86 + x64) **بلا أي أداة خارجية** (فك CAB داخلي)، ونتحقق من بصمة كل
//! ملف قبل كتابته، ثم ننشرها في القارورة مع تجاوز `native`.
//!
//! • ما نعيد توزيع أي ملف من مايكروسوفت: التنزيل يصير عند المستخدم.
//! • نتحقق من بصمة الحزمة كاملة (100MB) **و** بصمة كل DLL مستخرجة.
//! • كل شي قابل للتراجع: نمسح التجاوزات ونرجع لنسخ Wine.
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

/// معرّف الأصل في كتالوج التنزيل
pub const ASSET_ID: &str = "d3dx9-jun2010";

/// الملفات المطلوبة: (مجلد القارورة، اسم الملف، حجمه، بصمته)
/// البصمات محسوبة من حزمة مايكروسوفت الرسمية (sha256 الحزمة مثبّت في الكتالوج).
const TARGETS: [(&str, &str, u64, &str); 4] = [
    (
        "x86",
        "d3dx9_43.dll",
        1_998_168,
        "0b28546be22c71834501f7d7185ede5d79742457331c7ee09efc14490dd64f5f",
    ),
    (
        "x86",
        "D3DCompiler_43.dll",
        2_106_216,
        "2f23182ec6f4889397ac4bf03d62536136c5bdba825c7d2c4ef08c827f3a8a1c",
    ),
    (
        "x64",
        "d3dx9_43.dll",
        2_401_112,
        "84b900dbd7fa978d6e0caee26fc54f2f61d92c9c75d10b35f00e3e82cd1d67b4",
    ),
    (
        "x64",
        "D3DCompiler_43.dll",
        2_526_056,
        "44c3a7e330b54a35a9efa015831392593aa02e7da1460be429d17c3644850e8a",
    ),
];

/// أسماء حزم CAB الداخلية (بلا حساسية لحالة الأحرف)
fn cab_name(dir: &str, dll: &str) -> &'static str {
    // d3dx9_43.dll → Jun2010_d3dx9_43_x86.cab | D3DCompiler_43.dll → Jun2010_D3DCompiler_43_x86.cab
    match (dir, dll) {
        ("x86", "d3dx9_43.dll") => "Jun2010_d3dx9_43_x86.cab",
        ("x64", "d3dx9_43.dll") => "Jun2010_d3dx9_43_x64.cab",
        ("x86", _) => "Jun2010_D3DCompiler_43_x86.cab",
        _ => "Jun2010_D3DCompiler_43_x64.cab",
    }
}

pub fn install_dir() -> PathBuf {
    crate::data_dir().join("graphics").join("d3dx9")
}

fn marker_path(prefix: &Path) -> PathBuf {
    prefix.join(".jisr-d3dx9.ok")
}

/// جاهز في مجلد التطبيق؟ (الملفات الأربعة بأحجامها الصحيحة)
pub fn is_installed() -> bool {
    let dir = install_dir();
    TARGETS.iter().all(|(arch, file, size, _)| {
        std::fs::metadata(dir.join(arch).join(file))
            .map(|m| m.len() == *size)
            .unwrap_or(false)
    })
}

/// منشور داخل القارورة؟ (بلا تشغيل Wine — فحص ملفات فقط)
pub fn deployed_in(prefix: &Path) -> bool {
    let win = prefix.join("drive_c").join("windows");
    win.join("syswow64").join("d3dx9_43.dll").is_file()
        && win.join("syswow64").join("D3DCompiler_43.dll").is_file()
        && win.join("system32").join("d3dx9_43.dll").is_file()
        && win.join("system32").join("D3DCompiler_43.dll").is_file()
}

fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// أسماء الملفات داخل حزمة CAB
fn cab_files<R: Read + std::io::Seek>(cab: &mut cab::Cabinet<R>) -> Vec<String> {
    let mut out = Vec::new();
    for folder in cab.folder_entries() {
        for file in folder.file_entries() {
            out.push(file.name().to_string());
        }
    }
    out
}

/// أول حزمة CAB خارجية تحتوي D3DX9 (داخل ملف Microsoft ذاتي الاستخراج)
fn find_outer_cab(data: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0usize;
    while i + 16 <= data.len() {
        if &data[i..i + 4] == b"MSCF" {
            let size =
                u32::from_le_bytes([data[i + 8], data[i + 9], data[i + 10], data[i + 11]]) as usize;
            if size > 64 && i + size <= data.len() {
                if let Ok(mut cab) = cab::Cabinet::new(Cursor::new(&data[i..i + size])) {
                    let has = cab_files(&mut cab)
                        .iter()
                        .any(|n| n.to_ascii_lowercase().contains("d3dx9_43"));
                    if has {
                        return Some((i, size));
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// يستخرج ملفات D3DX9 من حزمة مايكروسوفت (حصرياً صحيح البصمة).
/// يرجع عدد الملفات المكتوبة.
pub fn extract_from_redist(redist: &Path) -> Result<usize, String> {
    let data = std::fs::read(redist).map_err(|e| format!("تعذّر قراءة حزمة DirectX: {e}"))?;
    let (off, size) =
        find_outer_cab(&data).ok_or_else(|| "حزمة DirectX ما تحتوي D3DX9 المتوقع".to_string())?;

    let mut outer = cab::Cabinet::new(Cursor::new(&data[off..off + size]))
        .map_err(|e| format!("تعذّر فك حزمة DirectX: {e}"))?;
    let outer_names = cab_files(&mut outer);

    let mut written = 0usize;
    for (arch, file, want_size, want_sha) in TARGETS {
        let want_cab = cab_name(arch, file);
        let Some(entry) = outer_names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(want_cab))
        else {
            return Err(format!("ما لقينا {want_cab} داخل حزمة DirectX"));
        };

        let mut inner_bytes = Vec::new();
        outer
            .read_file(entry)
            .map_err(|e| format!("تعذّر قراءة {want_cab}: {e}"))?
            .read_to_end(&mut inner_bytes)
            .map_err(|e| format!("تعذّر قراءة {want_cab}: {e}"))?;

        let mut inner = cab::Cabinet::new(Cursor::new(inner_bytes.as_slice()))
            .map_err(|e| format!("تعذّر فك {want_cab}: {e}"))?;
        let Some(dll_entry) = cab_files(&mut inner)
            .into_iter()
            .find(|n| n.eq_ignore_ascii_case(file))
        else {
            return Err(format!("ما لقينا {file} داخل {want_cab}"));
        };

        let mut dll = Vec::new();
        inner
            .read_file(&dll_entry)
            .map_err(|e| format!("تعذّر قراءة {file}: {e}"))?
            .read_to_end(&mut dll)
            .map_err(|e| format!("تعذّر قراءة {file}: {e}"))?;

        // حارس سلسلة التوريد: الحجم ثم البصمة — أي اختلاف = رفض
        if dll.len() as u64 != want_size {
            return Err(format!(
                "حجم {file} غير متوقع ({} بدل {want_size}) — رفضناه",
                dll.len()
            ));
        }
        if !sha256(&dll).eq_ignore_ascii_case(want_sha) {
            return Err(format!("بصمة {file} لا تطابق البصمة الموثّقة — رفضناه"));
        }

        let dst_dir = install_dir().join(arch);
        std::fs::create_dir_all(&dst_dir).map_err(|e| format!("تعذّر تجهيز مجلد D3DX9: {e}"))?;
        let tmp = dst_dir.join(format!("{file}.tmp"));
        std::fs::write(&tmp, &dll).map_err(|e| format!("تعذّر كتابة {file}: {e}"))?;
        std::fs::rename(&tmp, dst_dir.join(file)).map_err(|e| format!("تعذّر تثبيت {file}: {e}"))?;
        written += 1;
    }

    let _ = std::fs::write(install_dir().join("version.txt"), b"Jun2010\n");
    Ok(written)
}

/// كتابة تجاوزات `native` في ريجستري القارورة
async fn apply_registry_overrides(wine: &Path, prefix: &Path) -> Result<(), String> {
    for dll in ["d3dx9_43", "d3dcompiler_43"] {
        let mut cmd = tokio::process::Command::new(wine);
        cmd.env("WINEPREFIX", prefix)
            .arg("reg")
            .arg("add")
            .arg("HKCU\\Software\\Wine\\DllOverrides")
            .arg("/v")
            .arg(dll)
            .arg("/t")
            .arg("REG_SZ")
            .arg("/d")
            .arg("native")
            .arg("/f")
            .stdin(std::process::Stdio::null());
        let out = match tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await
        {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(format!("reg add {dll} فشل: {e}")),
            Err(_) => return Err(format!("reg add {dll} تجاوز الوقت")),
        };
        if !out.status.success() {
            return Err(format!(
                "reg add {dll} فشل: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    // تحقق بالقراءة — نعرف فعلياً إن التجاوزات مكتوبة
    for dll in ["d3dx9_43", "d3dcompiler_43"] {
        let mut cmd = tokio::process::Command::new(wine);
        cmd.env("WINEPREFIX", prefix)
            .arg("reg")
            .arg("query")
            .arg("HKCU\\Software\\Wine\\DllOverrides")
            .arg("/v")
            .arg(dll)
            .stdin(std::process::Stdio::null());
        let out = match tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await
        {
            Ok(Ok(o)) => o,
            _ => return Err(format!("تعذّر التحقق من تجاوز {dll}")),
        };
        if !String::from_utf8_lossy(&out.stdout)
            .to_lowercase()
            .contains("native")
        {
            return Err(format!("تجاوز {dll} ما انكتب في الريجستري"));
        }
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.len() == y.len() && x.len() > 0,
        _ => false,
    }
}

/// علامة «الحماية مفعّلة» داخل القارورة — **تعيش بعد إعادة التشغيل**،
/// و`deploy_files` يحترمها فلا يكتب المترجم الأصلي فوق غلافنا.
///
/// **درس ميداني:** بدون هذه العلامة كان `deploy_files()` (يعمل قبل كل تشغيل)
/// ينسخ `D3DCompiler_43.dll` فوق الغلاف ⇒ الحماية تُلغى في نفس اللحظة
/// واللعبة ترجع تنهار (والواجهة تقول «غير مفعّلة» بصدق).
fn guard_marker(prefix: &Path) -> PathBuf {
    prefix.join(".jisr-d3dx-guard.ok")
}

/// هل الحماية مفعّلة لهذه القارورة؟
///
/// صارت **دائمة**: `deploy_files` يثبّتها قبل كل تشغيل، والعلامة تُكتب
/// تلقائياً. ما فيه مفتاح يطفيها — لأن إطفاءها يرجّع انهيار `pTarget=NULL`.
pub fn guard_enabled(prefix: &Path) -> bool {
    guard_marker(prefix).is_file() || shim_installed(prefix)
}

/// يضمن أن `d3dcompiler_43.dll` في المجلد المعطى هو **غلافنا**،
/// ويحفظ الأصل بجانبه (`d3dcompiler_43_ms.dll`) مرة واحدة.
fn ensure_guard_file(dir: &Path, bytes: &[u8]) -> Result<(), String> {
    let target = dir.join("d3dcompiler_43.dll");
    let real = dir.join("d3dcompiler_43_ms.dll");
    // غلاف أحدث؟ نستبدله دائماً (التطبيق يتحدّث تلقائياً، والغلاف القديم
    // كان يبقى للأبد لأن الفحص كان «هل هو غلاف؟» بدل «هل هو هذا الغلاف؟»).
    if let Ok(existing) = std::fs::read(&target) {
        if existing == bytes {
            return Ok(());
        }
        if is_shim(&target) {
            let tmp = dir.join("d3dcompiler_43.dll.jisr-tmp");
            std::fs::write(&tmp, bytes).map_err(|e| format!("تعذّر كتابة الغلاف: {e}"))?;
            let _ = std::fs::remove_file(&target);
            std::fs::rename(&tmp, &target).map_err(|e| format!("تعذّر تثبيت الغلاف: {e}"))?;
            if is_shim(&target) {
                return Ok(());
            }
        }
    }
    // نحفظ الأصل: الملف نفسه، أو النسخة ذات الأحرف الكبيرة (D3DCompiler_43.dll)
    if !real.is_file() {
        let candidates = [target.clone(), dir.join("D3DCompiler_43.dll")];
        for src in candidates.iter().filter(|p| p.is_file()) {
            if !is_shim(src) {
                std::fs::copy(src, &real)
                    .map_err(|e| format!("تعذّر حفظ نسخة المترجم الأصلية: {e}"))?;
                break;
            }
        }
    }
    if !real.is_file() {
        return Err("ما لقينا المترجم الأصلي — ثبّت D3DX9 أولاً".into());
    }
    let tmp = dir.join("d3dcompiler_43.dll.jisr-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("تعذّر كتابة الغلاف: {e}"))?;
    let _ = std::fs::remove_file(&target);
    std::fs::rename(&tmp, &target).map_err(|e| format!("تعذّر تثبيت الغلاف: {e}"))?;
    // تحقّق: لو الملف ما انكتب (أو ما فيه البصمة) نفشل بصوت عالٍ
    if !is_shim(&target) {
        return Err("فشل التحقّق: الغلاف ما انكتب (صلاحيات؟)".into());
    }
    Ok(())
}

/// ينشر ملفات D3DX9 داخل القارورة (system32 / syswow64) — **بلا ريجستري**.
///
/// لماذا منفصلة عن التجاوزات؟ لأن **ملف تعريف اللعبة** هو اللي يقرر
/// native/builtin — فلو كتبنا هنا native دائماً لكانت الطبقة تتخطّى قرار الملف.
///
/// **الحماية دائمة:** `D3DCompiler_43.dll` ما يُنسخ الأصلي أبداً — نكتب
/// غلافنا الحارس بدلاً منه قبل كل تشغيل (إصلاح ذاتي). لو ما لقينا الأصل
/// (قارورة ناقصة) نكمل النشر العادي بدل ما نفشل التشغيل كله.
pub fn deploy_files(prefix: &Path) -> Result<(), String> {
    if !is_installed() {
        return Err("ملفات D3DX9 مو مثبّتة بعد".into());
    }
    let win = prefix.join("drive_c").join("windows");
    let mut guard_ok = true;
    for (arch, sys_dir) in [("x86", "syswow64"), ("x64", "system32")] {
        let dst_dir = win.join(sys_dir);
        if !dst_dir.is_dir() {
            return Err("القارورة مو جاهزة — نكمل النشر لاحقاً".into());
        }
        for (_, file, _, _) in TARGETS.iter().filter(|(a, _, _, _)| *a == arch) {
            if file.eq_ignore_ascii_case("d3dcompiler_43.dll") {
                let bytes = if arch == "x86" { SHIM_X86 } else { SHIM_X64 };
                if ensure_guard_file(&dst_dir, bytes).is_err() {
                    // أول تشغيل: ما فيه أصل بعد ⇒ ننشر نسخة مايكروسوفت،
                    // ثم نثبّت الحارس فوقها فوراً (مهم: الحماية من أول تشغيل)
                    let src = install_dir().join(arch).join(file);
                    let dst = dst_dir.join(file);
                    if src.is_file() && !same_file(&src, &dst) {
                        let _ = std::fs::copy(&src, &dst);
                    }
                    if ensure_guard_file(&dst_dir, bytes).is_err() {
                        guard_ok = false;
                    }
                }
                continue;
            }
            let src = install_dir().join(arch).join(file);
            let dst = dst_dir.join(file);
            if same_file(&src, &dst) {
                continue;
            }
            std::fs::copy(&src, &dst).map_err(|e| format!("تعذّر نشر {file}: {e}"))?;
        }
    }
    if guard_ok {
        let _ = std::fs::write(guard_marker(prefix), b"1\n");
    }
    Ok(())
}

/// ينشر DLL داخل القارورة ثم يكتب التجاوزات (مرة واحدة — بعلامة جاهزية)
pub async fn deploy_to_prefix(wine: &Path, prefix: &Path) -> Result<(), String> {
    deploy_files(prefix)?;
    apply_registry_overrides(wine, prefix).await?;
    let _ = std::fs::write(marker_path(prefix), b"ok\n");
    Ok(())
}

// ============================================================
//  غلاف التشخيص (Microscope) لمترجم الـHLSL
// ============================================================

/// غلاف 32-بت (و64-بت) يجلس بين اللعبة و`d3dcompiler_43` الحقيقي:
/// يسجّل كل استدعاء، يمنع القفزة على مدخل فارغ، ويقدر يوجّه الاستدعاء
/// لمترجم آخر (بديل) عبر `JISR_D3DCOMPILER_BACKEND`.
///
/// **لماذا:** ألعاب DX9 القديمة تنهار داخل المترجم نفسه (page fault) —
/// ونفس الانهيار يتكرّر مع مترجم Wine ⇒ إذن المشكلة في **الاستدعاء**
/// أو في المترجم تحت WoW64. هذا الغلاف يخبرنا أيّهما بالضبط.
const SHIM_X86: &[u8] = include_bytes!("../assets/d3dcompiler_43_x86.dll");
const SHIM_X64: &[u8] = include_bytes!("../assets/d3dcompiler_43_x64.dll");

/// بصمة داخل الغلاف — نعرف بها أنه غلافنا (لا نلمس الأصلي)
const SHIM_MARKER: &[u8] = b"JISR_D3DX_SHIM_V1";

/// مسار سجل التشخيص **داخل القارورة** (الغلاف عملية ويندوز: المسار لازم
/// يكون `C:\...` وإلا الكتابة تفشل بصمت وتبقى الواجهة بلا سجل).
pub fn shim_log_path_in(prefix: &Path) -> PathBuf {
    prefix.join("drive_c").join("jisr-d3dx-shim.log")
}

/// المسار اللي نمرّره للعبة (ويندوز)
pub const SHIM_LOG_WIN_PATH: &str = r"C:\jisr-d3dx-shim.log";

/// مسار السجل المحلي (للتوثيق/التصدير)
pub fn shim_log_path() -> PathBuf {
    crate::data_dir().join("logs").join("d3dx-shim.log")
}

/// يدوّر سجل الحارس **قبل كل تشغيل** — كل جلسة لها سجلها، وما نخلط
/// سجل تشغيل فاشل مع تشغيل ناجح. نحتفظ بجلسة واحدة سابقة فقط.
pub fn rotate_shim_log(prefix: &Path) {
    let current = shim_log_path_in(prefix);
    if !current.is_file() {
        return;
    }
    let old = prefix.join("drive_c").join("jisr-d3dx-shim.1.log");
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::rename(&current, &old);
}

/// هل هذه البايتات غلاف جسر؟ (للتصدير والتشخيص)
pub fn bytes_are_shim(bytes: &[u8]) -> bool {
    bytes.windows(SHIM_MARKER.len()).any(|w| w == SHIM_MARKER)
}

/// هل الملف المعطى هو غلافنا؟
fn is_shim(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes_are_shim(&bytes)
}

/// هل الغلاف مثبّت داخل القارورة؟
pub fn shim_installed(prefix: &Path) -> bool {
    let win = prefix.join("drive_c").join("windows");
    is_shim(&win.join("syswow64").join("d3dcompiler_43.dll"))
}

/// يثبّت الغلاف: نحفظ الأصل بجانبه (`d3dcompiler_43_ms.dll`) ونضع غلافنا مكانه.
///
/// الأصل **لا يُحذف** ⇒ الرجوع سهل (نرجّعه مكانه).
pub async fn install_shim(wine: &Path, prefix: &Path) -> Result<(), String> {
    let win = prefix.join("drive_c").join("windows");
    for (sys_dir, bytes) in [("syswow64", SHIM_X86), ("system32", SHIM_X64)] {
        let dir = win.join(sys_dir);
        if !dir.is_dir() {
            return Err("القارورة مو جاهزة".into());
        }
        ensure_guard_file(&dir, bytes)?;
    }
    // علامة دائمة: بعدها `deploy_files` ما يكتب الأصل فوق الغلاف أبداً
    std::fs::write(guard_marker(prefix), b"1\n")
        .map_err(|e| format!("تعذّر حفظ علامة الحماية: {e}"))?;

    // التحويل لـnative (الغلاف أصلاً —原件) ونكتب مسار السجل للعبة
    let mut cmd = tokio::process::Command::new(wine);
    cmd.env("WINEPREFIX", prefix)
        .env("WINEMSYNC", "1")
        .arg("reg")
        .arg("add")
        .arg(r"HKCU\Software\Wine\DllOverrides")
        .arg("/v")
        .arg("d3dcompiler_43")
        .arg("/t")
        .arg("REG_SZ")
        .arg("/d")
        .arg("native")
        .arg("/f")
        .stdin(std::process::Stdio::null());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await;
    Ok(())
}

/// يرجّع المترجم الأصلي مكان الغلاف (إلغاء التشخيص)
pub fn remove_shim(prefix: &Path) -> Result<(), String> {
    let win = prefix.join("drive_c").join("windows");
    for sys_dir in ["syswow64", "system32"] {
        let dir = win.join(sys_dir);
        let target = dir.join("d3dcompiler_43.dll");
        let real = dir.join("d3dcompiler_43_ms.dll");
        if is_shim(&target) && real.is_file() {
            let _ = std::fs::remove_file(&target);
            std::fs::rename(&real, &target)
                .map_err(|e| format!("تعذّر استرجاع المترجم الأصلي: {e}"))?;
        }
    }
    // نلغي العلامة ⇒ `deploy_files` يرجع يكتب الأصل بشكل طبيعي
    let _ = std::fs::remove_file(guard_marker(prefix));
    Ok(())
}

/// يثبّت غلاف التشخيص داخل القارورة (مِجهر + مِطرقة لمترجم الـHLSL)
#[tauri::command]
pub async fn d3dx_shim_install() -> Result<bool, String> {
    let _trace = crate::debuglog::trace("d3dx", "d3dx_shim_install", "");
    let prefix = crate::bottle::bottle_dir(crate::bottle::MAIN_ID);
    let Some(wine) = crate::bottle::wine_bin() else {
        return Err("ما لقينا محرّك Wine".into());
    };
    install_shim(&wine, &prefix).await?;
    crate::debuglog::raw(
        "info",
        "d3dx",
        &format!("d3dx shim installed (log: {})", shim_log_path().display()),
    );
    Ok(true)
}

/// يرجّع المترجم الأصلي (إلغاء التشخيص)
#[tauri::command]
pub fn d3dx_shim_remove() -> Result<bool, String> {
    let _trace = crate::debuglog::trace("d3dx", "d3dx_shim_remove", "");
    let prefix = crate::bottle::bottle_dir(crate::bottle::MAIN_ID);
    remove_shim(&prefix)?;
    Ok(true)
}

/// حالة غلاف التشخيص + آخر أسطر السجل (حتى نقرأها من الواجهة)
#[tauri::command]
pub fn d3dx_shim_status() -> serde_json::Value {
    let prefix = crate::bottle::bottle_dir(crate::bottle::MAIN_ID);
    let log = shim_log_path_in(&prefix);
    // سقف 1 MiB: لا نترك سجل الغلاف يكبر بلا حد
    if let Ok(md) = std::fs::metadata(&log) {
        if md.len() > 1_048_576 {
            let _ = std::fs::remove_file(&log);
        }
    }
    let tail = std::fs::read_to_string(&log)
        .map(|s| {
            s.lines()
                .rev()
                .take(40)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    serde_json::json!({
        // enabled = المستخدم فعّلها (علامة دائمة) — هذا اللي تعرضه الواجهة
        "enabled": guard_enabled(&prefix),
        // installed = الملف الحالي هو غلافنا فعلاً (قد يُعاد تركيبه قبل التشغيل)
        "installed": shim_installed(&prefix),
        "log": log.display().to_string(),
        "tail": tail,
    })
}

/// حالة الطبقة للواجهة
#[derive(Serialize)]
pub struct D3dxStatus {
    pub installed: bool,
    pub deployed: bool,
}

#[tauri::command]
pub fn d3dx_status() -> D3dxStatus {
    let prefix = crate::bottle::bottle_dir(crate::bottle::MAIN_ID);
    D3dxStatus {
        installed: is_installed(),
        deployed: deployed_in(&prefix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// حارس سلسلة التوريد: بصمة غلاف التشخيص (x86 + x64) مثبّتة.
    /// أي تغيير في الـDLL لازم يتحدّث هنا (ونعيد البناء من tools/d3dx-shim).
    #[test]
    fn shim_assets_are_pinned() {
        use sha2::{Digest, Sha256};
        let mut h86 = Sha256::new();
        h86.update(SHIM_X86);
        assert_eq!(
            format!("{:x}", h86.finalize()),
            "ab64971ba62e8056a55eefcb5af1754f331fc9505d0e74d758b10044cdaaa3ec",
            "تغيّر غلاف D3DX (x86) — أعد البناء من tools/d3dx-shim وحدّث البصمة"
        );
        let mut h64 = Sha256::new();
        h64.update(SHIM_X64);
        assert_eq!(
            format!("{:x}", h64.finalize()),
            "8baffbb0f3be0cfaf4007c89259d3adebe2c4fc7997be5cf03c0b00920576e72",
            "تغيّر غلاف D3DX (x64) — أعد البناء من tools/d3dx-shim وحدّث البصمة"
        );
    }

    #[test]
    fn targets_are_unique_and_named_sanely() {
        let mut seen = std::collections::HashSet::new();
        for (arch, file, size, sha) in TARGETS {
            assert!(seen.insert((arch, file)));
            assert!(size > 500_000);
            assert_eq!(sha.len(), 64);
            assert!(file.ends_with(".dll"));
            assert!(matches!(arch, "x86" | "x64"));
        }
    }

    #[test]
    fn cab_names_match_targets() {
        assert_eq!(cab_name("x86", "d3dx9_43.dll"), "Jun2010_d3dx9_43_x86.cab");
        assert_eq!(cab_name("x64", "d3dx9_43.dll"), "Jun2010_d3dx9_43_x64.cab");
        assert_eq!(
            cab_name("x86", "D3DCompiler_43.dll"),
            "Jun2010_D3DCompiler_43_x86.cab"
        );
        assert_eq!(
            cab_name("x64", "D3DCompiler_43.dll"),
            "Jun2010_D3DCompiler_43_x64.cab"
        );
    }

    #[test]
    fn rejects_file_without_cab() {
        let dir = std::env::temp_dir().join(format!("jisr-d3dx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.exe");
        std::fs::write(&fake, b"MZ not a cabinet").unwrap();
        assert!(extract_from_redist(&fake).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
