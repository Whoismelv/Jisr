// ============================================================
//  جسر — السجل التشخيصي الكامل (debug log)
//  كل حدث وكل أمر وكل خطأ يُكتب في ملف واحد دائم:
//  ~/Library/Application Support/Jisr/logs/jisr-debug.log
//  نفس السطور تظهر خاماً في صفحة «السجل» — وآخر تقرير يُنسخ بزر واحد.
// ============================================================
use serde_json::json;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

const FILE_NAME: &str = "jisr-debug.log";
/// الحد الأقصى قبل التدوير (نحتفظ بنسخة واحدة قديمة .1.log)
const MAX_BYTES: u64 = 8 * 1024 * 1024;
/// أقصى طول لسطر واحد — حماية من رسائل ضخمة (مخرجات ويندوز مثلاً)
const MAX_CHARS: usize = 8_000;

struct Sink {
    file: Option<File>,
    path: PathBuf,
    written: u64,
}

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

/// نفتح السجل مرة واحدة عند إقلاع التطبيق. نُدوّر الملف مسبقاً لو تجاوز الحد.
pub fn init(dir: &Path, app: AppHandle) -> PathBuf {
    let path = init_path(dir);
    let _ = APP.set(app);
    path
}

/// نفس التهيئة بدون واجهة (للاختبارات ومحاكاة البيئة)
pub fn init_path(dir: &Path) -> PathBuf {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join(FILE_NAME);

    let rotate = std::fs::metadata(&path)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false);
    if rotate {
        rotate_file(&path);
    }

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    let written = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    match SINK.get() {
        // السجل مفتوح مسبقاً (محاكاة/اختبار) → نعيد توجيهه للمجلد الجديد
        Some(m) => {
            if let Ok(mut sink) = m.lock() {
                sink.file = file;
                sink.path = path.clone();
                sink.written = written;
            }
        }
        None => {
            let _ = SINK.set(Mutex::new(Sink {
                file,
                path: path.clone(),
                written,
            }));
        }
    }
    path
}

pub fn path() -> Option<PathBuf> {
    SINK.get()
        .and_then(|m| m.lock().ok().map(|s| s.path.clone()))
}

fn rotate_file(path: &Path) {
    let old = path.with_file_name("jisr-debug.1.log");
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::rename(path, &old);
}

impl Sink {
    fn rotate(&mut self) {
        self.file.take();
        rotate_file(&self.path);
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .ok();
        self.written = 0;
    }

    fn write(&mut self, level: &str, scope: &str, msg: &str) {
        let text = format_block(level, scope, msg);
        let bytes = text.len() as u64;

        if self.written + bytes > MAX_BYTES {
            self.rotate();
        }

        if let Some(f) = self.file.as_mut() {
            if f.write_all(text.as_bytes()).is_ok() {
                // flush فوري: لو انهار التطبيق أو انقطعت الكهرباء يبقى السجل على القرص
                let _ = f.flush();
                self.written = self.written.saturating_add(bytes);
            }
        }
    }
}

fn format_block(level: &str, scope: &str, msg: &str) -> String {
    let ts = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f %:z")
        .to_string();
    let scope: String = scope.chars().take(40).collect();
    let msg: String = msg.chars().take(MAX_CHARS).collect();
    let mut out = String::with_capacity(msg.len() + 80);
    let mut first = true;
    for line in msg.lines() {
        if first {
            out.push_str(&format!(
                "{ts} [{:<5}] [{scope}] {line}\n",
                level.to_ascii_uppercase()
            ));
            first = false;
        } else {
            // سطور متعددة (backtrace مثلاً) — نزّلها تحت نفس الحدث بتزحيف واضح
            out.push_str(&format!("                              │ {line}\n"));
        }
    }
    if msg.is_empty() {
        out.push_str(&format!(
            "{ts} [{:<5}] [{scope}] \n",
            level.to_ascii_uppercase()
        ));
    }
    out
}

/// كتابة خام في الملف فقط (بدون بثّ للواجهة).
pub fn write(level: &str, scope: &str, msg: &str) {
    if let Some(m) = SINK.get() {
        if let Ok(mut sink) = m.lock() {
            sink.write(level, scope, msg);
        }
    }
}

/// كتابة خام + بثّ فوري للواجهة (تظهر في صفحة السجل حرفياً).
pub fn raw(level: &str, scope: &str, msg: &str) {
    write(level, scope, msg);
    if let Some(app) = APP.get() {
        let line: String = format!("[{scope}] {msg}").chars().take(4_000).collect();
        let entry = crate::LogEntry {
            time_ms: crate::now_ms(),
            level: level.to_string(),
            key: "log.raw".to_string(),
            params: json!({ "line": line }),
        };
        crate::remember(app, &entry);
        let _ = app.emit("jisr-log", entry);
    }
}

/// مقياس زمني لأمر Tauri: يسجّل البداية والنهاية والمدة (حتى لو رجع بخطأ).
pub struct CmdTrace {
    scope: &'static str,
    action: &'static str,
    start: Instant,
}

impl Drop for CmdTrace {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_millis();
        raw(
            "debug",
            self.scope,
            &format!("← {} ({}ms)", self.action, ms),
        );
    }
}

pub fn trace(scope: &'static str, action: &'static str, details: impl AsRef<str>) -> CmdTrace {
    let d = details.as_ref();
    if d.is_empty() {
        raw("debug", scope, &format!("→ {action}"));
    } else {
        raw("debug", scope, &format!("→ {action} {d}"));
    }
    CmdTrace {
        scope,
        action,
        start: Instant::now(),
    }
}

/// تفريغ السجل الحالي (زر «تفريغ»): يبدأ الملف من جديد مع سطر توضيحي.
pub fn clear() {
    if let Some(m) = SINK.get() {
        if let Ok(mut sink) = m.lock() {
            sink.file.take();
            match OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&sink.path)
            {
                Ok(f) => {
                    sink.file = Some(f);
                    sink.written = 0;
                }
                Err(_) => {
                    sink.file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&sink.path)
                        .ok();
                }
            }
        }
        raw(
            "info",
            "log",
            "--- تم تفريغ السجل بناءً على طلب المستخدم ---",
        );
    }
}

/// يفكّ سطر السجل الخام: (الوقت، المستوى، النص بدون الطابع الزمني)
/// شكل السطر: "2026-10-20 12:34:56.789 +03:00 [INFO ] [scope] الرسالة"
fn parse_line(line: &str) -> (u64, &'static str, String) {
    let level = if line.contains("[ERROR]") {
        "error"
    } else if line.contains("[WARN ]") {
        "warn"
    } else if line.contains("[DEBUG]") {
        "debug"
    } else {
        "info"
    };

    let stamp = line
        .get(..23)
        .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f").ok());
    let time_ms = stamp
        .and_then(|ndt| ndt.and_local_timezone(chrono::Local).single())
        .map(|dt| dt.timestamp_millis() as u64)
        .unwrap_or_else(crate::now_ms);

    let mut rest = line.get(23..).unwrap_or(line).trim_start();
    if rest.starts_with('+') || rest.starts_with('-') {
        // نفصل فرق التوقيت (+03:00) عن باقي السطر
        rest = rest.splitn(2, ' ').nth(1).unwrap_or(rest).trim_start();
    }
    if rest.starts_with('[') {
        // نشيل رمز المستوى ([ERROR]) ونبقي [scope] الرسالة — مثل السطر الحي
        if let Some(end) = rest.find(']') {
            rest = rest[end + 1..].trim_start();
        }
    }
    (time_ms, level, rest.to_string())
}

/// نرجّع آخر سطور **الجلسة السابقة** إلى واجهة السجل عند الإقلاع.
/// السبب: بعد إعادة تشغيل الماك أو التطبيق كانت القائمة تظهر فاضية
/// ويظن المستخدم أن السجل «اختفى» — والمهم فيه بالضبط ما صار قبل الإغلاق.
pub fn seed(app: &AppHandle, max_lines: usize) {
    let Some(p) = path() else { return };
    let text = tail_file(&p, max_lines, 400_000);
    if text.trim().is_empty() {
        return;
    }
    let Some(state) = app.try_state::<crate::LogState>() else {
        return;
    };
    let Ok(mut v) = state.0.lock() else { return };
    v.clear();

    for line in text.lines() {
        let (time_ms, level, body) = parse_line(line);
        v.push(crate::LogEntry {
            time_ms,
            level: level.to_string(),
            key: "log.raw".to_string(),
            params: json!({ "line": body }),
        });
    }
    v.push(crate::LogEntry {
        time_ms: crate::now_ms(),
        level: "info".to_string(),
        key: "log.raw".to_string(),
        params: json!({ "line": "────────── ↑ سجل الجلسة السابقة · ما تحت = الجلسة الحالية ──────────" }),
    });
}

/// آخر أسطر من ملف كبير بدون تحميله كاملاً (seek من النهاية).
pub fn tail_file(path: &Path, max_lines: usize, max_bytes: usize) -> String {
    let Ok(mut f) = File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes as u64);

    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    let _ = f.take(max_bytes as u64).read_to_end(&mut buf);

    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        // أول سطر غالباً مقطوع في منتصفه — نستبعده
        lines.remove(0);
    }
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_file_returns_last_lines_only() {
        let dir = std::env::temp_dir().join(format!("jisr-log-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("x.log");
        let body: String = (1..=50).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&p, body).unwrap();

        let tail = tail_file(&p, 5, 4096);
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines.last(), Some(&"line 50"));
        // قراءة من المنتصف لا تُرجع سطراً مقطوعاً
        let tiny = tail_file(&p, 10, 40);
        assert!(tiny.lines().all(|l| l.starts_with("line ")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_block_prefixes_multiline() {
        let text = format_block("error", "panic", "أول سطر\nثاني سطر");
        assert!(text.contains("[ERROR] [panic] أول سطر"));
        assert!(text.contains("│ ثاني سطر"));
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn format_block_caps_huge_messages() {
        let huge = "ا".repeat(MAX_CHARS * 2);
        let text = format_block("info", "x", &huge);
        assert!(text.chars().count() < MAX_CHARS + 120);
    }

    #[test]
    fn parse_line_extracts_level_and_body() {
        let line = "2026-10-20 12:34:56.789 +03:00 [ERROR] [steam] فشل التشغيل";
        let (ms, level, body) = parse_line(line);
        assert_eq!(level, "error");
        assert!(body.starts_with("[steam]"));
        assert!(body.contains("فشل التشغيل"));
        assert!(
            ms > 1_700_000_000_000,
            "وقت السطر لازم يكون بصيغة ميلي ثانية"
        );
    }

    #[test]
    fn parse_line_survives_garbage() {
        let (ms, level, body) = parse_line("سطر مكسور بدون طابع");
        assert_eq!(level, "info");
        assert_eq!(body, "سطر مكسور بدون طابع");
        assert!(ms > 0);
    }
}
