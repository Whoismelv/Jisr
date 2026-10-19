//! Jisr — غلاف CEF لـ `steamwebhelper.exe` (ستيم داخل Wine على Apple Silicon)
//!
//! السبب: واجهة ستيم مبنية على Chromium/CEF. النسخة الحالية تطلب سياق GLES 3.0
//! لعملية الـGPU المنفصلة (out-of-process GPU)، وWine على الماك يوصل هذا الطريق
//! لسقف GLES 2.0 → النافذة تبقى سوداء رغم أن ستيم شغّال.
//!
//! الحل المعتمد من مجتمع Wine/ماك (Gcenx، notpop، wisnuub، ramiabih…):
//! نستبدل `steamwebhelper.exe` بغلاف صغير، ونحتفظ بالأصل بجانبه باسم
//! `steamwebhelper_real.exe`. الغلاف يشغّل الأصل مع نفس الوسائط + فلاقين
//! افتراضيين للرسم البرمجي داخل عملية واحدة (`--disable-gpu --single-process`).
//! ولمن يبي مسار GPU: `--in-process-gpu --use-angle=vulkan --ignore-gpu-blocklist`
//! عبر ملف `cef-flags.txt` (ANGLE → Vulkan → MoltenVK → Metal).
//!
//! يمكن تجاوز الفلاقين بدون بناء جديد عبر متغيّر البيئة `JISR_CEF_FLAGS`
//! (تقرأه الواجهة من ملف اختياري — مفيد للتشخيص الميداني).

#![windows_subsystem = "windows"]

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// بصمة يتعرّف بها ظهر Jisr على أن الملف الحالي هو غلافه (لا نلمس الأصل)
static MARKER: &[u8; 16] = b"JISR_CEF_SHIM_V1";

const REAL_NAME: &str = "steamwebhelper_real.exe";
const LOG_NAME: &str = "steamwebhelper_shim.log";
const LOG_MAX_BYTES: u64 = 256 * 1024;
/// الافتراضي: رسم واجهة ستيم على المعالج داخل عملية واحدة.
/// أكثر الطرق ثباتاً على Apple Silicon (تستخدمها notpop وwisnuub افتراضياً):
/// تُلغي تسليم الفريمات بين العمليات (سبب الشاشة السوداء) وتُنهي وميض النوافذ.
/// الوضع المسرّع بالـGPU متاح عبر `cef-flags.txt`:
/// `--in-process-gpu --use-angle=vulkan --ignore-gpu-blocklist`
const DEFAULT_FLAGS: &str = "--disable-gpu --single-process";

fn log_line(dir: &Path, line: &str) {
    let path = dir.join(LOG_NAME);
    if let Ok(md) = std::fs::metadata(&path) {
        if md.len() > LOG_MAX_BYTES {
            let _ = std::fs::remove_file(&path);
        }
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

fn main() {
    // نثبّت البصمة داخل الملف التنفيذي (يستخدمها ظهر Jisr للتحقق)
    std::hint::black_box(MARKER);

    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(_) => std::process::exit(1),
    };
    let dir: PathBuf = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let real = dir.join(REAL_NAME);

    // حماية من التكرار اللانهائي: لو صار الملف المحفوظ نفسه غلافاً (تلف نادر)
    // أو أعد Wine تشغيل الغلاف كطفل، نرفض بدل ما نولّد سلسلة عمليات لا تنتهي.
    let launched_as_real = exe
        .file_name()
        .map(|n| n.eq_ignore_ascii_case(REAL_NAME))
        .unwrap_or(false);
    if launched_as_real || env::var_os("JISR_CEF_DEPTH").is_some() {
        log_line(&dir, "error: shim recursion blocked");
        std::process::exit(2);
    }

    let args: Vec<String> = env::args().skip(1).collect();

    // الفلاقين: افتراضي مدروس، أو تجاوز اختياري من ظهر Jisr
    let raw = env::var("JISR_CEF_FLAGS").unwrap_or_default();
    let flags_raw = if raw.trim().is_empty() {
        DEFAULT_FLAGS
    } else {
        raw.trim()
    };
    let flags: Vec<String> = flags_raw
        .split_whitespace()
        .map(str::to_string)
        .collect();

    log_line(
        &dir,
        &format!(
            "launch pid={} args={:?} flags={:?}",
            std::process::id(),
            args,
            flags
        ),
    );

    if !real.is_file() {
        log_line(&dir, &format!("error: missing {REAL_NAME}"));
        std::process::exit(1);
    }

    let mut cmd = Command::new(&real);
    cmd.env("JISR_CEF_DEPTH", "1");
    cmd.args(&args);
    for f in &flags {
        if !args.iter().any(|a| a.eq_ignore_ascii_case(f.as_str())) {
            cmd.arg(f);
        }
    }

    match cmd.status() {
        Ok(status) => {
            let code = status.code().unwrap_or(0);
            log_line(&dir, &format!("exit code={code}"));
            std::process::exit(code);
        }
        Err(e) => {
            log_line(&dir, &format!("error: spawn failed: {e}"));
            std::process::exit(1);
        }
    }
}
