// ============================================================
//  جسر — تشخيص انهيار Wine: تقرير winedbg → حكم قابل للتنفيذ
// ============================================================
//! **المشكلة الميدانية:** Wine يكتب تقرير انهيار (`winedbg` auto-mode) على
//! stderr اللعبة. التقرير فيه كل التفاصيل (أي مكتبة انحرفت، وهل هي نسخة Wine
//! أم نسخة أصلية) — لكنه بالعربي غير مفهوم، والمستخدم يحتاج «ماذا أفعل الآن».
//!
//! **ليش هذا الملف:** التحويل من «إحصاء انهيار» إلى «قرار» — صنف الخطأ +
//! خطوة تالية واحدة + أدلة ملموسة. التطبيق يحوّل `action` إلى تبديل آلي
//! لطبقة الرسوميات (DXVK / WineD3D / D3DX أصلي / تعطيل Overlay) أو يعرض
//! توصية للمستخدم.
//!
//! **دلالات الجدول (متحقَّق منها من كود Wine نفسه — لا تخمّن):**
//! • عمود النوع في `Modules:` هو `PE-Wine` للمكتبات **المدمجة في Wine**،
//!   و`PE` للمكتبات **الأصلية/الخارجية** (DXVK، d3dx9_43 الأصلي، ملفات اللعبة).
//! • العمود الثالث (`--none--` / `Deferred` / `Export` / `PDB`) هو **حالة
//!   معلومات التصحيح** وليس حالة التحميل! `Deferred` تعني «ما طلبنا الرموز»
//!   ⇒ المكتبة **مُحمَّلة** لكنها ليست على مسار الانهيار.
//!   لا تُعامل `Deferred` أبداً كـ«غير محمّلة».
//! • بعض الصفوف صور PE **مدمجة داخل** ملفات unix (تبدأ بـ `\-`) — هي ليست
//!   مكتبات محمّلة، وتُستبعد من نسبة الانهيار إلى طبقة معيّنة.
//!
//! **مبدأ السلامة:** أي نص مشوّه يرجع `None` أو تصنيف `Unknown` —
//! ما في `unwrap` ولا `panic` في كود الإنتاج (ملف التشخيص ما يصح يطير التطبيق).

use serde::{Deserialize, Serialize};

/// صف واحد من جدول `Modules:` في تقرير winedbg.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleRow {
    /// اسم الوحدة كما في التقرير (عادةً بلا امتداد: `d3dx9_43`).
    pub name: String,
    /// `true` إذا كانت PE أصلية/خارجية (DXVK، d3dx9_43 الأصلي، ملفات اللعبة).
    pub native: bool,
    /// `true` إذا كانت PE مدمجة في Wine (`PE-Wine`).
    pub builtin: bool,
    /// `true` إذا الرموز «مؤجَّلة» (Deferred) — أي أن المكتبة **مُحمَّلة**
    /// لكن بدون معلومات تصحيح (وهي ليست على مسار الانهيار غالباً).
    pub deferred: bool,
    /// `true` إذا كانت صورة PE مدمجة داخل ملف unix (وليست مكتبة محمّلة).
    pub embedded: bool,
    /// حالة معلومات التصحيح كما في التقرير (`--none--`, `Deferred`, `Export`, `PDB`).
    pub dbg_info: String,
    /// عنوان الأساس في ذاكرة الضيف — نحتاجه لنعرف أي وحدة هي ملف اللعبة
    /// (أقل عنوان بين الوحدات غير المدمجة). `0` إذا تعذّر قراءته.
    pub base: u64,
}

/// إطار واحد من «مسار الانهيار» (Backtrace).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// رقم الإطار بالترتيب (0 = أول إطار).
    pub index: u32,
    /// عنوان التعليمة داخل الإطار.
    pub addr: u64,
    /// اسم المكتبة/الملف (None إذا الإطار بلا اسم مكتبة).
    pub module: Option<String>,
    /// الإزاحة داخل المكتبة (`+0x…`).
    pub offset: Option<u64>,
    /// `true` للإطار المتسبب في الانهيار (السطر الذي يبدأ بـ `=>`).
    pub is_crash_frame: bool,
}

/// تقرير الانهيار بعد التحليل.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrashReport {
    /// سطر «Unhandled exception: …» كما هو في التقرير.
    pub exception_line: Option<String>,
    /// وصف الخطأ بلا تفاصيل العنوان: «page fault on read access to 0x00000000».
    pub fault_kind: String,
    /// عنوان الوصول المتعثّر (من «access to 0x…») أو None لغيره.
    pub fault_addr: Option<u64>,
    /// معمارية الضيف: 32 أو 64، و0 إذا ما عرفنا.
    pub guest_bits: u8,
    /// `true` إذا البرنامج 32-بت يشتغل فوق Wine 64-بت (WoW64).
    pub wow64: bool,
    /// إصدار Wine (من «Wine build: wine-11.18»).
    pub wine_build: Option<String>,
    /// إطارات مسار الانهيار.
    pub frames: Vec<Frame>,
    /// جدول المكتبات المحمّلة.
    pub modules: Vec<ModuleRow>,
    /// طول النص الأصلي (بالبايت) — للمقارنة والعرض.
    pub raw_len: usize,
    /// `true` إذا النص فيه بصمة خطأ الـOverlay: «CreateDevice hook called».
    pub overlay_hook: bool,
}

/// صنف الخطأ — يحدّد **نوع** الإصلاح المطلوب، مو الإصلاح نفسه.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultClass {
    /// انهيار داخل d3dx9/d3dx10/d3dx11/d3dcompiler **المدمجة في Wine** (ناقصة).
    D3dxBuiltin,
    /// انهيار داخل نسخة **مايكروسوفت** الأصلية من d3dx9/d3dcompiler.
    D3dxNative,
    /// طبقة الجهاز هي WineD3D المدمج (d3d9/wined3d) — أي DXVK مو مُفعّل.
    D3d9Builtin,
    /// d3d9 الخاص بـDXVK هو النشط، والانهيار داخله.
    D3d9Native,
    /// انهيار داخل طبقة Vulkan (winevulkan / vulkan-1 / MoltenVK).
    Vulkan,
    /// انهيار 32-بت داخل طبقة تحويل WoW64 (wow64cpu / ntdll) — حساسية Rosetta.
    Wow64Thunk,
    /// «unimplemented function <dll>.<fn> called» — DLL ناقصة/خاطئة.
    MissingDll,
    /// ستيم الـOverlay يعترض على Direct3D («CreateDevice hook called»).
    OverlayHook,
    /// الانهيار داخل ملف اللعبة نفسه.
    GameItself,
    /// ما قدرنا نحدد السبب — نحتاج سجل WINEDEBUG مركّز.
    Unknown,
}

/// الحكم النهائي: ماذا حدث + ماذا نفعل.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    /// صنف الخطأ.
    pub class: FaultClass,
    /// المكتبة/الملف الذي انهار داخله (إن عرفنا).
    pub crashed_module: Option<String>,
    /// جملة واحدة بالعربي تشرح ما حدث (بدون مصطلحات).
    pub headline_ar: String,
    /// الخطوة التالية بالعربي — محدّدة وقابلة للتنفيذ.
    pub next_ar: String,
    /// رمز آلي قصير للإجراء (مثل «enable-dxvk»).
    pub action: String,
    /// أدلة ملموسة من التقرير (أسماء مكتبات وصفوفها).
    pub evidence: Vec<String>,
}

// ------------------------------------------------------------
//  ثوابت
// ------------------------------------------------------------

/// المكتبات الأساسية لويندوز: «unimplemented function» فيها تعني خلل في
/// Wine نفسه (أو DLL ناقص من داخل البريفكس) — مو DLL مفقودة نثبّتها للعبة.
const CORE_DLLS: [&str; 6] = ["kernel32", "ntdll", "user32", "gdi32", "msvcrt", "ucrtbase"];

/// بصمة خطأ الـOverlay في تقارير winedbg (يطبعها Hook الستيم عند CreateDevice).
const OVERLAY_MARKER: &str = "CreateDevice hook called";

/// قيم عمود «Debug info» في جدول Wine — **حالة الرموز** مو حالة التحميل.
const DBG_STATUSES: [&str; 12] = [
    "--none--", "none", "deferred", "export", "exported", "pdb", "dbg", "debug", "symbol",
    "symbols", "unknown", "stabs",
];

// ------------------------------------------------------------
//  أدوات نصّية صغيرة (بدون regex — ما نضيف اعتماديات)
// ------------------------------------------------------------

/// `strip_prefix` يتجاهل حالة الأحرف — رسائل Wine تختلف بين الإصدارات.
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    let tail = text.get(prefix.len()..)?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(tail)
    } else {
        None
    }
}

/// يبحث عن نص يتجاهل حالة الأحرف (بدون تخصيص ذاكرة للنص كامل).
fn contains_ci(haystack: &str, needle: &str) -> bool {
    let hay = haystack.to_ascii_lowercase();
    let pin = needle.to_ascii_lowercase();
    hay.contains(&pin)
}

/// اسم وحدة موحّد: أحرف صغيرة، بلا امتداد (`d3dx9_43.dll` → `d3dx9_43`).
fn normalize(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    for ext in [".dll", ".exe", ".so", ".dylib"] {
        if let Some(base) = lower.strip_suffix(ext) {
            return base.to_string();
        }
    }
    lower
}

/// يحوّل نص سداسي عشري (بلا `0x`) إلى رقم — يرجع None عند الفشل.
fn parse_hex(text: &str) -> Option<u64> {
    let t = text.trim();
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(t, 16).ok()
}

/// عمود «Debug info»: نتحقق قبل ما نعتبره عمود رموز (وإلا حسبنا العنوان خطأ).
fn is_dbg_status(token: &str) -> bool {
    let t = token.trim().to_ascii_lowercase();
    DBG_STATUSES.contains(&t.as_str())
}

/// اسم القسم من سطر عنوان مثل «Backtrace:».
fn section_of(line: &str) -> Option<&'static str> {
    let t = line.trim().strip_suffix(':')?;
    match t.to_ascii_lowercase().as_str() {
        "register dump" => Some("register"),
        "stack dump" => Some("stack"),
        "backtrace" => Some("backtrace"),
        "modules" => Some("modules"),
        "threads" => Some("threads"),
        "system information" => Some("sysinfo"),
        _ => None,
    }
}

/// معمارية من اسم («x86_64» → 64، «i386» → 32).
fn bits_of(name: &str) -> u8 {
    let n = name.trim().to_ascii_lowercase();
    if n.contains("64") {
        64
    } else if n.contains("86") {
        32
    } else {
        0
    }
}

/// «x86_64 (guest: i386)» → (مضيف 64، ضيف 32).
fn parse_platform(text: &str) -> (u8, u8) {
    let lower = text.to_ascii_lowercase();
    let host = lower.split_whitespace().next().map(bits_of).unwrap_or(0);
    let mut guest = 0;
    if let Some(pos) = lower.find("guest:") {
        let rest = text.get(pos + "guest:".len()..).unwrap_or("");
        let name: String = rest
            .chars()
            .take_while(|c| *c != ')' && *c != ' ' && *c != '\t')
            .collect();
        guest = bits_of(&name);
    }
    (host, guest)
}

// ------------------------------------------------------------
//  التحليل
// ------------------------------------------------------------

/// يحلّل وصف الاستثناء → (وصف الخطأ، عنوان الخطأ، هل ذُكر WoW64).
///
/// **ليش نفصل «في wow64 32-bit code»:** جملة Wine تلك ليست جزءاً من نوع
/// الخطأ — هي معلومة مستقلة: نحتاجها لمعرفة المقاس ولتصنيف انهيار Rosetta.
fn parse_exception(desc: &str) -> (String, Option<u64>, bool) {
    let mut s = desc.trim().to_string();
    // نقطة نهاية الجملة
    while s.ends_with('.') {
        s.pop();
        s = s.trim_end().to_string();
    }
    let wow64 = s.contains("wow64 32-bit code");
    // عنوان الوصول المتعثّر: «read access to 0x00000000»
    let addr = fault_addr_of(&s);
    // «(0x…)» في النهاية = عنوان التعليمة المتعثرة، مو عنوان الخطأ
    if s.ends_with(')')
        && let Some(open) = s.rfind(" (0x")
    {
        s.truncate(open);
        s = s.trim_end().to_string();
    }
    // لاحقة المقاس في نهاية الوصف
    for suffix in [
        " in wow64 32-bit code",
        " in 32-bit code",
        " in 64-bit code",
    ] {
        if let Some(pos) = s.find(suffix) {
            s.truncate(pos);
            s = s.trim_end().to_string();
        }
    }
    (s, addr, wow64)
}

/// عنوان الوصول المتعثّر من نص مثل «read access to 0x00000000».
fn fault_addr_of(desc: &str) -> Option<u64> {
    let lower = desc.to_ascii_lowercase();
    let pos = lower.find("access to 0x")? + "access to 0x".len();
    let hex: String = desc
        .get(pos..)?
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    parse_hex(&hex)
}

/// يحلّل سطر إطار واحد من «Backtrace:».
///
/// الصيغة: `=>0 0x7b74c870 in ucrtbase (+0x5c870) (0x0026f584)`
/// أو `  6 0x63616c70 (0x6552202f)` (إطار بلا اسم مكتبة).
fn parse_frame(line: &str) -> Option<Frame> {
    let mut s = line.trim();
    let mut is_crash_frame = false;
    if let Some(rest) = s.strip_prefix("=>") {
        is_crash_frame = true;
        s = rest.trim_start();
    } else if let Some(rest) = s.strip_prefix("->") {
        s = rest.trim_start();
    }
    // رقم الإطار (اختياري)
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let mut index = 0u32;
    if !digits.is_empty() {
        index = digits.parse().ok()?;
        s = s.get(digits.len()..)?.trim_start();
    }
    // عنوان التعليمة
    let rest = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    let hex: String = rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() {
        return None;
    }
    let addr = u64::from_str_radix(&hex, 16).ok()?;
    let tail = rest.get(hex.len()..)?;

    // اسم المكتبة: ما بعد « in » حتى أول مسافة أو قوس
    let mut module = None;
    let mut offset = None;
    if let Some(pos) = tail.find(" in ") {
        let after = tail.get(pos + 4..)?;
        let end = after.find([' ', '(']).unwrap_or(after.len());
        let name = after.get(..end)?.trim();
        if !name.is_empty() {
            // بعض إصدارات winedbg تكتب «module+0x1234»
            match name.split_once('+') {
                Some((n, o)) => {
                    module = Some(n.to_string());
                    offset = parse_hex(o);
                }
                None => module = Some(name.to_string()),
            }
        }
    }
    // «(+0x1234)» = الإزاحة داخل المكتبة
    if offset.is_none()
        && let Some(pos) = tail.find("(+0x")
    {
        let inner = tail.get(pos + 4..)?;
        let hex2: String = inner
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        offset = parse_hex(&hex2);
    }
    Some(Frame {
        index,
        addr,
        module,
        offset,
        is_crash_frame,
    })
}

/// يحلّل صفاً من جدول «Modules:».
///
/// **دلالات الأعمدة (مهمة):** العمود الأول نوع الصورة (`PE` أصلي / `PE-Wine`
/// مدمج)، والعمود الثالث **حالة الرموز** (`Deferred` = محمّلة بدون رموز)
/// وليس حالة التحميل.
fn parse_module_row(line: &str) -> Option<ModuleRow> {
    let mut s = line.trim();
    // صور PE المدمجة داخل ملفات unix تبدأ بـ «\-» (وليست مكتبات محمّلة)
    let mut embedded = false;
    if s.starts_with("\\-") {
        embedded = true;
        s = s.get(2..)?.trim_start();
        s = s.trim_start_matches('-').trim_start();
    }
    let mut parts = s.split_whitespace();
    let type_tok = parts.next()?;
    let (native, builtin) = match type_tok.to_ascii_lowercase().as_str() {
        "pe" => (true, false),
        "pe-wine" => (false, true),
        _ => return None,
    };
    let rest: Vec<&str> = parts.collect();
    let name = rest.last()?.to_string();
    // عمود الرموز: آخر رمز قبل الاسم — نتحقق أنه رموز فعلاً (وإلا هو عنوان)
    let mut dbg_info = String::new();
    let mut head: &[&str] = &rest[..rest.len() - 1];
    if let Some(last) = head.last()
        && is_dbg_status(last)
    {
        dbg_info = (*last).to_string();
        head = &head[..head.len() - 1];
    }
    // نطاق العناوين: «400000-982000» أو «400000-» + «982000»
    let addr_text: String = head.concat();
    let base = addr_text
        .split('-')
        .next()
        .and_then(|h| parse_hex(h.trim()))
        .unwrap_or(0);
    Some(ModuleRow {
        name,
        native,
        builtin,
        deferred: dbg_info.eq_ignore_ascii_case("deferred"),
        embedded,
        dbg_info,
        base,
    })
}

/// يحلّل تقرير winedbg كاملاً → [`CrashReport`].
///
/// يرجع `None` إذا النص **ما يشبه** تقرير انهيار أصلاً — عشان ما نعرض
/// على المستخدم «حكم» على نص عشوائي.
pub fn parse_report(text: &str) -> Option<CrashReport> {
    let mut exception_line: Option<String> = None;
    let mut fault_kind = String::new();
    let mut fault_addr: Option<u64> = None;
    let mut wow64_text = false;
    let mut frames: Vec<Frame> = Vec::new();
    let mut modules: Vec<ModuleRow> = Vec::new();
    let mut wine_build: Option<String> = None;
    let mut host_bits: u8 = 0;
    let mut guest_bits: u8 = 0;
    let mut section: Option<&'static str> = None;
    let mut recognized = false;

    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        // 0) تقرير حقيقي = فيه قسم winedbg واحد على الأقل.
        // (لولا هذا: برنامج يطبع "Unhandled exception:" في نصه ⇒ انهيار وهمي)
        if trimmed.starts_with("Register dump:")
            || trimmed.starts_with("Stack dump:")
            || trimmed.starts_with("Backtrace:")
            || trimmed.starts_with("Modules:")
        {
            recognized = true;
        }
        // 1) سطر الاستثناء (أولوية: قبل أي قسم)
        if let Some(rest) = strip_prefix_ci(trimmed, "Unhandled exception:") {
            exception_line = Some(trimmed.to_string());
            let (kind, addr, wow) = parse_exception(rest);
            fault_kind = kind;
            fault_addr = addr;
            wow64_text = wow;
            recognized = true;
            continue;
        }
        // 2) عنوان قسم
        if let Some(sec) = section_of(trimmed) {
            section = Some(sec);
            recognized = true;
            continue;
        }
        // 3) محتوى القسم
        match section {
            Some("backtrace") => {
                if let Some(f) = parse_frame(trimmed) {
                    frames.push(f);
                }
            }
            Some("modules") => {
                if let Some(m) = parse_module_row(trimmed) {
                    modules.push(m);
                }
            }
            Some("sysinfo") => {
                if let Some(v) = strip_prefix_ci(trimmed, "Wine build:") {
                    let v = v.trim();
                    if !v.is_empty() {
                        wine_build = Some(v.to_string());
                    }
                }
                if let Some(v) = strip_prefix_ci(trimmed, "Platform:") {
                    let (h, g) = parse_platform(v);
                    if h != 0 {
                        host_bits = h;
                    }
                    if g != 0 {
                        guest_bits = g;
                    }
                }
            }
            _ => {}
        }
    }

    if !recognized {
        return None;
    }
    // نص الاستثناء قال «wow64 32-bit code» ⇒ الضيف 32-بت
    if wow64_text && guest_bits == 0 {
        guest_bits = 32;
    }
    // WoW64 = ضيف 32-بت فوق Wine 64-بت
    let wow64 = wow64_text || (host_bits == 64 && guest_bits == 32);

    Some(CrashReport {
        exception_line,
        fault_kind,
        fault_addr,
        guest_bits,
        wow64,
        wine_build,
        frames,
        modules,
        raw_len: text.len(),
        overlay_hook: contains_ci(text, OVERLAY_MARKER),
    })
}

// ------------------------------------------------------------
//  التصنيف
// ------------------------------------------------------------

/// إطار الانهيار: الإطار المعلَّم بـ `=>`، وإلا أول إطار موجود.
fn crash_frame(report: &CrashReport) -> Option<&Frame> {
    report
        .frames
        .iter()
        .find(|f| f.is_crash_frame)
        .or_else(|| report.frames.first())
}

/// صف الوحدة المطابق — مع **تجاهل الصور المدمجة** داخل ملفات unix.
fn find_module<'a>(report: &'a CrashReport, name: &str) -> Option<&'a ModuleRow> {
    let target = normalize(name);
    if target.is_empty() {
        return None;
    }
    report
        .modules
        .iter()
        .find(|m| !m.embedded && normalize(&m.name) == target)
        .or_else(|| {
            report
                .modules
                .iter()
                .find(|m| !m.embedded && normalize(&m.name).starts_with(&target))
        })
}

/// اسم ملف اللعبة: صف ينتهي بـ `.exe`، وإلا أقل عنوان أساس بين الوحدات
/// غير المدمجة (Wine يرتّب جدول Modules تصاعدياً بالعنوان، فالـexe أولاً).
fn game_exe(report: &CrashReport) -> Option<String> {
    if let Some(m) = report
        .modules
        .iter()
        .find(|m| !m.embedded && m.name.to_ascii_lowercase().ends_with(".exe"))
    {
        return Some(m.name.clone());
    }
    let mut best: Option<&ModuleRow> = None;
    for m in report.modules.iter().filter(|m| !m.embedded && !m.builtin) {
        if m.base == 0 {
            continue;
        }
        if best.is_none_or(|b| m.base < b.base) {
            best = Some(m);
        }
    }
    if let Some(b) = best {
        return Some(b.name.clone());
    }
    // احتياط: أول وحدة غير مدمجة في الجدول
    report
        .modules
        .iter()
        .find(|m| !m.embedded && !m.builtin)
        .map(|m| m.name.clone())
}

/// اسم المكتبة الناقصة من «unimplemented function <dll>.<fn> called».
fn missing_dll_of(report: &CrashReport) -> Option<String> {
    let line = report.exception_line.as_deref()?;
    let lower = line.to_ascii_lowercase();
    let marker = "unimplemented function ";
    let pos = lower.find(marker)? + marker.len();
    let rest = line.get(pos..)?;
    let token: String = rest
        .chars()
        .take_while(|c| *c != ' ' && *c != '\t')
        .collect();
    if token.is_empty() {
        return None;
    }
    // «d3dx9_43.D3DXCreateTextureFromFileW» → «d3dx9_43»
    let raw = token.split('.').next().unwrap_or(token.as_str());
    let name = raw.trim().to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    Some(if name.ends_with(".dll") {
        name
    } else {
        format!("{name}.dll")
    })
}

/// هل المكتبة من مكتبات ويندوز الأساسية؟ (اللي ما نعتبرها «ناقصة»)
fn is_core_dll(dll: &str) -> bool {
    let base = normalize(dll);
    CORE_DLLS.contains(&base.as_str())
}

/// سطر دليل واحد: «d3dx9_43 = PE (native)».
fn module_kind_line(m: &ModuleRow) -> String {
    let kind = if m.builtin {
        "PE-Wine (builtin)"
    } else if m.native {
        "PE (native)"
    } else {
        "PE (unknown)"
    };
    format!("{} = {}", m.name, kind)
}

/// أدلة مشتركة لكل الأحكام.
fn base_evidence(
    report: &CrashReport,
    frame: Option<&Frame>,
    row: Option<&ModuleRow>,
) -> Vec<String> {
    let mut ev = Vec::new();
    if let Some(f) = frame {
        match &f.module {
            Some(m) => ev.push(format!("{m} ظهر في إطار الانهيار {}", f.index)),
            None => ev.push(format!("إطار الانهيار {} بلا اسم مكتبة", f.index)),
        }
    }
    if let Some(r) = row {
        ev.push(module_kind_line(r));
        if r.deferred {
            ev.push(format!(
                "{} = Deferred: محمّلة في الذاكرة بدون رموز (ليست على مسار الانهيار)",
                r.name
            ));
        }
    }
    if let Some(b) = &report.wine_build {
        ev.push(format!("نسخة Wine: {b}"));
    }
    if report.guest_bits != 0 {
        if report.wow64 {
            ev.push("الضيف 32-بت فوق Wine 64-بت (WoW64)".to_string());
        } else {
            ev.push(format!("معمارية الضيف: {}-بت", report.guest_bits));
        }
    }
    ev
}

/// أدلة الـOverlay.
fn overlay_evidence(report: &CrashReport) -> Vec<String> {
    let mut ev = vec![format!("التقرير فيه: \"{OVERLAY_MARKER}\"")];
    if let Some(f) = crash_frame(report)
        && let Some(m) = &f.module
    {
        ev.push(format!("{m} ظهر في إطار الانهيار {}", f.index));
    }
    if let Some(b) = &report.wine_build {
        ev.push(format!("نسخة Wine: {b}"));
    }
    ev
}

/// أدلة «ما عرفنا»: نضيف ما وجدناه في المسار حتى لو الحكم مجهول —
/// (مثلاً: d3dx9_43 مدمج داخل المسار ⇒ علاقة محتملة حتى لو سبب مباشر غير واضح).
fn unknown_evidence(report: &CrashReport) -> Vec<String> {
    let mut ev = base_evidence(report, crash_frame(report), None);
    // d3dx/d3dcompiler ظهرت في المسار — أفيد المستخدم بها
    for f in &report.frames {
        let Some(m) = &f.module else { continue };
        let n = normalize(m);
        if !(n.starts_with("d3dx") || n.starts_with("d3dcompiler")) {
            continue;
        }
        if let Some(row) = find_module(report, &n) {
            ev.push(format!("{n} في المسار (إطار {})", f.index));
            ev.push(module_kind_line(row));
        }
    }
    ev
}

/// يصنّف الانهيار ويحوّله إلى خطوات عملية.
///
/// ترتيب القواعد مهم: من العام (الـOverlay) إلى الخاص (المكتبة الناقصة)،
/// ثم نقرأ **إطار الانهيار** (`=>N`) لأن بقية المسار مجرد سياق.
pub fn classify(report: &CrashReport) -> Verdict {
    // 1) الـOverlay — السبب الجذري، يسبق كل شيء
    if report.overlay_hook {
        return Verdict {
            class: FaultClass::OverlayHook,
            crashed_module: None,
            headline_ar: "طبقة ستيم (Overlay) هي اللي معترضة على Direct3D".to_string(),
            next_ar: "عطّل Steam Overlay من إعدادات ستيم (إدارة > عام > Steam Overlay) \
                      لهذه اللعبة، ثم أعد تشغيلها."
                .to_string(),
            action: "disable-overlay".to_string(),
            evidence: overlay_evidence(report),
        };
    }

    // 2) DLL ناقصة (مو من المكتبات الأساسية)
    if let Some(dll) = missing_dll_of(report)
        && !is_core_dll(&dll)
    {
        let mut ev = vec![format!(
            "الاستثناء: {dll} — Wine طلب دالة غير موجودة (unimplemented function)"
        )];
        if let Some(row) = find_module(report, &dll) {
            ev.push(module_kind_line(row));
        } else {
            ev.push(format!("{dll} غير موجودة في جدول المكتبات المحمّلة"));
        }
        return Verdict {
            class: FaultClass::MissingDll,
            crashed_module: Some(dll.clone()),
            headline_ar: format!("مكتبة ويندوز ناقصة: {dll}"),
            next_ar: format!(
                "مكتبة {dll} ناقصة من مجلد ويندوز — نزّلها نسخة أصلية \
                     (حزمة DirectX أو الحزمة الرسمية للّعبة) وضعها بجانب ملف اللعبة."
            ),
            action: format!("missing-dll:{dll}"),
            evidence: ev,
        };
    }

    // 3) نقرأ إطار الانهيار
    let frame = crash_frame(report);
    let mod_norm = frame
        .and_then(|f| f.module.as_deref())
        .map(normalize)
        .unwrap_or_default();
    let crashed = frame.and_then(|f| f.module.clone());

    if !mod_norm.is_empty()
        && let Some(v) = classify_graphics(report, &mod_norm, frame, crashed.clone())
    {
        return v;
    }

    // 3.5) إطار الانهيار مو رسوم (مثل `ucrtbase`) ⇒ نمسح **كامل** الـbacktrace.
    //
    // مثالنا الحقيقي: `ucrtbase ← wined3d ← d3dcompiler_43 ← d3dx9_43 ← اللعبة`.
    // إطار الانهيار هنا مكتبة C نظامية، فبدون المسح كان التصنيف "غير معروف"、
    // والمهمّة (تثبيت D3DX9 الأصلي) ما بتوصلنا. نمسح السلسلة ⇒ نعرف أي
    // طبقة D3DX/D3DCompiler كانت شغّالة (مدمجة Wine أم أصلية مايكروسوفت).
    if is_system_frame(report) {
        // الأولوية لـD3DX/المترجم (المشكلة الحقيقية في حالتنا)، ثم طبقة Direct3D
        let candidates = [first_d3dx_module(report), first_device_module(report)];
        for candidate in candidates.into_iter().flatten() {
            let (name, row, frame) = candidate;
            if let Some(mut v) = classify_graphics(report, &name, frame, Some(name.clone())) {
                v.evidence
                    .push(format!("وحدة الرسوم في مسار الانهيار: {name}"));
                if let Some(r) = row {
                    v.evidence.push(module_kind_line(r));
                }
                // نشهد على كل وحدات D3DX/المترجم في المسار (هي اللي تحدد الإجراء)
                for f in report.frames.iter() {
                    let Some(m) = f.module.as_deref() else {
                        continue;
                    };
                    let n = normalize(m);
                    if (n.starts_with("d3dx") || n.starts_with("d3dcompiler"))
                        && let Some(r) = find_module(report, &n)
                    {
                        v.evidence.push(module_kind_line(r));
                    }
                }
                return v;
            }
        }
    }

    // 4) انهيار WoW64 (الضيف 32-بت ينهار في طبقة التحويل على Rosetta)
    if (report.wow64 || report.guest_bits == 32)
        && matches!(mod_norm.as_str(), "wow64cpu" | "ntdll")
    {
        let row = find_module(report, &mod_norm);
        let ev = base_evidence(report, frame, row);
        return Verdict {
            class: FaultClass::Wow64Thunk,
            crashed_module: crashed,
            headline_ar: "الانهيار في طبقة تحويل WoW64 (Rosetta)".to_string(),
            next_ar:
                "الانهيار في تحويل WoW64 على Rosetta — استخدم renderer=vulkan (wined3d عبر Vulkan)"
                    .to_string(),
            action: "renderer-vulkan".to_string(),
            evidence: ev,
        };
    }

    // 5) داخل اللعبة نفسها
    if !mod_norm.is_empty()
        && let Some(exe) = game_exe(report)
        && normalize(&exe) == mod_norm
    {
        let row = find_module(report, &mod_norm);
        let ev = base_evidence(report, frame, row);
        return Verdict {
            class: FaultClass::GameItself,
            crashed_module: crashed,
            headline_ar: "الانهيار داخل ملف اللعبة نفسه".to_string(),
            next_ar: "الانهيار داخل اللعبة نفسها — تأكّد من ملفاتها ومجلد العمل (working directory)"
                .to_string(),
            action: "check-game-files".to_string(),
            evidence: ev,
        };
    }

    // 6) ما عرفنا
    Verdict {
        class: FaultClass::Unknown,
        crashed_module: crashed,
        headline_ar: "ما قدرنا نحدد سبب الانهيار بدقة".to_string(),
        next_ar: "نحتاج سجل WINEDEBUG مركّز — شغّل اللعبة مع WINEDEBUG=relay,seh,loaddll \
                  وأرسلنا المخرجات كاملة."
            .to_string(),
        action: "collect-winedbg".to_string(),
        evidence: unknown_evidence(report),
    }
}

/// أصنّف وحدة رسوم (D3DX / d3dcompiler / Direct3D / Vulkan) — نفس المنطق
/// سواء جاءت من إطار الانهيار أو من مسح الـbacktrace.
fn classify_graphics(
    report: &CrashReport,
    name: &str,
    frame: Option<&Frame>,
    crashed: Option<String>,
) -> Option<Verdict> {
    // D3DX / d3dcompiler
    if name.starts_with("d3dx9")
        || name.starts_with("d3dx10")
        || name.starts_with("d3dx11")
        || name.starts_with("d3dcompiler")
    {
        let row = find_module(report, name);
        let builtin = row.is_some_and(|r| r.builtin);
        let ev = base_evidence(report, frame, row);
        if builtin {
            return Some(Verdict {
                class: FaultClass::D3dxBuiltin,
                crashed_module: crashed,
                headline_ar: format!("الانهيار داخل {name} — نسخة Wine المدمجة ناقصة"),
                next_ar: "ثبّت D3DX9 وd3dcompiler الأصليين من Microsoft's DirectX June 2010 redist"
                    .to_string(),
                action: "install-d3dx9-native".to_string(),
                evidence: ev,
            });
        }
        return Some(Verdict {
            class: FaultClass::D3dxNative,
            crashed_module: crashed,
            headline_ar: format!("الانهيار داخل {name} — نسخة مايكروسوفت الأصلية"),
            next_ar: "نسخة مايكروسوفت هي اللي تنهار — جرّب نسخة Wine المدمجة أو مترجم أحدث (d3dcompiler_47)"
                .to_string(),
            action: "try-d3dx9-builtin".to_string(),
            evidence: ev,
        });
    }

    // طبقة Direct3D نفسها
    if matches!(name, "d3d9" | "d3d10core" | "d3d11" | "dxgi" | "wined3d") {
        let row = find_module(report, name);
        let builtin = row.is_some_and(|r| r.builtin);
        let ev = base_evidence(report, frame, row);
        if builtin {
            return Some(Verdict {
                class: FaultClass::D3d9Builtin,
                crashed_module: crashed,
                headline_ar: "اللعبة تسير على محرك WineD3D المدمج (DXVK مو مُفعّل)".to_string(),
                next_ar: "اللعبة تسير على محرك WineD3D المدمج — فعّل DXVK (native)".to_string(),
                action: "enable-dxvk".to_string(),
                evidence: ev,
            });
        }
        return Some(Verdict {
            class: FaultClass::D3d9Native,
            crashed_module: crashed,
            headline_ar: "الانهيار داخل DXVK".to_string(),
            next_ar: "DXVK نشط لكن الاشتراك داخله ينهار — افحص سجل DXVK (DXVK_LOG_PATH)".to_string(),
            action: "check-dxvk-log".to_string(),
            evidence: ev,
        });
    }

    // Vulkan
    if matches!(name, "winevulkan" | "vulkan-1" | "moltenvk") {
        let row = find_module(report, name);
        let ev = base_evidence(report, frame, row);
        return Some(Verdict {
            class: FaultClass::Vulkan,
            crashed_module: crashed,
            headline_ar: "الانهيار داخل طبقة Vulkan".to_string(),
            next_ar: "انهيار داخل طبقة Vulkan — تحقّق من MoltenVK (إصدار 1.2.10 مع DXVK 1.10.3) ومن 32-بت Vulkan"
                .to_string(),
            action: "check-moltenvk".to_string(),
            evidence: ev,
        });
    }
    None
}

/// أول وحدة **D3DX/مترجم** في الـbacktrace (من الأعلى للأسفل).
///
/// الأولوية لها: D3DX فوق Direct3D — لأن `wined3d` يظهر في المسار كتبعٍ
/// لـ`d3dcompiler` المدمج، وم Joel הDiagnose الصحيح هو D3DX لا Direct3D.
fn first_d3dx_module(report: &CrashReport) -> Option<(String, Option<&ModuleRow>, Option<&Frame>)> {
    for frame in report.frames.iter() {
        let Some(raw) = frame.module.as_deref() else {
            continue;
        };
        let name = normalize(raw);
        if name.starts_with("d3dx9")
            || name.starts_with("d3dx10")
            || name.starts_with("d3dx11")
            || name.starts_with("d3dcompiler")
        {
            let row = find_module(report, &name);
            return Some((name, row, Some(frame)));
        }
    }
    None
}

/// أول وحدة **Direct3D/Vulkan** في الـbacktrace
fn first_device_module(
    report: &CrashReport,
) -> Option<(String, Option<&ModuleRow>, Option<&Frame>)> {
    for frame in report.frames.iter() {
        let Some(raw) = frame.module.as_deref() else {
            continue;
        };
        let name = normalize(raw);
        if matches!(
            name.as_str(),
            "d3d9" | "d3d10core" | "d3d11" | "dxgi" | "wined3d" | "winevulkan" | "vulkan-1"
        ) {
            let row = find_module(report, &name);
            return Some((name, row, Some(frame)));
        }
    }
    None
}

/// هل إطار الانهيار داخل مكتبة نظام (مش داخل اللعبة نفسها)؟
///
/// لو كان الانهيار في كود اللعبة ⇒ ما نمسح الـbacktrace (المشكلة من اللعبة).
fn is_system_frame(report: &CrashReport) -> bool {
    let Some(frame) = crash_frame(report) else {
        return true; // ما عرفنا الإطار ⇒ نفترض نظام
    };
    let Some(module) = frame.module.as_deref() else {
        return true;
    };
    let name = normalize(module);
    if name.is_empty() {
        return true;
    }
    // مكتبة اللعبة نفسها (exe) ⇒ مو إطار نظام
    if let Some(exe) = game_exe(report) {
        if normalize(&exe) == name {
            return false;
        }
    }
    matches!(
        name.as_str(),
        "ucrtbase"
            | "msvcrt"
            | "ntdll"
            | "kernel32"
            | "kernelbase"
            | "user32"
            | "gdi32"
            | "msvcp140"
            | "vcruntime140"
            | "vcruntime140_1"
            | "ucrtbased"
            | "api-ms-win-crt"
    ) || name.starts_with("msvcp")
        || name.starts_with("vcruntime")
        || name.starts_with("api-ms-win-crt")
}

/// يحلّل ويصنّف في خطوة واحدة (الوظيفة اللي يستدعيها التطبيق).
pub fn triage(text: &str) -> Option<Verdict> {
    let report = parse_report(text)?;
    Some(classify(&report))
}

/// رمز نصي قصير للصنف (يستهلكه الواجهة/السجل).
pub fn class_as_str(c: FaultClass) -> &'static str {
    match c {
        FaultClass::D3dxBuiltin => "d3dx-builtin",
        FaultClass::D3dxNative => "d3dx-native",
        FaultClass::D3d9Builtin => "d3d9-builtin",
        FaultClass::D3d9Native => "d3d9-native",
        FaultClass::Vulkan => "vulkan",
        FaultClass::Wow64Thunk => "wow64-thunk",
        FaultClass::MissingDll => "missing-dll",
        FaultClass::OverlayHook => "overlay-hook",
        FaultClass::GameItself => "game-itself",
        FaultClass::Unknown => "unknown",
    }
}

// ============================================================
//  الاختبارات
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// تقرير حقيقي من المستخدم: d3dx9_43/d3dcompiler_43 **الأصلية** من مايكروسوفت.
    const REAL_1: &str = r#"Unhandled exception: page fault on read access to 0x00000000 in wow64 32-bit code (0x781496a9).
Register dump:
 CS:0107 SS:0023 DS:0023 ES:0023 FS:010f GS:0023
 EIP:781496a9 ESP:0026e32c EBP:0026e338 EFLAGS:00000247(   - --  I  Z- -P-C)
 EAX:00000000 EBX:00000109 ECX:77f62008 EDX:00000018
 ESI:77f62060 EDI:00000000
Stack dump:
0x0026e32c:  0026ea90 0026e7c8 00000109 0026e73c
0x0026e33c:  780385e4 00000000 00000001 00000000
Backtrace:
=>0 0x781496a9 in d3dcompiler_43 (+0x1e96a9) (0x0026e338)
  1 0x780385e4 in d3dcompiler_43 (+0xd85e4) (0x0026e73c)
  2 0x7803750e in d3dcompiler_43 (+0xd750e) (0x0026e768)
  3 0x77fc6e6f in d3dcompiler_43 (+0x66e6f) (0x0026f4a4)
  4 0x7a2b6186 in d3dx9_43 (+0xe6186) (0x0026f750)
  5 0x004862e3 in limbo_demo (+0x862e3) (0x05728489)
  6 0x63616c70 (0x6552202f)
Modules:
Module  Address                 Debug info      Name (60 modules)
PE        400000-  982000       --none--        limbo_demo
PE        3b400000-3b41f000       Deferred        steam_api
PE        77f60000-78167000       --none--        d3dcompiler_43
PE-Wine   78390000-78398000       Deferred        nsi
PE-Wine   7a050000-7a059000       Deferred        winemac
PE        7a1d0000-7a3cf000       --none--        d3dx9_43
PE-Wine   7a3e0000-7a416000       Deferred        winevulkan
PE-Wine   7a430000-7a43b000       Deferred        vulkan-1
PE        7a450000-7a779000       Deferred        d3d9
PE-Wine   7b6f0000-7b7c8000       Deferred        ucrtbase
PE-Wine   7bdf0000-7be91000       Deferred        ntdll
System information:
    Wine build: wine-11.18
    Platform: x86_64 (guest: i386)
    Version: Windows 10
    Host system: Darwin
    Host version: 25.6.0
"#;

    /// تقرير حقيقي أقدم: d3dx9_43/d3dcompiler_43 **المدمجة في Wine** + wined3d.
    const REAL_2: &str = r#"Unhandled exception: page fault on read access to 0x00000000 in wow64 32-bit code (0x7b74c870).
Backtrace:
=>0 0x7b74c870 in ucrtbase (+0x5c870) (0x0026f584)
  1 0x7a0e9b88 in wined3d (+0xc9b88) (0x0026f668)
  2 0x7a30ac1e in d3dcompiler_43 (+0xac1e) (0x0026f6c4)
  3 0x7a30ad21 in d3dcompiler_43 (+0xad21) (0x0026f710)
  4 0x7a38dc04 in d3dx9_43 (+0x4dc04) (0x0026f750)
  5 0x004862e3 in limbo_demo (+0x862e3) (0x05748489)
Modules:
Module  Address                 Debug info      Name (63 modules)
PE        400000-  982000       --none--        limbo_demo
PE-Wine   7a020000-7a2ef000       --none--        wined3d
PE-Wine   7a300000-7a326000       --none--        d3dcompiler_43
PE-Wine   7a340000-7a3cc000       --none--        d3dx9_43
PE-Wine   7a3e0000-7a416000       Deferred        winevulkan
PE-Wine   7a430000-7a43b000       Deferred        vulkan-1
PE        7a450000-7a779000       Deferred        d3d9
PE-Wine   7b6f0000-7b7c8000       Deferred        ucrtbase
System information:
    Wine build: wine-11.18
    Platform: x86_64 (guest: i386)
"#;

    /// تقرير اصطناعي: يُبنى بأجزاء صغيرة.
    fn synth(exception: &str, backtrace: &str, modules: &str) -> String {
        let mut s = String::new();
        s.push_str("Unhandled exception: ");
        s.push_str(exception);
        s.push('\n');
        s.push_str("Backtrace:\n");
        s.push_str(backtrace);
        s.push_str("Modules:\n");
        s.push_str("Module  Address                 Debug info      Name (3 modules)\n");
        s.push_str(modules);
        s.push_str("System information:\n");
        s.push_str("    Wine build: wine-11.18\n");
        s.push_str("    Platform: x86_64 (guest: i386)\n");
        s
    }

    #[test]
    fn parses_real_report_one() {
        let r = parse_report(REAL_1).expect("تقرير صالح");
        // سطر الاستثناء
        assert!(
            r.exception_line
                .as_deref()
                .unwrap_or("")
                .starts_with("Unhandled exception:")
        );
        assert_eq!(r.fault_kind, "page fault on read access to 0x00000000");
        assert_eq!(r.fault_addr, Some(0));
        // المقاس: 32-بت فوق Wine 64-بت
        assert_eq!(r.guest_bits, 32);
        assert!(r.wow64);
        assert_eq!(r.wine_build.as_deref(), Some("wine-11.18"));
        assert_eq!(r.raw_len, REAL_1.len());
        // إطار الانهيار
        let cf = r
            .frames
            .iter()
            .find(|f| f.is_crash_frame)
            .expect("إطار الانهيار");
        assert_eq!(cf.index, 0);
        assert_eq!(cf.module.as_deref(), Some("d3dcompiler_43"));
        assert_eq!(cf.offset, Some(0x1e96a9));
        assert_eq!(r.frames.len(), 7);
        // إطار بلا اسم مكتبة
        let last = r.frames.last().expect("آخر إطار");
        assert_eq!(last.module, None);
        // أنواع المكتبات
        let d3dc = find_module(&r, "d3dcompiler_43").expect("صف d3dcompiler_43");
        assert!(d3dc.native && !d3dc.builtin);
        assert!(!d3dc.deferred);
        let d3dx = find_module(&r, "d3dx9_43").expect("صف d3dx9_43");
        assert!(d3dx.native && !d3dx.builtin);
        let ucrt = find_module(&r, "ucrtbase").expect("صف ucrtbase");
        assert!(ucrt.builtin && !ucrt.native);
        assert!(ucrt.deferred);
        let d3d9 = find_module(&r, "d3d9").expect("صف d3d9");
        assert!(d3d9.native);
        let vulkan = find_module(&r, "winevulkan").expect("صف winevulkan");
        assert!(vulkan.builtin);
    }

    #[test]
    fn classifies_real_report_one_as_native_d3dx() {
        let v = triage(REAL_1).expect("حكم");
        assert_eq!(v.class, FaultClass::D3dxNative);
        assert_eq!(v.action, "try-d3dx9-builtin");
        assert_eq!(v.crashed_module.as_deref(), Some("d3dcompiler_43"));
        assert!(!v.evidence.is_empty());
        assert!(v.next_ar.contains("d3dcompiler_47"));
    }

    #[test]
    fn parses_real_report_two() {
        let r = parse_report(REAL_2).expect("تقرير صالح");
        assert_eq!(r.guest_bits, 32);
        assert!(r.wow64);
        assert_eq!(r.wine_build.as_deref(), Some("wine-11.18"));
        let cf = r
            .frames
            .iter()
            .find(|f| f.is_crash_frame)
            .expect("إطار الانهيار");
        assert_eq!(cf.module.as_deref(), Some("ucrtbase"));
        assert_eq!(r.frames.len(), 6);
        // d3dx/d3dcompiler مدمجة (PE-Wine) في هذه الجلسة
        let d3dx = find_module(&r, "d3dx9_43").expect("صف d3dx9_43");
        assert!(d3dx.builtin && !d3dx.native);
        let d3dc = find_module(&r, "d3dcompiler_43").expect("صف d3dcompiler_43");
        assert!(d3dc.builtin);
        let wined3d = find_module(&r, "wined3d").expect("صف wined3d");
        assert!(wined3d.builtin);
        // d3d9 أصلي (DXVK) رغم أنDeferred — محمّلة!
        let d3d9 = find_module(&r, "d3d9").expect("صف d3d9");
        assert!(d3d9.native);
        assert!(d3d9.deferred);
    }

    #[test]
    fn classifies_real_report_two() {
        let v = triage(REAL_2).expect("حكم");
        // إطار الانهيار = ucrtbase ⇒ نصنّف بمسح الـbacktrace كامل:
        // d3dcompiler_43/d3dx9_43 **مدمجة** (PE-Wine) ⇒ الحل: D3DX9 الأصلي
        assert_eq!(v.class, FaultClass::D3dxBuiltin);
        assert_eq!(v.action, "install-d3dx9-native");
        // الأدلة تشير إلى d3dx المدمجة في المسار
        assert!(v.evidence.iter().any(|e| e.contains("d3dx9_43 = PE-Wine")));
    }

    #[test]
    fn builtin_d3dx_is_classified_as_builtin() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x7a38dc04).",
            "=>0 0x7a38dc04 in d3dx9_43 (+0x4dc04) (0x0026f750)\n  1 0x004862e3 in limbo_demo (+0x862e3) (0x05748489)\n",
            "PE-Wine   7a340000-7a3cc000       --none--        d3dx9_43\nPE        400000-  982000       --none--        limbo_demo\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::D3dxBuiltin);
        assert_eq!(v.action, "install-d3dx9-native");
        assert_eq!(v.crashed_module.as_deref(), Some("d3dx9_43"));
        assert!(v.next_ar.contains("DirectX June 2010"));
        assert!(v.evidence.iter().any(|e| e.contains("d3dx9_43 = PE-Wine")));
    }

    #[test]
    fn native_d3dx_is_classified_as_native() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x781496a9).",
            "=>0 0x781496a9 in d3dcompiler_43 (+0x1e96a9) (0x0026e338)\n",
            "PE        77f60000-78167000       --none--        d3dcompiler_43\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::D3dxNative);
        assert_eq!(v.action, "try-d3dx9-builtin");
        assert!(
            v.evidence
                .iter()
                .any(|e| e.contains("d3dcompiler_43 = PE (native)"))
        );
    }

    #[test]
    fn builtin_d3d9_means_wined3d_is_active() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x7a0e9b88).",
            "=>0 0x7a0e9b88 in d3d9 (+0xc9b88) (0x0026f668)\n",
            "PE-Wine   7a020000-7a2ef000       --none--        wined3d\nPE-Wine   7a020000-7a2ef000       --none--        d3d9\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::D3d9Builtin);
        assert_eq!(v.action, "enable-dxvk");
        assert!(v.next_ar.contains("DXVK"));
    }

    #[test]
    fn native_d3d9_means_dxvk_is_active() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x7a450000).",
            "=>0 0x7a450000 in d3d9 (+0x10000) (0x0026f668)\n",
            "PE        7a450000-7a779000       Deferred        d3d9\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::D3d9Native);
        assert_eq!(v.action, "check-dxvk-log");
    }

    #[test]
    fn overlay_hook_wins_over_everything() {
        let mut text = REAL_1.to_string();
        text.push_str("\nCreateDevice hook called\n");
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::OverlayHook);
        assert_eq!(v.action, "disable-overlay");
        assert!(v.next_ar.contains("Overlay"));
        assert!(
            v.evidence
                .iter()
                .any(|e| e.contains("CreateDevice hook called"))
        );
    }

    #[test]
    fn missing_dll_is_detected() {
        let text = synth(
            "unimplemented function d3dx9_43.D3DXCreateTextureFromFileW called",
            "=>0 0x004862e3 in limbo_demo (+0x862e3) (0x05728489)\n",
            "PE        400000-  982000       --none--        limbo_demo\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::MissingDll);
        assert_eq!(v.action, "missing-dll:d3dx9_43.dll");
        assert_eq!(v.crashed_module.as_deref(), Some("d3dx9_43.dll"));
        assert!(
            v.evidence
                .iter()
                .any(|e| e.contains("unimplemented function"))
        );
    }

    #[test]
    fn unimplemented_core_dll_is_not_a_missing_dll() {
        let text = synth(
            "unimplemented function kernel32.Sleep called",
            "=>0 0x004862e3 in limbo_demo (+0x862e3) (0x05728489)\n",
            "PE        400000-  982000       --none--        limbo_demo\n",
        );
        let v = triage(&text).expect("حكم");
        assert_ne!(v.class, FaultClass::MissingDll);
    }

    #[test]
    fn wow64_ntdll_crash_is_a_thunk_problem() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x7bdf0100).",
            "=>0 0x7bdf0100 in ntdll (+0x10100) (0x0026f584)\n",
            "PE-Wine   7bdf0000-7be91000       Deferred        ntdll\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::Wow64Thunk);
        assert_eq!(v.action, "renderer-vulkan");
        assert!(v.next_ar.contains("Rosetta"));
    }

    #[test]
    fn report_without_modules_section_is_unknown() {
        let text = "Unhandled exception: page fault on read access to 0x00000000 in wow64 32-bit code (0x1234).\n\
                    Backtrace:\n\
                    =>0 0x004862e3 in some_module (+0x862e3) (0x05728489)\n";
        let r = parse_report(text).expect("تحليل");
        assert!(r.modules.is_empty());
        assert_eq!(r.guest_bits, 32);
        let v = triage(text).expect("حكم");
        assert_eq!(v.class, FaultClass::Unknown);
        assert_eq!(v.action, "collect-winedbg");
    }

    #[test]
    fn garbage_text_is_rejected_without_panic() {
        assert!(parse_report("").is_none());
        assert!(parse_report("hello world\njust some text\n").is_none());
        assert!(parse_report("!!! ??? ***\n").is_none());
        assert!(triage("not a report at all").is_none());
    }

    #[test]
    fn deferred_means_loaded_not_absent() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x7a450000).",
            "=>0 0x7a450000 in d3d9 (+0x10000) (0x0026f668)\n",
            "PE        7a450000-7a779000       Deferred        d3d9\n",
        );
        let r = parse_report(&text).expect("تحليل");
        let d3d9 = find_module(&r, "d3d9").expect("صف d3d9");
        assert!(d3d9.deferred);
        // رغم Deferred هي محمّلة فعلاً — ومع ذلك نعدّها DXVK نشط
        assert_eq!(classify(&r).class, FaultClass::D3d9Native);
    }

    #[test]
    fn embedded_pe_rows_are_excluded_from_attribution() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x7a38dc04).",
            "=>0 0x7a38dc04 in d3dx9_43 (+0x4dc04) (0x0026f750)\n",
            "\\-PE      7a340000-7a350000       --none--        d3dx9_43.dll\n\
             PE        7a350000-7a3cc000       --none--        d3dx9_43\n",
        );
        let r = parse_report(&text).expect("تحليل");
        let embedded = r.modules.iter().find(|m| m.embedded).expect("صف مدمج");
        assert_eq!(embedded.name, "d3dx9_43.dll");
        // النسبة تستخدم الصف غير المدمج (الأصلي) لا الصورة المدمجة
        assert_eq!(classify(&r).class, FaultClass::D3dxNative);
    }

    #[test]
    fn crash_inside_the_game_exe_is_game_itself() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x004862e3).",
            "=>0 0x004862e3 in limbo_demo (+0x862e3) (0x05728489)\n",
            "PE        400000-  982000       --none--        limbo_demo\n\
             PE-Wine   7b6f0000-7b7c8000       Deferred        ucrtbase\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::GameItself);
        assert_eq!(v.action, "check-game-files");
        assert!(v.next_ar.contains("working directory"));
    }

    #[test]
    fn vulkan_crash_points_to_moltenvk() {
        let text = synth(
            "page fault on read access to 0x00000000 in wow64 32-bit code (0x7a3e0010).",
            "=>0 0x7a3e0010 in winevulkan (+0x10) (0x0026f584)\n",
            "PE-Wine   7a3e0000-7a416000       Deferred        winevulkan\n",
        );
        let v = triage(&text).expect("حكم");
        assert_eq!(v.class, FaultClass::Vulkan);
        assert_eq!(v.action, "check-moltenvk");
        assert!(
            v.evidence
                .iter()
                .any(|e| e.contains("winevulkan = PE-Wine"))
        );
    }

    #[test]
    fn class_tokens_are_stable() {
        assert_eq!(class_as_str(FaultClass::D3dxBuiltin), "d3dx-builtin");
        assert_eq!(class_as_str(FaultClass::D3dxNative), "d3dx-native");
        assert_eq!(class_as_str(FaultClass::D3d9Builtin), "d3d9-builtin");
        assert_eq!(class_as_str(FaultClass::D3d9Native), "d3d9-native");
        assert_eq!(class_as_str(FaultClass::Vulkan), "vulkan");
        assert_eq!(class_as_str(FaultClass::Wow64Thunk), "wow64-thunk");
        assert_eq!(class_as_str(FaultClass::MissingDll), "missing-dll");
        assert_eq!(class_as_str(FaultClass::OverlayHook), "overlay-hook");
        assert_eq!(class_as_str(FaultClass::GameItself), "game-itself");
        assert_eq!(class_as_str(FaultClass::Unknown), "unknown");
    }
}
