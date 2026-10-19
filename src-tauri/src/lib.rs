// ============================================================
//  جسر — الخلفية (Rust)
//  • فحص الجهاز الحقيقي (نفس الكود على macOS و Windows)
//  • سجل أحداث يُرسل للواجهة **بمفاتيح/معاملات** (لا نصوص) — الترجمة في الواجهة
// ============================================================
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

mod bottle;
mod debuglog;
mod download;
mod engine;
mod game;
mod graphics;
mod steam;
mod steam_library;

// ---------- الأنواع اللي تروح للواجهة ----------
#[derive(Serialize, Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub cores: Option<u32>,
    pub vram_mb: Option<u64>,
}

#[derive(Serialize, Clone, Default)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub arch: String,
    pub is_apple_silicon: bool,
    pub chip: String,
    pub cpu_cores: Option<usize>,
    pub memory_mb: Option<u64>,
    pub gpus: Vec<GpuInfo>,
    pub rosetta_installed: bool,
    pub engine_installed: bool,
    pub engine_path: Option<String>,
    pub graphics_layer: Option<String>,
}

/// حدث سجل: مفتاح نصي + معاملات → الواجهة تترجم بأي لغة
#[derive(Serialize, Clone, Default)]
pub struct LogEntry {
    pub time_ms: u64,
    pub level: String, // info | warn | error
    pub key: String,   // مثال: "log.startup"
    pub params: Value, // مثال: { "dir": "..." }
}

#[derive(Default)]
pub struct LogState(pub Mutex<Vec<LogEntry>>);

// مفاتيح السجل (تطابق مفاتيح القاموس في i18n)
pub mod log_keys {
    pub const STARTUP: &str = "log.startup";
    pub const SCAN_DONE: &str = "log.scanDone";
    pub const ENGINE_FOUND: &str = "log.engineFound";
    pub const ENGINE_NOT_FOUND: &str = "log.engineNotFound";
    pub const ROSETTA_OK: &str = "log.rosettaReady";
    pub const ROSETTA_MISSING: &str = "log.rosettaMissing";
    pub const CLEARED: &str = "log.cleared";
    // التنزيل
    pub const DOWNLOAD_STARTED: &str = "log.downloadStarted";
    pub const DOWNLOAD_DONE: &str = "log.downloadDone";
    pub const DOWNLOAD_FAILED: &str = "log.downloadFailed";
    pub const DOWNLOAD_CANCELLED: &str = "log.downloadCancelled";
    // الفك والتركيب
    pub const GRAPHICS_READY: &str = "log.graphicsReady";
    pub const GRAPHICS_FAILED: &str = "log.graphicsFailed";
    pub const STEAM_GAME_LAUNCHING: &str = "log.steamGameLaunching";
    pub const STEAM_GAME_LAUNCH_FAILED: &str = "log.steamGameLaunchFailed";
    pub const EXTRACT_STARTED: &str = "log.extractStarted";
    pub const EXTRACT_DONE: &str = "log.extractDone";
    pub const EXTRACT_FAILED: &str = "log.extractFailed";
    pub const EXTRACT_SKIPPED: &str = "log.extractSkipped";
    pub const ENGINE_READY: &str = "log.engineReady";
    // القوارير
    pub const BOTTLE_STARTED: &str = "log.bottleStarted";
    pub const BOTTLE_READY: &str = "log.bottleReady";
    pub const BOTTLE_FAILED: &str = "log.bottleFailed";
    pub const BOTTLE_DELETED: &str = "log.bottleDeleted";
    // الألعاب
    pub const GAME_ADDED: &str = "log.gameAdded";
    pub const GAME_REMOVED: &str = "log.gameRemoved";
    pub const GAME_LAUNCHING: &str = "log.gameLaunching";
    pub const GAME_EXITED: &str = "log.gameExited";
    pub const GAME_STOPPED: &str = "log.gameStopped";
    pub const GAME_LAUNCH_FAILED: &str = "log.gameLaunchFailed";
    // ستيم
    pub const STEAM_INSTALLING: &str = "log.steamInstalling";
    pub const STEAM_INSTALLED: &str = "log.steamInstalled";
    pub const STEAM_INSTALL_EXITED: &str = "log.steamInstallExited";
    pub const STEAM_INSTALL_FAILED: &str = "log.steamInstallFailed";
    pub const STEAM_INSTALL_STOPPED: &str = "log.steamInstallStopped";
    pub const STEAM_LAUNCHING: &str = "log.steamLaunching";
    pub const STEAM_EXITED: &str = "log.steamExited";
    pub const STEAM_STOPPED: &str = "log.steamStopped";
    pub const STEAM_LAUNCH_FAILED: &str = "log.steamLaunchFailed";
    pub const STEAM_UI_FIX: &str = "log.steamUiFix";
    pub const STEAM_UI_FIX_FAILED: &str = "log.steamUiFixFailed";
}

const MAX_LOGS: usize = 800;

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// آخر أسطر من مخرجات الخطأ (للرسائل عند الفشل السريع)
pub(crate) fn tail_lines(bytes: &[u8], n: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > n {
        lines = lines.split_off(lines.len() - n);
    }
    lines.join(" | ").chars().take(300).collect()
}

/// نحفظ الحدث في الذاكرة (يُعرض عند فتح صفحة السجل) — بحد أقصى ثابت
pub(crate) fn remember(app: &AppHandle, entry: &LogEntry) {
    if let Some(state) = app.try_state::<LogState>() {
        if let Ok(mut v) = state.0.lock() {
            v.push(entry.clone());
            if v.len() > MAX_LOGS {
                let excess = v.len() - MAX_LOGS;
                v.drain(0..excess);
            }
        }
    }
}

/// نضيف سطر للسجل + نرسله للواجهة فوراً
pub(crate) fn log(app: &AppHandle, level: &str, key: &str, params: Value) {
    // السجل الدائم: المفتاح + كل المعاملات (حتى لو الواجهة ما تعرف ترجمته)
    let detail = match params.as_object() {
        Some(o) if o.is_empty() => String::new(),
        _ => format!(" {params}"),
    };
    debuglog::write(level, "event", &format!("{key}{detail}"));

    let entry = LogEntry {
        time_ms: now_ms(),
        level: level.to_string(),
        key: key.to_string(),
        params,
    };

    remember(app, &entry);
    let _ = app.emit("jisr-log", entry);
}

// ============================================================
//  تنفيذ أوامر النظام
// ============================================================
fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn run_env(cmd: &str, args: &[&str], envs: &[(&str, &str)]) -> Option<String> {
    let mut c = Command::new(cmd);
    c.args(args);
    for (k, v) in envs {
        c.env(k, v);
    }
    c.output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// مجلد بيانات جسر
pub(crate) fn data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join("Library")
            .join("Application Support")
            .join("Jisr")
    } else {
        PathBuf::from(
            std::env::var("LOCALAPPDATA")
                .or_else(|_| std::env::var("APPDATA"))
                .unwrap_or_default(),
        )
        .join("Jisr")
    }
}

pub(crate) fn engines_dir() -> PathBuf {
    data_dir().join("engines")
}

/// أول محرك موجود داخل engines؟
fn engine_info() -> (bool, Option<String>, Option<String>) {
    let dir = engines_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return (false, None, None);
    };

    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            // مجلد التنزيلات ومجلدات الفك المؤقتة مو محرك — نتخطاها
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "downloads" || name.starts_with("staging-") {
                continue;
            }
            let gfx = detect_graphics_layer(&path);
            return (true, path.to_str().map(String::from), gfx);
        }
    }
    (false, None, None)
}

/// هل المجلد/النظام يملك طبقة رسومات؟
fn detect_graphics_layer(engine_path: &std::path::Path) -> Option<String> {
    // نظرة سريعة: هل حزمة المحرك داخلها dxmt (بدون عدّ كل ملفات القرص)؟
    let name = engine_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    // نحاول أن نقرأ ملف تعريف بسيط لو المحرك أُعدّ مسبقاً، وإلا اعتمد الاسم
    if name.contains("dxmt") {
        return Some("DXMT".into());
    } else if name.contains("dxvk") {
        return Some("DXVK".into());
    } else if name.contains("moltenvk") {
        return Some("MoltenVK".into());
    }

    // مجرّد بحث في مجلد ضمني (سريع)
    if let Ok(rd) = std::fs::read_dir(engine_path) {
        for e in rd.flatten() {
            let n = e
                .path()
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if n.contains("d3dmt") || n.contains("dxmt") {
                return Some("DXMT".into());
            }
            if n.contains("dxvk") {
                return Some("DXVK".into());
            }
        }
    }

    None
}

// ============================================================
//  فحص الجهاز
// ============================================================
fn detect_system() -> SystemInfo {
    let mut info = SystemInfo {
        arch: std::env::consts::ARCH.to_string(),
        ..Default::default()
    };

    if cfg!(target_os = "macos") {
        info.os_name = "macOS".into();
        info.os_version = run_cmd("sw_vers", &["-productVersion"]).unwrap_or_default();
        info.is_apple_silicon = info.arch == "aarch64";
        info.chip = run_cmd("sysctl", &["-n", "machdep.cpu.brand_string"]).unwrap_or_default();
        info.cpu_cores = run_cmd("sysctl", &["-n", "hw.ncpu"]).and_then(|s| s.parse().ok());
        info.memory_mb = run_cmd("sysctl", &["-n", "hw.memsize"])
            .and_then(|s| s.parse::<u64>().ok())
            .map(|b| b / 1024 / 1024);
        info.gpus = macos_gpus();
        info.rosetta_installed = cfg!(target_arch = "aarch64")
            && std::path::Path::new("/Library/Apple/usr/libexec/oah").exists();
    } else {
        windows_info(&mut info);
        if info.os_name.is_empty() {
            info.os_name = std::env::consts::OS.to_string();
        }
    }

    let (engine, path, detected) = engine_info();
    info.engine_installed = engine;
    info.engine_path = path;
    // الطبقة الفعلية: DXVK إن كان منزّلاً ومفعّلاً، وإلا WineD3D
    info.graphics_layer = if engine {
        Some(graphics::active_layer().to_string())
    } else {
        detected
    };
    // نحفظ نتيجة الفحص كاملة في السجل الدائم (تشخيص من الجهاز نفسه)
    debuglog::raw(
        "debug",
        "system",
        &format!(
            "scan: {}",
            serde_json::to_string(&info).unwrap_or_else(|_| "{}".into())
        ),
    );
    info
}

/// كروت الشاشة على الماك — JSON (ما يتأثر بلغة النظام)
fn macos_gpus() -> Vec<GpuInfo> {
    let Some(out) = run_env(
        "system_profiler",
        &["-json", "SPDisplaysDataType"],
        &[("LC_ALL", "en_US.UTF-8")],
    ) else {
        return vec![];
    };

    let Ok(v) = serde_json::from_str::<Value>(&out) else {
        return vec![];
    };

    let Some(arr) = v.get("SPDisplaysDataType").and_then(|x| x.as_array()) else {
        return vec![];
    };

    arr.iter()
        .filter_map(|g| {
            let name = as_text(g.get("_name"));
            if name.is_empty() {
                return None;
            }
            Some(GpuInfo {
                name,
                vendor: as_text(g.get("sppci_vendor")),
                cores: as_text(g.get("sppci_cores")).parse::<u32>().ok(),
                vram_mb: None,
            })
        })
        .collect()
}

fn as_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// ويندوز (بيئة التطوير): نداء واحد يجيب كل شي
fn windows_info(info: &mut SystemInfo) {
    let script = r#"
$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$cs = Get-CimInstance Win32_ComputerSystem
'os=' + $os.Caption + '|' + $os.Version
'cpu=' + $cpu.Name + '|' + $cpu.NumberOfCores
'mem=' + $cs.TotalPhysicalMemory
Get-CimInstance Win32_VideoController | ForEach-Object { 'gpu=' + $_.Name + '|' + $_.AdapterRAM }
"#;

    let Some(out) = run_cmd(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    ) else {
        return;
    };

    for line in out.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("os=") {
            let mut parts = v.splitn(2, '|');
            info.os_name = parts.next().unwrap_or("").trim().to_string();
            info.os_version = parts.next().unwrap_or("").trim().to_string();
        } else if let Some(v) = line.strip_prefix("cpu=") {
            let mut parts = v.splitn(2, '|');
            info.chip = parts.next().unwrap_or("").trim().to_string();
            info.cpu_cores = parts.next().and_then(|x| x.trim().parse().ok());
        } else if let Some(v) = line.strip_prefix("mem=") {
            info.memory_mb = v.trim().parse::<u64>().ok().map(|b| b / 1024 / 1024);
        } else if let Some(v) = line.strip_prefix("gpu=") {
            let mut parts = v.splitn(2, '|');
            let name = parts.next().unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            let vram_mb = parts
                .next()
                .and_then(|x| x.trim().parse::<u64>().ok())
                .filter(|n| *n > 0)
                .map(|b| b / 1024 / 1024);
            info.gpus.push(GpuInfo {
                name,
                vendor: String::new(),
                cores: None,
                vram_mb,
            });
        }
    }
}

// ============================================================
//  أوامر Tauri
// ============================================================

/// فحص الجهاز + أحداث سجل بمفاتيح
#[tauri::command]
fn system_info(app: AppHandle) -> SystemInfo {
    let info = detect_system();

    log(&app, "info", log_keys::SCAN_DONE, json!({}));

    if info.is_apple_silicon {
        if info.rosetta_installed {
            log(&app, "info", log_keys::ROSETTA_OK, json!({}));
        } else {
            log(&app, "warn", log_keys::ROSETTA_MISSING, json!({}));
        }
    }

    if info.engine_installed {
        log(
            &app,
            "info",
            log_keys::ENGINE_FOUND,
            json!({ "path": info.engine_path.clone().unwrap_or_default() }),
        );
    } else {
        log(&app, "warn", log_keys::ENGINE_NOT_FOUND, json!({}));
    }

    info
}

#[tauri::command]
fn get_logs(app: AppHandle) -> Vec<LogEntry> {
    app.try_state::<LogState>()
        .and_then(|s| s.0.lock().ok().map(|v| v.clone()))
        .unwrap_or_default()
}

#[tauri::command]
fn clear_logs(app: AppHandle) {
    if let Some(state) = app.try_state::<LogState>() {
        if let Ok(mut v) = state.0.lock() {
            v.clear();
        }
    }
    // نبدأ ملف التشخيص من جديد أيضاً — «تفريغ» يعني تفريغ فعلاً
    debuglog::clear();
    log(&app, "info", log_keys::CLEARED, json!({}));
}

/// الواجهة تكتب سجل (بمفتاح محلي)
/// مدخلات الواجهة غير موثوقة دفاعياً: نحدّ المستوى والمفتاح والحجم
#[tauri::command]
fn log_line(app: AppHandle, level: String, key: String, params: Option<Value>) {
    let level = match level.as_str() {
        "warn" | "error" => level,
        _ => "info".to_string(),
    };
    let key: String = key.chars().take(80).collect();
    let params = params
        .filter(|v| v.to_string().len() <= 4000)
        .unwrap_or_else(|| json!({}));
    log(&app, &level, &key, params);
}

/// سطر تشخيص خام من الواجهة (أخطاء JS، فشل الأوامر…) — يظهر حرفياً في السجل
#[tauri::command]
fn debug_log(level: String, message: String) {
    let level = match level.as_str() {
        "warn" | "error" | "info" | "debug" => level,
        _ => "debug".to_string(),
    };
    let message: String = message.chars().take(4000).collect();
    debuglog::raw(&level, "ui", &message);
}

/// مسار ملف السجل الحالي (زر «إظهار في Finder»)
#[tauri::command]
fn debug_log_path() -> Option<String> {
    debuglog::path().map(|p| p.to_string_lossy().to_string())
}

fn push_section(out: &mut String, title: &str, body: &str) {
    out.push_str("\n\n========== ");
    out.push_str(title);
    out.push_str(" ==========\n");
    if body.trim().is_empty() {
        out.push_str("(empty)\n");
    } else {
        out.push_str(body.trim_end());
        out.push('\n');
    }
}

/// تقرير تشخيص كامل بنسخة واحدة: بيئة الجهاز + السجل الدائم + سجلات ستيم والألعاب.
/// المستخدم ينسخه ويرسله — ما يحتاج يفتح مجلدات ولا يبحث عن ملفات.
fn build_report(app: &AppHandle) -> String {
    let info = system_info(app.clone());
    let mut out = String::with_capacity(256 * 1024);
    out.push_str("============================================================\n");
    out.push_str("Jisr — full debug report\n");
    out.push_str("============================================================\n");
    out.push_str(&format!(
        "generated: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z")
    ));
    out.push_str(&format!("version: {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("os: {} {}\n", info.os_name, info.os_version));
    out.push_str(&format!(
        "arch: {} | apple_silicon: {}\n",
        info.arch, info.is_apple_silicon
    ));
    out.push_str(&format!(
        "chip: {} | cores: {:?} | memory_mb: {:?}\n",
        info.chip, info.cpu_cores, info.memory_mb
    ));
    out.push_str(&format!(
        "gpu: {}\n",
        info.gpus
            .iter()
            .map(|g| g.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    ));
    out.push_str(&format!("rosetta: {}\n", info.rosetta_installed));
    out.push_str(&format!("engine: {:?}\n", info.engine_path));
    out.push_str(&format!(
        "graphics_layer: {:?} | graphics_mode: {}\n",
        info.graphics_layer,
        graphics::mode()
    ));
    out.push_str(&format!("data_dir: {}\n", data_dir().display()));
    out.push_str(&format!(
        "bottles_dir: {}\n",
        bottle::bottles_dir().display()
    ));
    out.push_str(&format!("log_file: {:?}\n", debuglog::path()));

    if let Some(wine) = bottle::wine_bin() {
        let ver = Command::new(&wine)
            .arg("--version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        out.push_str(&format!("wine: {} ({})\n", wine.display(), ver));
    } else {
        out.push_str("wine: not installed\n");
    }

    let flags = std::fs::read_to_string(data_dir().join("cef-flags.txt")).unwrap_or_default();
    if !flags.trim().is_empty() {
        out.push_str(&format!("cef_flags_override: {}\n", flags.trim()));
    }

    // حكم سريع: هل وصل فلاقين غلاف CEF إلى ستيم فعلاً؟
    // (webhelper.txt = سطر الأمر الحقيقي اللي شغّل به ستيم الـwebhelper)
    let mut webhelper_tail = String::new();
    if let Some((mac_exe, _)) = steam::steam_exe() {
        if let Some(dir) = mac_exe.parent() {
            webhelper_tail =
                debuglog::tail_file(&dir.join("logs").join("webhelper.txt"), 400, 60_000);
        }
    }
    if webhelper_tail.trim().is_empty() {
        out.push_str(
            "cef_shim_flags_seen: (webhelper.txt غير موجود — الـwebhelper ما اشتغل بعد)\n",
        );
    } else {
        out.push_str(&format!(
            "cef_shim_flags_seen: single-process={} disable-gpu={}\n",
            webhelper_tail.contains("--single-process"),
            webhelper_tail.contains("--disable-gpu")
        ));
    }

    if let Some(p) = debuglog::path() {
        push_section(
            &mut out,
            "jisr-debug.log (last lines)",
            &debuglog::tail_file(&p, 5_000, 1_500_000),
        );
        let rotated = p.with_file_name("jisr-debug.1.log");
        push_section(
            &mut out,
            "jisr-debug.1.log (older, last lines)",
            &debuglog::tail_file(&rotated, 1_500, 500_000),
        );
    }

    // سجلات العمليات: ستيم والألعاب (كاملة في ملفاتها، وآخر جزء منها هنا)
    let logs_dir = data_dir().join("logs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&logs_dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension().and_then(|x| x.to_str()) == Some("log")
                        && !p
                            .file_name()
                            .map(|n| n.to_string_lossy().starts_with("jisr-debug"))
                            .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    files.reverse();
    for p in files.into_iter().take(5) {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        push_section(
            &mut out,
            &format!("{name} (last lines)"),
            &debuglog::tail_file(&p, 1_200, 400_000),
        );
    }

    // سجلات ستيم الرسمية — أهم источ لفهم واجهة ستيم:
    // webhelper.txt (الفلاقين الفعلية) · bootstrap_log.txt (تحقق الملفات)
    // cef_log.txt (أخطاء CEF) · وكل *.txt آخر في مجلد logs
    if let Some((mac_exe, _)) = steam::steam_exe() {
        if let Some(steam_dir) = mac_exe.parent() {
            let logs_dir = steam_dir.join("logs");
            let mut files: Vec<PathBuf> = std::fs::read_dir(&logs_dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("txt")
                        })
                        .collect()
                })
                .unwrap_or_default();
            files.sort();
            for p in files.into_iter().take(6) {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                push_section(
                    &mut out,
                    &format!("Steam/logs/{name} (last lines)"),
                    &debuglog::tail_file(&p, 250, 80_000),
                );
            }
        }
    }

    // سجل غلاف CEF — أهم ملف لتشخيص واجهة ستيم
    let dc = bottle::bottle_dir(bottle::MAIN_ID).join("drive_c");
    for base in ["Program Files (x86)", "Program Files"] {
        let cef_root = dc.join(base).join("Steam").join("bin").join("cef");
        let Ok(rd) = std::fs::read_dir(&cef_root) else {
            continue;
        };
        for e in rd.flatten() {
            let shim_log = e.path().join("steamwebhelper_shim.log");
            if shim_log.is_file() {
                push_section(
                    &mut out,
                    &format!("{} (last lines)", shim_log.display()),
                    &debuglog::tail_file(&shim_log, 400, 128_000),
                );
            }
        }
    }

    // نحفظ نسخة على القرص + نُبقي آخر 5 تقارير فقط
    let stamp = now_ms();
    let saved = logs_dir.join(format!("jisr-report-{stamp}.txt"));
    if let Err(e) = std::fs::write(&saved, &out) {
        out.push_str(&format!("\n\n(report save failed: {e})\n"));
    } else {
        out.push_str(&format!("\n\nreport_file: {}\n", saved.display()));
        if let Ok(rd) = std::fs::read_dir(&logs_dir) {
            let mut reports: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().starts_with("jisr-report-"))
                        .unwrap_or(false)
                })
                .collect();
            reports.sort();
            reports.reverse();
            for old in reports.into_iter().skip(5) {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    out
}

/// تقرير جاهز للنسخ — يُبنى في الخلفية حتى لا تتجمّد الواجهة
#[tauri::command]
async fn export_debug_report(app: AppHandle) -> Result<String, String> {
    let _t = debuglog::trace("log", "export_debug_report", "");
    tauri::async_runtime::spawn_blocking(move || build_report(&app))
        .await
        .map_err(|e| format!("تعذّر تجهيز التقرير: {e}"))
}

// ============================================================
//  التشغيل
// ============================================================
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // أي انهيار (panic) يُكتب في السجل الدائم مع تتبّع كامل — أهم أداة تشخيص ميداني
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        debuglog::raw("error", "panic", &format!("{info}\n{bt}"));
    }));

    tauri::Builder::default()
        // مهم: «النسخة الواحدة» لازم تُسجّل أول إضافة — نمنع نسختين تتخانق على
        // نفس القارورة أو تشغّل ستيم مرتين (وهو مصدر نافذتين/تعارض ملفات)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            debuglog::raw(
                "info",
                "app",
                "single-instance activation — focus main window",
            );
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(LogState::default())
        .manage(download::DownloadsState::default())
        .manage(bottle::BottlesState::default())
        .manage(game::GamesState::default())
        .manage(steam::SteamState::default())
        .invoke_handler(tauri::generate_handler![
            system_info,
            get_logs,
            clear_logs,
            log_line,
            debug_log,
            debug_log_path,
            export_debug_report,
            download::download_catalog,
            download::download_start,
            download::download_cancel,
            download::downloads_snapshot,
            bottle::bottles_list,
            bottle::bottle_create,
            bottle::bottle_retry,
            bottle::bottle_delete,
            game::games_list,
            game::game_add,
            game::game_remove,
            game::game_launch,
            game::game_stop,
            steam::steam_status,
            steam::steam_install,
            steam::steam_run,
            steam::steam_stop,
            steam_library::steam_games_list,
            steam_library::steam_game_launch,
            graphics::graphics_mode,
            graphics::graphics_set_mode
        ])
        .setup(|app| {
            // السجل الدائم أول شيء — حتى أخطاء الإقلاع تنتسجّل
            let log_path = debuglog::init(&data_dir().join("logs"), app.handle().clone());

            let os_version = if cfg!(target_os = "macos") {
                run_cmd("sw_vers", &["-productVersion"]).unwrap_or_default()
            } else {
                std::env::consts::OS.to_string()
            };
            debuglog::raw(
                "info",
                "startup",
                &format!(
                    "Jisr v{} | os={} | arch={} | data={}",
                    env!("CARGO_PKG_VERSION"),
                    os_version,
                    std::env::consts::ARCH,
                    data_dir().display()
                ),
            );
            debuglog::raw(
                "info",
                "startup",
                &format!(
                    "log={} | rosetta={} | gstreamer={}",
                    log_path.display(),
                    std::path::Path::new("/Library/Apple/usr/libexec/oah").exists(),
                    std::path::Path::new("/Library/Frameworks/GStreamer.framework").exists(),
                ),
            );
            match bottle::wine_bin() {
                Some(w) => {
                    let ver = std::process::Command::new(&w)
                        .arg("--version")
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_default();
                    debuglog::raw(
                        "info",
                        "startup",
                        &format!("wine={} | version={}", w.display(), ver),
                    );
                }
                None => debuglog::raw("warn", "startup", "wine: not installed"),
            }
            debuglog::raw(
                "info",
                "startup",
                &format!(
                    "graphics_mode={} | steam_installed={}",
                    graphics::mode(),
                    steam::steam_exe().is_some()
                ),
            );

            // نظافة: نمسح بقايا أي فك ضغط مات في جلسة سابقة
            engine::cleanup_staging();
            graphics::cleanup_staging();

            // نحمّل مكتبة الألعاب المحفوظة
            game::init(app.handle());

            log(
                app.handle(),
                "info",
                log_keys::STARTUP,
                json!({
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                }),
            );

            // مستخدم عنده محرك من نسخة سابقة: نجهّز DXVK بالخلفية (2.8MB)
            if bottle::wine_bin().is_some() {
                download::ensure_graphics_layer(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
