// ============================================================
//  جسر — الخلفية (Rust)
//  • فحص الجهاز الحقيقي (نفس الكود على macOS و Windows)
//  • سجل أحداث مباشر يوصل للواجهة عبر Tauri events
// ============================================================
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

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

#[derive(Serialize, Clone, Default)]
pub struct LogEntry {
    pub time_ms: u64,
    pub level: String,
    pub message: String,
}

#[derive(Default)]
pub struct LogState(pub Mutex<Vec<LogEntry>>);

const MAX_LOGS: usize = 800;

// ============================================================
//  السجل
// ============================================================
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// نكتب سطر في السجل + نرسله للواجهة فوراً (live)
fn log(app: &AppHandle, level: &str, message: &str) {
    let entry = LogEntry {
        time_ms: now_ms(),
        level: level.to_string(),
        message: message.to_string(),
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
//  أدوات تنفيذ أوامر النظام
// ============================================================
fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// نفس الشي لكن مع متغيّر بيئة (مثلاً نجبر الإخراج يكون إنجليزي)
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

/// مجلد بيانات جسر:
/// macOS: ~/Library/Application Support/Jisr
/// Windows: %LOCALAPPDATA%\Jisr
fn data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Jisr")
    } else {
        let base = std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("APPDATA"))
            .unwrap_or_default();
        PathBuf::from(base).join("Jisr")
    }
}

/// هل عندنا محرك مثبت؟ وشنو طبقة الرسومات؟
fn engine_info() -> (bool, Option<String>, Option<String>) {
    let dir = data_dir().join("engines");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return (false, None, None);
    };

    for e in entries.flatten() {
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        let gfx = detect_graphics_layer(&path);
        return (true, path.to_str().map(String::from), gfx);
    }

    (false, None, None)
}

/// نبحث في مجلد المحرك عن ملفات الطبقات المعروفة
fn detect_graphics_layer(engine_path: &std::path::Path) -> Option<String> {
    let mut has_dxmt = false;
    let mut has_dxvk = false;
    let mut has_moltenvk = false;

    let mut stack = vec![engine_path.to_path_buf()];
    let mut visited = 0;

    while let Some(dir) = stack.pop() {
        visited += 1;
        if visited > 60 {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if name.contains("dxmt") {
                has_dxmt = true;
            } else if name.contains("dxvk") {
                has_dxvk = true;
            } else if name.contains("moltenvk") {
                has_moltenvk = true;
            }
        }
    }

    if has_dxmt {
        Some("DXMT".into())
    } else if has_dxvk {
        Some(if has_moltenvk { "DXVK + MoltenVK" } else { "DXVK" }.into())
    } else {
        None
    }
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
        // Rosetta 2: مسارها المعروف على أبل سيليكون
        info.rosetta_installed = cfg!(target_arch = "aarch64")
            && std::path::Path::new("/Library/Apple/usr/libexec/oah").exists();
    } else {
        windows_info(&mut info);
        info.os_name = if info.os_name.is_empty() {
            std::env::consts::OS.to_string()
        } else {
            info.os_name.clone()
        };
    }

    let (engine, path, gfx) = engine_info();
    info.engine_installed = engine;
    info.engine_path = path;
    info.graphics_layer = gfx;

    info
}

/// كروت الشاشة على الماك — نستخدم إخراج JSON (ما يتأثر بلغة النظام)
fn macos_gpus() -> Vec<GpuInfo> {
    let Some(out) = run_env(
        "system_profiler",
        &["-json", "SPDisplaysDataType"],
        &[("LC_ALL", "en_US.UTF-8")],
    ) else {
        return vec![];
    };

    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else {
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

/// نص مهما كان نوعه (نص أو رقم)
fn as_text(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
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
//  أوامر Tauri (تنادى من الواجهة)
// ============================================================

/// فحص الجهاز + كتابة النتائج في السجل
#[tauri::command]
fn system_info(app: AppHandle) -> SystemInfo {
    let info = detect_system();

    log(
        &app,
        "info",
        &format!(
            "فحص الجهاز: {} {} ({})",
            if info.os_name.is_empty() {
                "?"
            } else {
                &info.os_name
            },
            info.os_version,
            info.arch
        ),
    );

    if !info.chip.is_empty() {
        log(
            &app,
            "info",
            &format!(
                "المعالج: {}{}",
                info.chip,
                info.cpu_cores
                    .map(|c| format!(" — {} نواة", c))
                    .unwrap_or_default()
            ),
        );
    }

    if let Some(mb) = info.memory_mb {
        log(&app, "info", &format!("الذاكرة: {} جيجابايت", mb / 1024));
    }

    for g in &info.gpus {
        log(
            &app,
            "info",
            &format!(
                "كرت الشاشة: {}{}",
                g.name,
                g.cores.map(|c| format!(" — {} نواة", c)).unwrap_or_default()
            ),
        );
    }

    if info.is_apple_silicon {
        if info.rosetta_installed {
            log(&app, "info", "Rosetta 2 مثبتة ✓");
        } else {
            log(
                &app,
                "warn",
                "Rosetta 2 غير مثبتة — نحتاجها لتشغيل ألعاب ويندوز",
            );
        }
    }

    if info.engine_installed {
        log(
            &app,
            "info",
            &format!(
                "محرك Wine مثبت: {}{}",
                info.engine_path.clone().unwrap_or_default(),
                info.graphics_layer
                    .as_ref()
                    .map(|g| format!(" — طبقة الرسومات: {}", g))
                    .unwrap_or_default()
            ),
        );
    } else {
        log(&app, "warn", "محرك Wine غير مثبت بعد");
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
    log(&app, "info", "تم تفريغ السجل");
}

/// الواجهة تقدر تكتب في السجل (للأحداث اللي تصير في الواجهة)
#[tauri::command]
fn log_line(app: AppHandle, level: String, message: String) {
    log(&app, &level, &message);
}

// ============================================================
//  التشغيل
// ============================================================
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(LogState::default())
        .invoke_handler(tauri::generate_handler![
            system_info,
            get_logs,
            clear_logs,
            log_line
        ])
        .setup(|app| {
            log(
                app.handle(),
                "info",
                &format!(
                    "بدأ جسر — {}/{}",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ),
            );
            log(
                app.handle(),
                "info",
                &format!("مجلد البيانات: {}", data_dir().display()),
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
