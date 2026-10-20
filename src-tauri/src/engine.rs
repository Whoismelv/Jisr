// ============================================================
//  جسر — دورة حياة المحرك (ثقة صفرية بالأرشيفات)
//  • فك tar.xz بأمان: رفض المسارات المطلقة و".." (هجمات Zip-Slip)
//  • الروابط الرمزية مسموحة فقط لو بقيت داخل مجلد الفك
//  • سقوف حماية: حد أقصى لعدد الملفات والحجم الكلي
//  • تركيب ذرّي: فك في مجلد مؤقت → تحقق من البنية → نقل للمكان النهائي
//    (لو صار أي خطأ ما تبقى عندنا محرك "نص-نص")
// ============================================================
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::engines_dir;

/// سقف عدد الملفات داخل الأرشيف — حماية من "قنبلة ملفات"
const MAX_FILES: u64 = 300_000;
/// سقف الحجم الكلي بعد الفك (3GB) — حماية من "قنبلة ضغط"
const MAX_TOTAL: u64 = 3 * 1024 * 1024 * 1024;
/// أقصى عدد مكوّنات لمسار الملف
const MAX_DEPTH: i64 = 40;

/// نتيجة تركيب ناجحة
pub struct Installed {
    pub path: String,
    pub files: u64,
    pub bytes: u64,
    pub skipped: u64, // روابط تخطّيناها (خبيثة أو غير مدعومة على ويندوز)
}

/// خطأ الفك: إما إلغاء المستخدم (وقفة نظيفة) أو فشل حقيقي
#[derive(Debug)]
pub enum ExtErr {
    Cancelled,
    Failed(String),
}

// ============================================================
//  المسارات
// ============================================================

/// مجلد التركيب النهائي للمحرك: engines/<id>
pub fn install_dir(id: &str) -> PathBuf {
    engines_dir().join(id)
}

/// مجلد الفك المؤقت: engines/staging-<id> (يُمسح بعد كل محاولة)
pub fn staging_dir(id: &str) -> PathBuf {
    engines_dir().join(format!("staging-{id}"))
}

/// اسم ملف آمن من رابط: أحرف إنجليزية/أرقام/نقاط/شرطات فقط
pub fn safe_file_name(url: &str) -> Option<String> {
    let name = url.rsplit('/').next()?.to_string();
    let ok = !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() < 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    ok.then_some(name)
}

/// مسار أرشيف المحرك المنزّل: engines/downloads/<اسم الملف>
pub fn archive_path(asset_url: &str) -> Option<PathBuf> {
    safe_file_name(asset_url).map(|n| engines_dir().join("downloads").join(n))
}

// ============================================================
//  الفحص والتعرّف
// ============================================================

/// هل فيه ملف تشغيل Wine داخل هذا المجلد؟ (bin/wine أو bin/wine64)
fn has_wine_bin(dir: &Path) -> bool {
    ["wine", "wine64"]
        .iter()
        .any(|exe| std::fs::symlink_metadata(dir.join("bin").join(exe)).is_ok())
}

/// نبحث عن جذر المحرك داخل مجلد — قد يكون عميقاً
/// (مثال حقيقي: Wine Devel.app/Contents/Resources/wine) — لحد عمق 8 مستويات.
/// نتبع المجلدات الحقيقية فقط (ما نتبع الروابط — لا دورات لا نهائية)
fn find_root(dir: &Path) -> Option<PathBuf> {
    fn walk(d: &Path, depth: usize) -> Option<PathBuf> {
        if has_wine_bin(d) {
            return Some(d.to_path_buf());
        }
        if depth == 0 {
            return None;
        }
        let rd = std::fs::read_dir(d).ok()?;
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(found) = walk(&e.path(), depth - 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(dir, 8)
}

/// هل المحرك مثبّت وجاهز؟
pub fn is_installed(id: &str) -> bool {
    find_root(&install_dir(id)).is_some()
}

/// مسار ملف تشغيل Wine داخل محرك مثبّت (bin/wine ثم bin/wine64)
pub fn wine_bin(engine_id: &str) -> Option<PathBuf> {
    let root = if engine_id == EXTERNAL_ENGINE_ID {
        external_engine_root()?
    } else {
        find_root(&install_dir(engine_id))?
    };
    for exe in ["wine", "wine64"] {
        let p = root.join("bin").join(exe);
        if std::fs::symlink_metadata(&p).is_ok() {
            return Some(p);
        }
    }
    None
}

/// معرّف «المحرك الخارجي» — محرك جاهز عند المستخدم (مثل Whisky) نستخدمه كما هو.
///
/// **لماذا:** محرّك WhiskyWine نسل CodeWeavers (فيه رقعة `winemac` اللي
/// تخلي نافذة ستيم تُرسم عبر Metal، وDXMT/D3DMetal معه). إحنا ما نوزّع
/// GPL ولا ملفات Apple المغلقة — المستخدم يجيب المحرك بنفسه، والتطبيق
/// يكتشفه ويستخدمه.
pub const EXTERNAL_ENGINE_ID: &str = "external";

/// ملف ي��فظ مسار المحرك الخارجي
fn external_path() -> PathBuf {
    engines_dir().join("external-engine.txt")
}

/// مسار المحرك الخارجي (المحفوظ) — None لو ما حدده المستخدم
pub fn external_engine_root() -> Option<PathBuf> {
    let text = std::fs::read_to_string(external_path()).ok()?;
    let raw = text.trim();
    if raw.is_empty() {
        return None;
    }
    let p = PathBuf::from(raw);
    if p.is_dir() { Some(p) } else { None }
}

/// يحفظ مسار المحرك الخارجي (يقبل جذر التطبيق مثل
/// `/Applications/Whisky.app/Contents/Resources/WhiskyWine` أو أي مجلد فيه `bin/wine`).
pub fn set_external_engine(path: &Path) -> Result<String, String> {
    // نقبل الجذر مباشرة أو مجلداً فيه bin/wine
    let root =
        if path.join("bin").join("wine").is_file() || path.join("bin").join("wine64").is_file() {
            path.to_path_buf()
        } else {
            // نبحث عن جذر Wine داخل المسار (عمق 6)
            match find_root(path) {
                Some(r) => r,
                None => {
                    return Err("ما لقينا `bin/wine` داخل هذا المسار — اختر مجلد Wine نفسه".into());
                }
            }
        };
    let exe = root.join("bin").join("wine");
    let exe64 = root.join("bin").join("wine64");
    if !exe.is_file() && !exe64.is_file() {
        return Err("المجلد المختار فيه ما فيه `bin/wine`".into());
    }
    let path_file = external_path();
    if let Some(parent) = path_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = path_file.with_extension("tmp");
    std::fs::write(&tmp, format!("{}\n", root.display()))
        .map_err(|e| format!("تعذّر حفظ مسار المحرك: {e}"))?;
    let _ = std::fs::remove_file(&path_file);
    std::fs::rename(&tmp, &path_file).map_err(|e| format!("تعذّر حفظ مسار المحرك: {e}"))?;
    Ok(root.display().to_string())
}

/// يمسح اختيار المحرك الخارجي (رجوع للمحرك المدمج)
pub fn clear_external_engine() -> Result<(), String> {
    let _ = std::fs::remove_file(external_path());
    Ok(())
}

/// محركات Whisky المعروفة على الماك — نبحث عنها تلقائياً (مسارات ثابتة)
pub fn detect_whisky_engines() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let candidates = [
        "/Applications/Whisky.app/Contents/Resources/WhiskyWine",
        "/Applications/Whisky.app/Contents/Resources/wine",
        "/Applications/Whisky.app/Contents/Resources/WhiskyWine.app/Contents/Resources/wine",
        "~/Library/Application Support/Whisky/WhiskyWine",
        "~/Library/Containers/com.isaacmarovitz.Whisky/Data/WhiskyWine",
    ];
    for c in candidates {
        let p = std::path::PathBuf::from(shellexpand_home(c));
        if p.join("bin").join("wine").is_file() || p.join("bin").join("wine64").is_file() {
            out.push(p);
        }
    }
    out
}

/// يوسّع `~` إلى المسار الحقيقي
fn shellexpand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

/// نظّف بقايا محاولات فك سابقة (تشتغل عند بدء التطبيق)
pub fn cleanup_staging() {
    let Ok(rd) = std::fs::read_dir(engines_dir()) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with("staging-") {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

// ============================================================
//  حراسة المسارات (Zip-Slip وما شابه)
// ============================================================

/// مسار آمن؟ (نسبي فقط، بدون ".." ولا حرف قرص ولا جذر)
fn safe_rel(path: &Path) -> bool {
    let mut depth = 0i64;
    for c in path.components() {
        match c {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            // ParentDir / RootDir / Prefix (مثل C:) → رفض فوري
            _ => return false,
        }
        if depth > MAX_DEPTH {
            return false;
        }
    }
    depth > 0
}

/// هدف الرابط يبقى داخل مجلد الفك؟ (فحص لفظي قبل أي لمس للقرص)
/// ملاحظة: الروابط النسبية اللي فيها ".." مشروعة تماماً (شائعة في حزم
/// Wine مثل mono/DLL cross-links) — المرفوض فقط اللي *يخرج* فوق الجذر.
fn link_stays_inside(entry_rel: &Path, link: &Path) -> bool {
    // المطلق مرفوض دائماً (قد يشير لأي مكان بالجهاز)
    if link.is_absolute() {
        return false;
    }
    let parent = entry_rel.parent().unwrap_or(Path::new(""));
    let mut depth = 0i64;
    for c in parent.join(link).components() {
        match c {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false; // خرج فوق جذر الفك
                }
            }
            _ => return false, // RootDir / Prefix — احتياط
        }
    }
    true
}

// ============================================================
//  قارئ يعدّ البايتات المقروءة (لقياس تقدّم فك الضغط)
// ============================================================
struct CountReader<R> {
    inner: R,
    counter: Arc<AtomicU64>,
}

impl<R: Read> Read for CountReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

// ============================================================
//  فك + تحقق + تركيب (يعمل في خيط منفصل — ما يجمّد الواجهة)
// ============================================================

/// السلسلة الكاملة: فك tar.xz → تحقق من البنية → نقل للمكان النهائي.
/// يتصل بالواجهة عبر progress(بايتات مقروءة، إجمالي مضغوط، عدد ملفات).
pub fn install_pipeline(
    id: &str,
    archive: &Path,
    staging: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64, u64) + Send + Sync),
) -> Result<Installed, ExtErr> {
    let (files, bytes, skipped) = extract_tar_xz(archive, staging, cancel, progress)?;

    // تحقق: لازم نلقى بنية Wine حقيقية (bin/wine)
    let root = find_root(staging).ok_or_else(|| {
        ExtErr::Failed("بنية غير متوقعة داخل الأرشيف — ما لقينا ملف تشغيل Wine (bin/wine)".into())
    })?;

    // تركيب ذرّي: نقل للمكان النهائي (نفس القرص = لحظي)
    let target = install_dir(id);
    if target.exists() {
        let _ = std::fs::remove_dir_all(&target);
    }
    std::fs::rename(&root, &target)
        .map_err(|e| ExtErr::Failed(format!("تعذّر نقل المحرك للمكان النهائي: {e}")))?;

    // نظّف بقايا مجلد الفك
    let _ = std::fs::remove_dir_all(staging);

    Ok(Installed {
        path: target.display().to_string(),
        files,
        bytes,
        skipped,
    })
}

/// فك أرشيف tar.xz مع كل الحراسات. يرجع (عدد ملفات، حجم كلي، روابط متخطّاة).
fn extract_tar_xz(
    archive: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64, u64) + Send + Sync),
) -> Result<(u64, u64, u64), ExtErr> {
    let file = File::open(archive).map_err(|e| ExtErr::Failed(format!("تعذّر فتح الأرشيف: {e}")))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);

    let counter = Arc::new(AtomicU64::new(0));
    let reader = BufReader::with_capacity(
        512 * 1024,
        CountReader {
            inner: file,
            counter: Arc::clone(&counter),
        },
    );
    let decoder = xz2::read::XzDecoder::new(reader);
    extract_tar(decoder, total, counter, dest, cancel, progress)
}

/// فك أرشيف tar.gz (طبقة DXVK) بنفس حراسات tar.xz تماماً.
pub(crate) fn extract_tar_gz(
    archive: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64, u64) + Send + Sync),
) -> Result<(u64, u64, u64), ExtErr> {
    let file = File::open(archive).map_err(|e| ExtErr::Failed(format!("تعذّر فتح الأرشيف: {e}")))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);

    let counter = Arc::new(AtomicU64::new(0));
    let reader = BufReader::with_capacity(
        512 * 1024,
        CountReader {
            inner: file,
            counter: Arc::clone(&counter),
        },
    );
    let decoder = flate2::read::GzDecoder::new(reader);
    extract_tar(decoder, total, counter, dest, cancel, progress)
}

/// جوهر الفك الموحّد: أي قارئ tar (xz أو gz) يمر من هنا — حراسة واحدة للجميع.
fn extract_tar<R: Read>(
    reader: R,
    total: u64,
    counter: Arc<AtomicU64>,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64, u64) + Send + Sync),
) -> Result<(u64, u64, u64), ExtErr> {
    std::fs::create_dir_all(dest)
        .map_err(|e| ExtErr::Failed(format!("تعذّر إنشاء مجلد الفك: {e}")))?;

    let mut ar = tar::Archive::new(reader);

    let mut files: u64 = 0;
    let mut bytes: u64 = 0;
    let mut skipped: u64 = 0;
    let mut last = Instant::now();

    let entries = ar
        .entries()
        .map_err(|e| ExtErr::Failed(format!("تعذّر قراءة محتوى الأرشيف: {e}")))?;

    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExtErr::Cancelled);
        }

        let mut e = entry.map_err(|er| ExtErr::Failed(format!("ملف تالف داخل الأرشيف: {er}")))?;

        files += 1;
        if files > MAX_FILES {
            return Err(ExtErr::Failed(
                "عدد ملفات الأرشيف أكبر من الحد المسموح — رفضناه".into(),
            ));
        }

        let etype = e.header().entry_type();
        let size = e.header().size().unwrap_or(0);
        bytes = bytes.saturating_add(size);
        if bytes > MAX_TOTAL {
            return Err(ExtErr::Failed(
                "حجم الأرشيف بعد الفك أكبر من الحد المسموح — رفضناه".into(),
            ));
        }

        // الحارس الأول: مسار الملف نفسه
        let path = e
            .path()
            .map_err(|er| ExtErr::Failed(format!("مسار غير صالح داخل الأرشيف: {er}")))?
            .into_owned();

        if !safe_rel(&path) {
            return Err(ExtErr::Failed(
                "رفضنا ملفاً بمسار خبيث داخل الأرشيف (محاولة خروج عن مجلد الفك)".into(),
            ));
        }

        match etype {
            // ملفات ومجلدات عادية
            tar::EntryType::Directory
            | tar::EntryType::Regular
            | tar::EntryType::Continuous
            | tar::EntryType::GNUSparse => {
                let ok = e
                    .unpack_in(dest)
                    .map_err(|er| ExtErr::Failed(format!("تعذّر فك {}: {er}", path.display())))?;
                if !ok {
                    return Err(ExtErr::Failed(
                        "رفضنا ملفاً بمسار خبيث داخل الأرشيف (تحقق ثانٍ)".into(),
                    ));
                }
            }

            // روابط رمزية/صلبة: ننشئها فقط لو هدفها يبقى داخل مجلد الفك.
            // أي رابط مشبوه أو يفشل إنشاؤه (صلاحيات ويندوز مثلاً) = نتخطاه
            // بهدوء ونكمّل — مع عدّاد نعرضه بالسجل (ما نوقف الفك كله أبداً)
            tar::EntryType::Symlink | tar::EntryType::Link => {
                let link = e.link_name().ok().flatten().map(|c| c.into_owned());

                let Some(link) = link else {
                    skipped += 1;
                    continue;
                };

                if !link_stays_inside(&path, &link) {
                    skipped += 1;
                    continue;
                }

                if e.unpack_in(dest).is_err() {
                    skipped += 1;
                }
            }

            // أنواع أخرى (أجهزة/أنابيب…) — نتخطاها
            _ => {}
        }

        // نبلّغ التقدّم كل ربع ثانية تقريباً (خفيف على الواجهة)
        if last.elapsed() >= Duration::from_millis(250) {
            progress(counter.load(Ordering::Relaxed), total, files);
            last = Instant::now();
        }
    }

    progress(counter.load(Ordering::Relaxed), total, files);
    Ok((files, bytes, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// نفس مسار فك DXVK: gzip → tar → حراسة المسارات → ملفات صحيحة
    #[test]
    fn extracts_tar_gz_with_nested_dirs() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("jisr-gz-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let archive = base.join("layer.tar.gz");

        {
            let file = File::create(&archive).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut builder = tar::Builder::new(enc);
            let data = b"d3d11";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "root/x86_64-windows/d3d11.dll", &data[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }

        let dest = base.join("out");
        let cancel = AtomicBool::new(false);
        let (files, bytes, _) = extract_tar_gz(&archive, &dest, &cancel, &|_, _, _| {}).unwrap();
        assert_eq!(files, 1);
        assert_eq!(bytes, 5);
        assert_eq!(
            std::fs::read(dest.join("root").join("x86_64-windows").join("d3d11.dll")).unwrap(),
            b"d3d11"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cancel_stops_extraction() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("jisr-gz-c-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let archive = base.join("layer.tar.gz");
        {
            let file = File::create(&archive).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut builder = tar::Builder::new(enc);
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "root/a.bin", &b"a"[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let dest = base.join("out");
        let cancel = AtomicBool::new(true);
        assert!(matches!(
            extract_tar_gz(&archive, &dest, &cancel, &|_, _, _| {}),
            Err(ExtErr::Cancelled)
        ));
        let _ = std::fs::remove_dir_all(&base);
    }
}
