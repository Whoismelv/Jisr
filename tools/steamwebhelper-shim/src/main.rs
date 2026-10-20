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
//!
//! **درس ميداني (0.1.20):**
//! • `--single-process` للمتصفح فقط (بلا `--type=`)، والأطفال بلا هذا الفلاق.
//! • الفلاقين الاختيارية من `JISR_CEF_FLAGS` **تُضاف** للافتراضي ولا تستبدله —
//!   استبدالها كان يزيل `--single-process` ⇒ شاشة سوداء عند تفعيل رسم GPU.
//! • Job Object: لو ستيم قتل الغلاف، تُقتل العملية الأصلية معه. بدونها تبقى
//!   `steamwebhelper_real.exe` حيّة بنافذتها ⇒ **نافذة ستيم شبح ثانية**.
//!
//! الفلاقين المتاحة للتجربة (cef-flags.txt):
//!   `--in-process-gpu --use-angle=vulkan --ignore-gpu-blocklist` (رسم GPU)
//!   `--no-single-process` (تشخيص: يرجّع CEF متعدد العمليات)

#![windows_subsystem = "windows"]

use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// بصمة يتعرّف بها ظهر Jisr على أن الملف الحالي هو غلافه (لا نلمس الأصل)
static MARKER: &[u8; 16] = b"JISR_CEF_SHIM_V5";

const REAL_NAME: &str = "steamwebhelper_real.exe";
const LOG_NAME: &str = "steamwebhelper_shim.log";
const LOG_MAX_BYTES: u64 = 256 * 1024;

/// هل هذي عملية فرعية داخل CEF؟ (Chromium يضع `--type=` للأبناء فقط)
fn is_child_process(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "--type" || a.starts_with("--type="))
}

/// الفلاقين = الافتراضي حسب الدور + أي إضافة من المستخدم (لا استبدال).
///
/// المتصفح: `--disable-gpu --single-process` — تعطيل GPU يحل الشاشة السوداء،
/// والـsingle-process يخلي الواجهة تُعرض داخل عملية واحدة (الطريقة المثبتة).
/// الأطفال: `--disable-gpu` فقط — بلا `--single-process` حتى لا يبني الطفل نافذة.
/// `--no-single-process` = مخرج تشخيصي يزيل الفلاق من المتصفح.
fn flags_for(args: &[String], overridden: Option<&str>) -> Vec<String> {
    let child = is_child_process(args);
    let raw = overridden.unwrap_or("");
    let mut tokens: Vec<&str> = raw.split_whitespace().collect();
    let drop_single = tokens.iter().any(|t| *t == "--no-single-process");
    let drop_gpu_off = tokens.iter().any(|t| *t == "--no-disable-gpu");
    tokens.retain(|t| *t != "--no-single-process" && *t != "--no-disable-gpu");

    let mut flags: Vec<String> = Vec::new();
    if !drop_gpu_off {
        flags.push("--disable-gpu".to_string());
    }
    if !child && !drop_single {
        flags.push("--single-process".to_string());
    }
    for t in tokens {
        if !flags.iter().any(|f| f == t) {
            flags.push(t.to_string());
        }
    }
    if child {
        flags.retain(|f| f != "--single-process");
    }
    flags
}

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

/// Job Object: يقتل العملية الأصلية تلقائياً عند موت الغلاف (حتى لو قُتل فجأة).
#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimit {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    const LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const EXTENDED_LIMIT_CLASS: i32 = 9;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            info: *mut c_void,
            len: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub struct Job(*mut c_void);

    impl Job {
        pub fn new() -> Option<Job> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info = ExtendedLimit::default();
                info.basic.limit_flags = LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    handle,
                    EXTENDED_LIMIT_CLASS,
                    &mut info as *mut _ as *mut c_void,
                    std::mem::size_of::<ExtendedLimit>() as u32,
                );
                if ok == 0 {
                    CloseHandle(handle);
                    return None;
                }
                Some(Job(handle))
            }
        }

        pub fn assign(&self, child: &std::process::Child) -> bool {
            unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as *mut c_void) != 0 }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
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

    // نمرّر الوسائط كما هي (OsString) — مسارات غير UTF-8 ما تكسر الغلاف
    let raw_args: Vec<OsString> = env::args_os().skip(1).collect();
    let args: Vec<String> = raw_args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    let raw = env::var("JISR_CEF_FLAGS").unwrap_or_default();
    let flags = flags_for(&args, Some(raw.as_str()));

    log_line(
        &dir,
        &format!(
            "launch pid={} role={} args={:?} flags={:?}",
            std::process::id(),
            if is_child_process(&args) {
                "child"
            } else {
                "browser"
            },
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
    // ما نورّث فلاقين جسر للعملية الأصلية وأطفالها (تصير فلاقين مكرّرة)
    cmd.env_remove("JISR_CEF_FLAGS");
    cmd.args(&raw_args);
    for f in &flags {
        if !args.iter().any(|a| a.eq_ignore_ascii_case(f.as_str())) {
            cmd.arg(f);
        }
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log_line(&dir, &format!("error: spawn failed: {e}"));
            std::process::exit(1);
        }
    };

    // نربط الأصل بـJob: موت الغلاف = موت الأصل (ما تبقى نافذة شبح).
    // ⚠️ **مهم:** المقبض لازم يبقى حيًّا **لحد ما ينتظر** الغلاف العملية.
    // لو أسقطناه هنا، `KILL_ON_JOB_CLOSE` يقتل العملية الأصلية فوراً ⇒
    // «ستيم يفتح بدون نافذة» (هذا بالضبط ما صار في 0.1.21).
    // ⚠️.opt-in عبر `JISR_CEF_JOB=1` (افتراضياً مفعّل؟ لا — مطفأ).
    #[cfg(windows)]
    let _job = {
        let mut handle: Option<job::Job> = None;
        if std::env::var("JISR_CEF_JOB").as_deref() == Ok("1") {
            match job::Job::new() {
                Some(j) => {
                    if !j.assign(&child) {
                        log_line(&dir, "warn: job assign failed (continuing)");
                    }
                    handle = Some(j);
                }
                None => log_line(&dir, "warn: job create failed (continuing)"),
            }
        }
        handle
    };

    match child.wait() {
        Ok(status) => {
            let code = status.code().unwrap_or(1);
            log_line(&dir, &format!("exit code={code}"));
            std::process::exit(code);
        }
        Err(e) => {
            log_line(&dir, &format!("error: wait failed: {e}"));
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn browser_gets_single_process() {
        let f = flags_for(&args(&["-lang=en_US", "-cachedir=C:\\cache"]), None);
        assert!(f.iter().any(|x| x == "--single-process"));
        assert!(f.iter().any(|x| x == "--disable-gpu"));
    }

    #[test]
    fn children_never_get_single_process() {
        let f = flags_for(&args(&["--type=renderer", "--no-sandbox"]), None);
        assert!(!f.iter().any(|x| x == "--single-process"));
        assert!(f.iter().any(|x| x == "--disable-gpu"));

        let g = flags_for(
            &args(&["--type=gpu-process"]),
            Some("--in-process-gpu --single-process"),
        );
        assert!(!g.iter().any(|x| x == "--single-process"));
    }

    #[test]
    fn type_switch_without_equals_is_detected() {
        assert!(is_child_process(&args(&["--type", "utility"])));
        assert!(is_child_process(&args(&["--type=utility"])));
        assert!(!is_child_process(&args(&["-silent"])));
    }

    /// الدرس: التجاوز يُضاف ولا يستبدل — تفعيل GPU ما يلغي single-process
    #[test]
    fn override_is_additive() {
        let f = flags_for(
            &args(&["-lang=en_US"]),
            Some("--in-process-gpu --use-angle=vulkan"),
        );
        assert!(f.iter().any(|x| x == "--single-process"));
        assert!(f.iter().any(|x| x == "--disable-gpu"));
        assert!(f.iter().any(|x| x == "--in-process-gpu"));
        assert!(f.iter().any(|x| x == "--use-angle=vulkan"));
        assert_eq!(f.iter().filter(|x| *x == "--disable-gpu").count(), 1);
    }

    #[test]
    fn no_single_process_escape_hatch() {
        let f = flags_for(&args(&["-lang=en_US"]), Some("--no-single-process"));
        assert!(!f.iter().any(|x| x == "--single-process"));
        assert!(!f.iter().any(|x| x == "--no-single-process"));
    }

    /// وضع «أحادي»: `--single-process` وحده — بدون `--disable-gpu`
    /// (تركيبة `--disable-gpu --single-process` هي الوحيدة التي قيستها مصادر
    ///  مستقلة وتسبب «نافذة ما تظهر» على محركات بلا سياق GLES 3.0)
    #[test]
    fn no_disable_gpu_escape_hatch() {
        let f = flags_for(&args(&["-lang=en_US"]), Some("--single-process --no-disable-gpu"));
        assert!(f.iter().any(|x| x == "--single-process"));
        assert!(!f.iter().any(|x| x == "--disable-gpu"));
        assert!(!f.iter().any(|x| x == "--no-disable-gpu"));
    }
}
