// ============================================================
//  جسر — الخلفية (Rust)
//  • فحص الجهاز الحقيقي (نفس الكود على macOS و Windows)
//  • سجل أحداث يُرسل للواجهة **بمفاتيح/معاملات** (لا نصوص) — الترجمة في الواجهة
// ============================================================
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

mod bottle;
mod download;
mod engine;

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
}

const MAX_LOGS: usize = 800;

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// نضيف سطر للسجل + نرسله للواجهة فوراً
pub(crate) fn log(app: &AppHandle, level: &str, key: &str, params: Value) {
    let entry = LogEntry {
        time_ms: now_ms(),
        level: level.to_string(),
        key: key.to_string(),
        params,
    };

    if let Some(state) = app.try_state::<LogState>() {
        if let Ok(mut v) = state.0.lock() {
            v.push(entry.clone());
            if v.len() > MAX_LOGS {
                let excess = v.len() - MAX_LOGS;
                v.drain(0..excess);
            }
        }
    }
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
            std::env::var("LOCALAPPDATA").or_else(|_| std::env::var("APPDATA")).unwrap_or_default(),
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

    let (engine, path, gfx) = engine_info();
    info.engine_installed = engine;
    info.engine_path = path;
    info.graphics_layer = gfx;
    info
}

/// كروت الشاشة على الماك — JSON (ما يتأثر بلغة النظام)
fn macos_gpus() -> Vec<GpuInfo> {
    let Some(out) = run_env("system_profiler", &["-json", "SPDisplaysDataType"], &[("LC_ALL", "en_US.UTF-8")]) else {
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

    let Some(out) = run_cmd("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]) else {
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
    log(&app, "info", log_keys::CLEARED, json!({}));
}

/// الواجهة تكتب سجل (بمفتاح محلي)
#[tauri::command]
fn log_line(app: AppHandle, level: String, key: String, params: Option<Value>) {
    log(&app, &level, &key, params.unwrap_or(json!({})));
}

// ============================================================
//  التشغيل
// ============================================================
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(LogState::default())
        .manage(download::DownloadsState::default())
        .manage(bottle::BottlesState::default())
        .invoke_handler(tauri::generate_handler![
            system_info,
            get_logs,
            clear_logs,
            log_line,
            download::download_catalog,
            download::download_start,
            download::download_cancel,
            download::downloads_snapshot,
            bottle::bottles_list,
            bottle::bottle_create,
            bottle::bottle_retry,
            bottle::bottle_delete
        ])
        .setup(|app| {
            // نظافة: نمسح بقايا أي فك ضغط مات في جلسة سابقة
            engine::cleanup_staging();

            log(
                app.handle(),
                "info",
                log_keys::STARTUP,
                json!({
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                }),
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
