// ============================================================
//  جسر — محاكاة بيئة macOS كاملة (اختبار فقط)
//  نبني شجرة مجلدات مطابقة ل/what يلمسه المستخدم بالليل على الماك:
//    ~/Library/Application Support/Jisr            (بيانات جسر)
//    ~/Library/Application Support/Jisr/bottles/main (القارورة)
//    .../drive_c/Program Files (x86)/Steam          (تثبيت ستيم)
//    .../drive_c/users/<user>/AppData/Local/Steam   (كاش CEF الحقيقي)
//  ونشغّل عليها نفس الكود (غلاف CEF · تنظيف الكاش · التقرير · السجل)
//  عشان نكتشف الأخطاء هنا بدل ما ننتظر تجر��ة الماك.
// ============================================================
#![cfg(test)]

use std::fs;
use std::path::PathBuf;

/// قفل عالمي: الاختبارات تلمس حالة مشتركة (مجلد البيانات + السجل)
static SIM_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// بيئة macOS محاكاة
struct FakeMac {
    /// جذر "الماك الوهمي"
    home: PathBuf,
    /// مجلد بيانات جسر (Application Support/Jisr)
    data: PathBuf,
    /// مجلد ستيم داخل القارورة
    steam_dir: PathBuf,
    /// بروفايل مستخدم ويندوز داخل القارورة
    user_local: PathBuf,
}

impl FakeMac {
    /// يبني الشجرة كاملة
    fn build(tag: &str) -> FakeMac {
        let home = std::env::temp_dir().join(format!(
            "jisr-macsim-{tag}-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        let _ = fs::remove_dir_all(&home);

        // ~/Library/Application Support/Jisr
        let data = home
            .join("Library")
            .join("Application Support")
            .join("Jisr");
        fs::create_dir_all(data.join("logs")).unwrap();

        // قارورة ستيم
        let drive_c = data.join("bottles").join("main").join("drive_c");
        let steam_dir = drive_c.join("Program Files (x86)").join("Steam");
        fs::create_dir_all(&steam_dir).unwrap();
        fs::write(steam_dir.join("steam.exe"), b"MZfake steam.exe").unwrap();

        // سجلات ستيم الرسمية (المهمة اللي تبيّن الفلاقين الحقيقية)
        fs::create_dir_all(steam_dir.join("logs")).unwrap();
        fs::write(
            steam_dir.join("logs").join("webhelper.txt"),
            b"Command line: steamwebhelper.exe --type=renderer --single-process --disable-gpu\n",
        )
        .unwrap();
        fs::write(
            steam_dir.join("logs").join("bootstrap_log.txt"),
            b"Bootstrap: verifying files OK\n",
        )
        .unwrap();
        fs::write(
            steam_dir.join("logs").join("cef_log.txt"),
            b"CEF log: no errors\n",
        )
        .unwrap();

        // غلاف CEF: ملف Valve الأصلي (حجمه أكبر من ميغابايت)
        let cef = steam_dir.join("bin").join("cef").join("cef.win64");
        fs::create_dir_all(&cef).unwrap();
        let mut valve = vec![0x41u8; 1_100_000];
        valve[0] = b'M';
        valve[1] = b'Z';
        fs::write(cef.join("steamwebhelper.exe"), valve).unwrap();

        // بروفايل مستخدم ويندوز: كاش CEF الحقيقي + قفل قديم
        let user_local = drive_c
            .join("users")
            .join("tester")
            .join("AppData")
            .join("Local")
            .join("Steam");
        fs::create_dir_all(user_local.join("htmlcache")).unwrap();
        fs::write(user_local.join("htmlcache").join("Cache_Data"), b"stale").unwrap();
        fs::write(user_local.join("SingletonLock"), b"stale-lock").unwrap();

        // ملف ما له علاقة — لازم ما ينمس
        fs::create_dir_all(steam_dir.join("config")).unwrap();
        fs::write(
            steam_dir.join("config").join("config.vdf"),
            b"\"InstallDir\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"",
        )
        .unwrap();

        FakeMac {
            home,
            data,
            steam_dir,
            user_local,
        }
    }

    fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

/// يوقف أي تجاوز لمجلد البيانات (ينادى دائماً بعد الاختبار)
fn reset_data_dir() {
    crate::set_data_dir_override(None);
}

/// يبني بيئة محاكاة كاملة ويشغّل كل المسار الحرج عليها
#[test]
fn simulated_mac_environment_runs_the_whole_critical_path() {
    let _guard = SIM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mac = FakeMac::build("full");
    crate::set_data_dir_override(Some(mac.data.clone()));
    // السجل الدائم يشير لمجلد السجل داخل البيئة المحاكاة
    let log_path = crate::debuglog::init_path(&mac.data.join("logs"));

    // 1) المسارات تتبع البيئة المحاكاة (زي ما بالماك بالضبط)
    assert_eq!(crate::data_dir(), mac.data);
    assert!(
        crate::data_dir()
            .to_string_lossy()
            .contains("Application Support")
    );
    assert_eq!(
        crate::steam::steam_exe().map(|(p, _)| p),
        Some(mac.steam_dir.join("steam.exe"))
    );

    // 2) غلاف CEF: يُركَّب ويحفظ الأصل + نسخة احتياطية
    let backup_dir = mac.data.join("steam-cef-backup");
    let installed = crate::steam::ensure_cef_shim(&mac.steam_dir, &backup_dir);
    assert_eq!(installed, Some(true));
    let helper = mac
        .steam_dir
        .join("bin")
        .join("cef")
        .join("cef.win64")
        .join("steamwebhelper.exe");
    assert!(crate::steam::is_cef_shim(&helper), "الغلاف لازم يكون مركّب");
    assert!(
        mac.steam_dir
            .join("bin")
            .join("cef")
            .join("cef.win64")
            .join("steamwebhelper_real.exe")
            .is_file(),
        "لازم الأصل يكون محفوظ بجانبه"
    );
    assert!(backup_dir.join("steamwebhelper.exe").is_file());
    // التثبيت idempotent: تشغيل ثانٍ ما يرجّع anything
    assert_eq!(
        crate::steam::ensure_cef_shim(&mac.steam_dir, &backup_dir),
        Some(false)
    );

    // 3) تنظيف البقايا: المسار الحقيقي لكاش CEF + القفل القديم
    crate::steam::cleanup_stale_steam_state(&mac.steam_dir);
    assert!(!mac.user_local.join("htmlcache").exists(), "الكاش انمسح");
    assert!(
        !mac.user_local.join("SingletonLock").exists(),
        "القفل انمسح"
    );
    assert!(
        mac.steam_dir.join("config").join("config.vdf").is_file(),
        "ملف ما له علاقة ما ينمس"
    );
    // والنسخ الاحتياطية موجودة (تراجع ممكن)
    let backup_dir_cache = fs::read_dir(&mac.user_local)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("jisr-bak"))
        .count();
    assert!(backup_dir_cache > 0, "لازم نسخة احتياطية قابلة للتراجع");

    // 4) مفتاح الطوارئ: سطر steam: يُقرأ، وغير المسموح يُتجاهل
    fs::write(
        mac.data.join("cef-flags.txt"),
        "--in-process-gpu --use-angle=vulkan\nsteam:-cef-single-process -rm -rf /\n",
    )
    .unwrap();
    assert_eq!(
        crate::steam::steam_args_override(),
        vec!["-cef-single-process".to_string()],
        "القائمة البيضاء لازم ترفض أي وسائط أخرى"
    );
    assert_eq!(
        crate::steam::cef_flags_override().as_deref(),
        Some("--in-process-gpu --use-angle=vulkan")
    );

    // 5) السجل الدائم يكتب فعلاً في المسار المتوقع
    crate::debuglog::raw("info", "sim", "محاكاة macOS — سطر اختبار");
    let content = fs::read_to_string(&log_path).unwrap();
    assert!(
        content.contains("محاكاة macOS — سطر اختبار"),
        "السجل لازم يكتب"
    );
    assert!(content.contains("[sim]"));

    // 6) التقرير الكامل: لازم يحتوي كل ما يحتاجه المطور
    let info = crate::SystemInfo {
        os_name: "macOS".into(),
        os_version: "26.6.1".into(),
        arch: "aarch64".into(),
        is_apple_silicon: true,
        chip: "Apple M1 Max".into(),
        cpu_cores: Some(10),
        memory_mb: Some(32_768),
        rosetta_installed: true,
        engine_installed: true,
        engine_path: Some("/Applications/Jisr/wine".into()),
        graphics_layer: Some("dxvk".into()),
        ..Default::default()
    };
    let report = crate::build_report_from(&info, Some(mac.steam_dir.as_path()));

    assert!(report.contains("Jisr — full debug report"), "رأس التقرير");
    assert!(report.contains("macOS 26.6.1"), "معلومات النظام");
    assert!(report.contains("aarch64"), "المعمارية");
    assert!(
        report.contains("cef_shim_flags_seen: single-process=true disable-gpu=true")
            || report.contains("cef_shim_flags_seen: single-process=true"),
        "حكم الفلاقين لازم يطلع من webhelper.txt"
    );
    assert!(report.contains("Steam/logs/webhelper.txt"), "سجل webhelper");
    assert!(
        report.contains("Steam/logs/bootstrap_log.txt"),
        "سجل bootstrap"
    );
    assert!(report.contains("Steam/logs/cef_log.txt"), "سجل cef");
    assert!(
        report.contains("Bootstrap: verifying files OK"),
        "محتوى السجلات"
    );
    assert!(report.contains("jisr-debug.log"), "السجل الدائم");
    assert!(
        report.contains("محاكاة macOS — سطر اختبار"),
        "سطور السجل داخل التقرير"
    );
    assert!(report.contains("cef_flags_override"), "فلاقين المستخدم");
    assert!(report.contains("bottles_dir:"), "مسار القوارير");

    reset_data_dir();
    mac.cleanup();
}

/// محاكاة جهاز المستخدم بالضبط: MacBook Pro 16" M1 Max — macOS 26.6.1
/// (المسار الماك ما يشتغل على ويندوز أبداً — الحقن يخلّينا نختبره)
#[test]
fn macos_detection_reads_the_real_values() {
    let run = |cmd: &str, args: &[&str]| -> Option<String> {
        match (cmd, args) {
            ("sw_vers", ["-productVersion"]) => Some("26.6.1".to_string()),
            ("sysctl", ["-n", "machdep.cpu.brand_string"]) => Some("Apple M1 Max".to_string()),
            ("sysctl", ["-n", "hw.ncpu"]) => Some("10".to_string()),
            ("sysctl", ["-n", "hw.memsize"]) => Some("34359738368".to_string()),
            _ => None,
        }
    };

    let gpus = vec![crate::GpuInfo {
        name: "Apple M1 Max".to_string(),
        vendor: "Apple".to_string(),
        cores: Some(24),
        vram_mb: None,
    }];
    let info = crate::detect_macos_with(&run, gpus, true, "aarch64");
    assert_eq!(info.os_name, "macOS");
    assert_eq!(info.os_version, "26.6.1");
    assert!(info.is_apple_silicon);
    assert_eq!(info.chip, "Apple M1 Max");
    assert_eq!(info.cpu_cores, Some(10));
    assert_eq!(info.memory_mb, Some(32768), "32GB لازم تطلع 32768");
    assert!(info.rosetta_installed);
    assert_eq!(info.gpus.len(), 1);
    assert_eq!(info.gpus[0].name, "Apple M1 Max");

    // إنتل بدون Rosetta
    let intel = crate::detect_macos_with(&run, vec![], false, "x86_64");
    assert!(!intel.is_apple_silicon);
    assert!(!intel.rosetta_installed);

    // أوامر ناقصة — لازم ما ينهار
    let empty = |_: &str, _: &[&str]| -> Option<String> { None };
    let bare = crate::detect_macos_with(&empty, vec![], false, "aarch64");
    assert_eq!(bare.os_version, "");
    assert_eq!(bare.memory_mb, None);
    assert_eq!(bare.cpu_cores, None);
}

/// التقرير لازم ينتج نص منطقي حتى لو ما في ستيم ولا محرك (حالة أول تشغيل)
#[test]
fn report_works_on_a_bare_machine() {
    let _guard = SIM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mac = FakeMac::build("bare");
    crate::set_data_dir_override(Some(mac.data.clone()));
    crate::debuglog::init_path(&mac.data.join("logs"));

    let info = crate::SystemInfo {
        os_name: "macOS".into(),
        os_version: "26.6.1".into(),
        arch: "aarch64".into(),
        ..Default::default()
    };
    // بدون ستيم ولا محرك
    let report = crate::build_report_from(&info, None);
    assert!(report.contains("wine: not installed"));
    assert!(report.contains("cef_shim_flags_seen: (webhelper.txt غير موجود"));
    assert!(report.contains("Jisr — full debug report"));

    reset_data_dir();
    mac.cleanup();
}

/// تفريغ السجل من الواجهة يبدأ الملف من جديد
#[test]
fn clearing_the_log_truncates_the_file() {
    let _guard = SIM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mac = FakeMac::build("clear");
    crate::set_data_dir_override(Some(mac.data.clone()));
    let log_path = crate::debuglog::init_path(&mac.data.join("logs"));

    crate::debuglog::raw("info", "sim", "سطر قبل التفريغ");
    assert!(
        fs::read_to_string(&log_path)
            .unwrap()
            .contains("قبل التفريغ")
    );

    crate::debuglog::clear();
    let after = fs::read_to_string(&log_path).unwrap();
    assert!(!after.contains("قبل التفريغ"), "السطر القديم لازم ينمسح");
    assert!(after.contains("تم تفريغ السجل"), "لازم يبقى سطر التوضيح");

    reset_data_dir();
    mac.cleanup();
}
