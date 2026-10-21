//! Jisr — درع مترجم الـHLSL (`d3dcompiler_43`)
//!
//! **لماذا:** ألعاب DirectX 9 القديمة (LIMBO مثلًا) تنهار داخل مترجم
//! مايكروسوفت `d3dcompiler_43` نفسه عندما تمرّر `pTarget=NULL` أو
//! `pEntrypoint=NULL` (مُثبت معملياً: +0x1E96A9 و+0x1C5C66).
//!
//! **هذا الغلاف** يجلس بين اللعبة والمترجم الحقيقي:
//! 1. **حارس**: لو المدخل باطل ⇒ نكمّله بقيمة مشتقّة من المصدر بدل قفزة
//!    الذاكرة، ولو المصدر نفسه فارغ ⇒ `E_FAIL` نظيف.
//! 2. **مِجهر**: يسجّل كل استدعاء + النتيجة (`hr`) + نص الخطأ + طول الكود.
//! 3. **بصمة**: يخبر التطبيق أنه غلافنا (لا نسخة مايكروسوفت).
//!
//! المرجع الحقيقي يُحمَّل من `d3dcompiler_43_ms.dll` **بجانب هذا الملف**
//! (وليس بجانب اللعبة — هذا كان العيب القاتل في 0.1.30: كنا نبحث في مجلد
//! اللعبة فما نلقى المترجم ⇒ كل ترجمة ترجع E_FAIL ⇒ اللعبة تنهار وهي
//! تظن أن الشيدر انبنى).
//!
//! كل الاستدعاءات الأخرى (17 تصديراً) تمرّ للمترجم الحقيقي كما هي.

#![allow(non_snake_case)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

// ============================================================
//  تعريفات Win32 المطلوبة
// ============================================================

type HRESULT = i32;
type SIZE_T = usize;
type UINT = u32;
type DWORD = u32;
type LPCVOID = *const c_void;
type LPVOID = *mut c_void;
type LPCSTR = *const c_char;
type LPCWSTR = *const u16;
type REFIID = *const c_void;

const S_OK: HRESULT = 0;
const E_FAIL: HRESULT = -2147467259; // 0x80004005
const E_NOTIMPL: HRESULT = -2147467263; // 0x80004001

const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: DWORD = 0x0000_0002;
const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: DWORD = 0x0000_0004;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn GetModuleFileNameA(module: *mut c_void, buf: *mut c_char, size: DWORD) -> DWORD;
    fn GetModuleHandleExA(flags: DWORD, name: *const c_char, module: *mut *mut c_void) -> i32;
    fn GetSystemDirectoryA(buf: *mut c_char, size: UINT) -> UINT;
    fn GetLastError() -> DWORD;
    fn GetCurrentProcessId() -> DWORD;
    fn GetTickCount() -> DWORD;
}

/// عنوان ثابت داخل هذا الـDLL — نستخدمه لمعرفة مسار الوحدة نفسها
/// (`GetModuleHandleExA(FROM_ADDRESS)`) بدل `GetModuleFileNameA(NULL)` الذي
/// يرجع مسار **اللعبة** لا مسار الغلاف.
static MODULE_ANCHOR: u8 = 0;

// ============================================================
//  سجل الاستدعاءات
// ============================================================

/// المسار الافتراضي: جذر القارورة (`C:\jisr-d3dx-shim.log`) — نفس المسار
/// الذي يقرأه التطبيق. يُمكن تجاوزه بـ`JISR_D3DX_LOG`.
const DEFAULT_LOG: &str = r"C:\jisr-d3dx-shim.log";

fn log_path() -> PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        if let Ok(p) = std::env::var("JISR_D3DX_LOG") {
            let p = p.trim();
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
        PathBuf::from(DEFAULT_LOG)
    })
    .clone()
}

/// نكتب سطراً واحداً (best-effort — ما نبيّن فشل التسجيل للعبة)
fn log_line(line: &str) {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let pid = unsafe { GetCurrentProcessId() };
    let tick = unsafe { GetTickCount() };
    let path = log_path();
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "[jisr] #{n} pid={pid} t={tick} {line}");
    }
}

/// أول ٦٤ بايت من المصدر (hex + نص مقروء) — تكفي للتعرّف على الشيدر
fn source_head(p: *const c_void, size: usize) -> String {
    if p.is_null() || size == 0 {
        return String::from("<null>");
    }
    let bytes = unsafe { std::slice::from_raw_parts(p as *const u8, size.min(64)) };
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let printable: String = bytes
        .iter()
        .map(|b| {
            if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            }
        })
        .collect();
    format!("{hex} |{printable}|")
}

// ============================================================
//  مسار الوحدة + تحميل المترجم الحقيقي
// ============================================================

unsafe fn module_file_name(module: *mut c_void) -> Option<PathBuf> {
    let mut buf = [0i8; 512];
    let n = GetModuleFileNameA(module, buf.as_mut_ptr(), buf.len() as DWORD);
    if n == 0 || n as usize >= buf.len() {
        return None;
    }
    let bytes: Vec<u8> = buf.iter().take(n as usize).map(|c| *c as u8).collect();
    Some(PathBuf::from(String::from_utf8_lossy(&bytes).to_string()))
}

/// مسار **هذا الغلاف** (وليس مسار اللعبة)
fn shim_path() -> Option<PathBuf> {
    unsafe {
        let mut h: *mut c_void = std::ptr::null_mut();
        let addr = std::ptr::addr_of!(MODULE_ANCHOR) as *const c_char;
        let flags =
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        if GetModuleHandleExA(flags, addr, &mut h) == 0 || h.is_null() {
            return None;
        }
        module_file_name(h)
    }
}

fn system_dir() -> Option<PathBuf> {
    unsafe {
        let mut buf = [0i8; 512];
        let n = GetSystemDirectoryA(buf.as_mut_ptr(), buf.len() as UINT);
        if n == 0 || n as usize >= buf.len() {
            return None;
        }
        let bytes: Vec<u8> = buf.iter().take(n as usize).map(|c| *c as u8).collect();
        Some(PathBuf::from(String::from_utf8_lossy(&bytes).to_string()))
    }
}

/// ترتيب البحث عن المترجم الحقيقي:
/// 1. `JISR_D3DCOMPILER_BACKEND` (تشخيص/بديل)
/// 2. مجلد الغلاف نفسه (`syswow64\d3dcompiler_43_ms.dll`)
/// 3. مجلد النظام (نفس المكان فعلياً — احتياط)
/// 4. اسم مجرّد يترك Wine يبحث في مسارها
fn backend_candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("JISR_D3DCOMPILER_BACKEND") {
        let p = p.trim();
        if !p.is_empty() {
            out.push(PathBuf::from(p));
        }
    }
    if let Some(m) = shim_path() {
        if let Some(dir) = m.parent() {
            out.push(dir.join("d3dcompiler_43_ms.dll"));
        }
    }
    if let Some(dir) = system_dir() {
        out.push(dir.join("d3dcompiler_43_ms.dll"));
    }
    out.push(PathBuf::from("d3dcompiler_43_ms.dll"));
    out
}

type CompileFn = unsafe extern "system" fn(
    LPCVOID,
    SIZE_T,
    LPCSTR,
    *const c_void,
    *const c_void,
    LPCSTR,
    LPCSTR,
    UINT,
    UINT,
    *mut *mut c_void,
    *mut *mut c_void,
) -> HRESULT;

type AssembleFn = unsafe extern "system" fn(
    LPCVOID,
    SIZE_T,
    LPCSTR,
    *const c_void,
    *const c_void,
    UINT,
    *mut *mut c_void,
    *mut *mut c_void,
) -> HRESULT;

type CreateBlobFn = unsafe extern "system" fn(SIZE_T, *mut *mut c_void) -> HRESULT;

type DisassembleFn =
    unsafe extern "system" fn(LPCVOID, SIZE_T, UINT, *mut *mut c_void, *mut *mut c_void) -> HRESULT;

type GetBlobFn = unsafe extern "system" fn(LPCVOID, SIZE_T, *mut *mut c_void) -> HRESULT;

type GetBlobPartFn =
    unsafe extern "system" fn(LPCVOID, SIZE_T, UINT, UINT, *mut *mut c_void) -> HRESULT;

type PreprocessFn = unsafe extern "system" fn(
    LPCVOID,
    SIZE_T,
    LPCSTR,
    *const c_void,
    *const c_void,
    *mut *mut c_void,
    *mut *mut c_void,
) -> HRESULT;

type ReflectFn = unsafe extern "system" fn(LPCVOID, SIZE_T, REFIID, *mut *mut c_void) -> HRESULT;

type StripShaderFn = unsafe extern "system" fn(LPCVOID, SIZE_T, UINT, *mut *mut c_void) -> HRESULT;

struct Backend {
    module: *mut c_void,
    path: String,
    compile: Option<CompileFn>,
    assemble: Option<AssembleFn>,
    create_blob: Option<CreateBlobFn>,
    disassemble: Option<DisassembleFn>,
    get_blob_part: Option<GetBlobPartFn>,
    get_debug_info: Option<GetBlobFn>,
    get_inout_sig: Option<GetBlobFn>,
    get_in_sig: Option<GetBlobFn>,
    get_out_sig: Option<GetBlobFn>,
    preprocess: Option<PreprocessFn>,
    reflect: Option<ReflectFn>,
    strip_shader: Option<StripShaderFn>,
}

unsafe impl Send for Backend {}
unsafe impl Sync for Backend {}

fn load_backend() -> Option<Backend> {
    static BOOT: AtomicBool = AtomicBool::new(false);
    if !BOOT.swap(true, Ordering::Relaxed) {
        log_line(&format!(
            "BOOT shim=d3dcompiler_43 v2 module={:?} log={:?}",
            shim_path().map(|p| p.display().to_string()),
            log_path().display().to_string()
        ));
    }

    for cand in backend_candidates() {
        let c = match CString::new(cand.to_string_lossy().as_bytes()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let module = unsafe { LoadLibraryA(c.as_ptr()) };
        if module.is_null() {
            let err = unsafe { GetLastError() };
            log_line(&format!(
                "backend: load failed path={} err={err}",
                cand.display()
            ));
            continue;
        }
        log_line(&format!("backend: loaded path={}", cand.display()));

        let sym = |name: &str| -> *mut c_void {
            let c = CString::new(name).unwrap_or_default();
            unsafe { GetProcAddress(module, c.as_ptr()) }
        };

        return Some(Backend {
            module,
            path: cand.display().to_string(),
            compile: unsafe { std::mem::transmute(sym("D3DCompile")) },
            assemble: unsafe { std::mem::transmute(sym("D3DAssemble")) },
            create_blob: unsafe { std::mem::transmute(sym("D3DCreateBlob")) },
            disassemble: unsafe { std::mem::transmute(sym("D3DDisassemble")) },
            get_blob_part: unsafe { std::mem::transmute(sym("D3DGetBlobPart")) },
            get_debug_info: unsafe { std::mem::transmute(sym("D3DGetDebugInfo")) },
            get_inout_sig: unsafe { std::mem::transmute(sym("D3DGetInputAndOutputSignatureBlob")) },
            get_in_sig: unsafe { std::mem::transmute(sym("D3DGetInputSignatureBlob")) },
            get_out_sig: unsafe { std::mem::transmute(sym("D3DGetOutputSignatureBlob")) },
            preprocess: unsafe { std::mem::transmute(sym("D3DPreprocess")) },
            reflect: unsafe { std::mem::transmute(sym("D3DReflect")) },
            strip_shader: unsafe { std::mem::transmute(sym("D3DStripShader")) },
        });
    }

    log_line("backend: NOT FOUND — every D3DCompile will return E_FAIL");
    None
}

/// نحاول التحميل مرة واحدة؛ لو فشل نعيد المحاولة في الاستدعاء التالي
/// (بدل `OnceLock` الذي كان يخزّن الفشل للأبد).
static BACKEND: OnceLock<Backend> = OnceLock::new();

fn backend() -> Option<&'static Backend> {
    if let Some(b) = BACKEND.get() {
        return Some(b);
    }
    let b = load_backend()?;
    let _ = BACKEND.set(b);
    BACKEND.get()
}

// ============================================================
//  أدوات الـblob ونص الخطأ
// ============================================================

type BlobGetBuffer = unsafe extern "system" fn(*mut c_void) -> *const c_void;
type BlobGetSize = unsafe extern "system" fn(*mut c_void) -> usize;

/// (الطول، النص) من `ID3D10Blob` — vtable: QI/AddRef/Release ثم
/// GetBufferPointer (3) وGetBufferSize (4).
unsafe fn blob_info(p: *mut c_void) -> (usize, String) {
    if p.is_null() {
        return (0, String::from("<null>"));
    }
    let vt = *(p as *const *const *const c_void);
    if vt.is_null() {
        return (0, String::from("<no-vtable>"));
    }
    let get_buf: BlobGetBuffer = std::mem::transmute(*vt.add(3));
    let get_size: BlobGetSize = std::mem::transmute(*vt.add(4));
    let b = get_buf(p);
    let n = get_size(p);
    if b.is_null() || n == 0 {
        return (0, String::from("<empty>"));
    }
    let bytes = std::slice::from_raw_parts(b as *const u8, n.min(1024));
    let text = String::from_utf8_lossy(bytes)
        .replace('\0', " ")
        .replace('\r', " ")
        .replace('\n', " | ")
        .trim()
        .to_string();
    (n, text)
}

/// نقرأ نص C-string بحذر (قد يكون NULL)
unsafe fn cstr(p: LPCSTR) -> String {
    if p.is_null() {
        return String::from("<null>");
    }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

/// عدد الـdefines (سلسلة مرتبطة)
unsafe fn count_defines(p: *const c_void) -> usize {
    if p.is_null() {
        return 0;
    }
    // D3D_SHADER_MACRO { LPCSTR Name; LPCSTR Definition; struct* Next; } — 12 بايت على 32-بت
    let mut n = 0usize;
    let mut cur = p as *const *const c_void;
    while !cur.is_null() && n < 64 {
        let name = *cur as *const c_char;
        if name.is_null() {
            break;
        }
        n += 1;
        let next = *cur.add(2) as *const *const c_void;
        if next == cur {
            break;
        }
        cur = next;
    }
    n
}

// ============================================================
//  الحراسة
// ============================================================

const PS_PROFILE: &[u8] = b"ps_3_0\0";
const VS_PROFILE: &[u8] = b"vs_3_0\0";

/// نزيل تعليقات `//` و`/* */` قبل تحليل السمات — تعليق فيه `POSITION`
/// ما لازم يغيّر نوع الشيدر. نُبقي الأسطر (نستبدل المحتوى بمسافات) حتى
/// تبقى أرقام الأسطر في أخطاء المترجم صحيحة.
fn without_comments(src: &[u8]) -> Vec<u8> {
    let mut out = src.to_vec();
    let n = out.len();
    let mut i = 0;
    while i < n {
        if i + 1 < n && out[i] == b'/' && out[i + 1] == b'/' {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i < n && out[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if i + 1 < n && out[i] == b'/' && out[i + 1] == b'*' {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i + 1 < n {
                if out[i] == b'*' && out[i + 1] == b'/' {
                    out[i] = b' ';
                    out[i + 1] = b' ';
                    i += 2;
                    break;
                }
                if out[i] != b'\n' {
                    out[i] = b' ';
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// هل توجد سمة `name` مسبوقة بنقطتين؟ يقبل `POSITION0` و`:POSITION`
/// و`:  POSITION`، ويرفض اسم متغيّر عادي مثل `position`.
fn has_semantic(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find(name) {
        let pos = from + rel;
        let end = pos + name.len();
        // بعد الاسم: مسموح رقم فهرس السمة (POSITION0)، وممنوع حرف/شرطة سفلية
        let after_ok = match bytes.get(end) {
            Some(c) => !c.is_ascii_alphabetic() && *c != b'_',
            None => true,
        };
        if after_ok {
            let mut j = pos;
            while j > 0 && bytes[j - 1].is_ascii_whitespace() {
                j -= 1;
            }
            if j > 0 && bytes[j - 1] == b':' {
                return true;
            }
        }
        from = end.max(from + 1);
    }
    false
}

/// نستنتج profile من نص الشيدر لمّا تجي NULL.
///
/// **قاعدة D3D9:** `POSITION` سمة مرحلة الرؤوس فقط؛ البكسل يستخدم `VPOS`
/// ويُخرج `COLOR`/`DEPTH`. لذلك وجود `POSITION` (حتى لو معه `COLOR0`)
/// يعني **vs_3_0**، وليس ps_3_0. هذا كان سبب خطأ LIMBO الجديد:
/// شيدر رؤوس فيه POSITION + COLOR0، والكود القديم رآه «بكسل» بسبب COLOR0.
fn profile_from_source(src: &[u8]) -> (&'static [u8], &'static str) {
    let clean = without_comments(src);
    let text = String::from_utf8_lossy(&clean).to_ascii_lowercase();
    if text.contains("sv_position") || has_semantic(&text, "position") {
        return (VS_PROFILE, "POSITION");
    }
    if text.contains("sv_target") || has_semantic(&text, "vpos") {
        return (PS_PROFILE, "SV_Target/VPOS");
    }
    (PS_PROFILE, "default")
}

unsafe fn guess_profile(p_src: LPCVOID, size: usize) -> (&'static [u8], &'static str) {
    if p_src.is_null() || size == 0 {
        return (PS_PROFILE, "empty");
    }
    let bytes = std::slice::from_raw_parts(p_src as *const u8, size.min(64 * 1024));
    profile_from_source(bytes)
}

unsafe fn guard_target(
    target: LPCSTR,
    p_src: LPCVOID,
    size: usize,
) -> (*const c_char, bool, &'static str) {
    if !target.is_null() && *target != 0 {
        return (target, false, "");
    }
    let (profile, why) = guess_profile(p_src, size);
    (profile.as_ptr() as *const c_char, true, why)
}

unsafe fn guard_entry(entry: LPCSTR) -> (*const c_char, bool) {
    if !entry.is_null() && *entry != 0 {
        return (entry, false);
    }
    (b"main\0".as_ptr() as *const c_char, true)
}

// ============================================================
//  التصدير الأساسي: D3DCompile
// ============================================================

#[no_mangle]
pub unsafe extern "system" fn D3DCompile(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    pSourceName: LPCSTR,
    pDefines: *const c_void,
    pInclude: *const c_void,
    pEntrypoint: LPCSTR,
    pTarget: LPCSTR,
    Flags1: UINT,
    Flags2: UINT,
    ppCode: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
) -> HRESULT {
    let (target, t_sub, t_why) = guard_target(pTarget, pSrcData, SrcDataSize);
    let (entry, e_sub) = guard_entry(pEntrypoint);
    let guessed = if t_sub {
        format!(" (NULL=>guessed:{t_why})")
    } else {
        String::new()
    };
    log_line(&format!(
        "D3DCompile src={} size={} entry={}{} target={}{} flags=0x{Flags1:x}/0x{Flags2:x} defines={} include={}",
        if pSrcData.is_null() { "NULL" } else { "ok" },
        SrcDataSize,
        cstr(entry),
        if e_sub { " (NULL=>main)" } else { "" },
        cstr(target),
        guessed,
        count_defines(pDefines),
        if pInclude.is_null() { "null" } else { "set" },
    ));

    if pSrcData.is_null() || SrcDataSize == 0 {
        log_line(&format!(
            "D3DCompile: empty source => E_FAIL (guard) head={}",
            source_head(pSrcData, SrcDataSize)
        ));
        return E_FAIL;
    }

    let Some(b) = backend() else {
        log_line("D3DCompile: no backend => E_FAIL");
        return E_FAIL;
    };
    let Some(f) = b.compile else {
        log_line(&format!(
            "D3DCompile: {} has no D3DCompile => E_FAIL",
            b.path
        ));
        return E_FAIL;
    };

    let hr = f(
        pSrcData,
        SrcDataSize,
        pSourceName,
        pDefines,
        pInclude,
        entry,
        target,
        Flags1,
        Flags2,
        ppCode,
        ppErrorMsgs,
    );

    let code = if ppCode.is_null() {
        std::ptr::null_mut()
    } else {
        *ppCode
    };
    let err = if ppErrorMsgs.is_null() {
        std::ptr::null_mut()
    } else {
        *ppErrorMsgs
    };
    let (bytes, _) = if hr == S_OK {
        blob_info(code)
    } else {
        (0, String::new())
    };
    let err_text = if err.is_null() {
        String::from("<none>")
    } else {
        blob_info(err).1
    };
    log_line(&format!(
        "D3DCompile => hr=0x{:08X} code={:p} bytes={bytes} err={err_text} head={}",
        hr,
        code,
        source_head(pSrcData, SrcDataSize)
    ));
    hr
}

// ============================================================
//  بقية تصديرات d3dcompiler_43 (17 تصديراً)
// ============================================================

#[no_mangle]
pub unsafe extern "system" fn D3DAssemble(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    pSourceName: LPCSTR,
    pDefines: *const c_void,
    pInclude: *const c_void,
    Flags: UINT,
    ppShader: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
) -> HRESULT {
    log_line(&format!(
        "D3DAssemble src={} size={} flags=0x{Flags:x}",
        if pSrcData.is_null() { "NULL" } else { "ok" },
        SrcDataSize
    ));
    if pSrcData.is_null() || SrcDataSize == 0 {
        return E_FAIL;
    }
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.assemble else {
        log_line("D3DAssemble: backend has no export => E_NOTIMPL");
        return E_NOTIMPL;
    };
    let hr = f(
        pSrcData,
        SrcDataSize,
        pSourceName,
        pDefines,
        pInclude,
        Flags,
        ppShader,
        ppErrorMsgs,
    );
    log_line(&format!("D3DAssemble => hr=0x{hr:08X}"));
    hr
}

#[no_mangle]
pub unsafe extern "system" fn D3DCreateBlob(Size: SIZE_T, ppBlob: *mut *mut c_void) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.create_blob else {
        log_line("D3DCreateBlob: backend has no export => E_NOTIMPL");
        return E_NOTIMPL;
    };
    f(Size, ppBlob)
}

#[no_mangle]
pub unsafe extern "system" fn D3DDisassemble(
    pCode: LPCVOID,
    Size: SIZE_T,
    Flags: UINT,
    ppDisasm: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.disassemble else {
        log_line("D3DDisassemble: backend has no export => E_NOTIMPL");
        return E_NOTIMPL;
    };
    f(pCode, Size, Flags, ppDisasm, ppErrorMsgs)
}

#[no_mangle]
pub unsafe extern "system" fn D3DGetBlobPart(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    Part: UINT,
    Flags: UINT,
    ppPart: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.get_blob_part else {
        log_line("D3DGetBlobPart: backend has no export => E_NOTIMPL");
        return E_NOTIMPL;
    };
    f(pSrcData, SrcDataSize, Part, Flags, ppPart)
}

#[no_mangle]
pub unsafe extern "system" fn D3DGetDebugInfo(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    ppDebugInfo: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.get_debug_info else {
        return E_NOTIMPL;
    };
    f(pSrcData, SrcDataSize, ppDebugInfo)
}

#[no_mangle]
pub unsafe extern "system" fn D3DGetInputAndOutputSignatureBlob(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    ppSignatureBlob: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.get_inout_sig else {
        return E_NOTIMPL;
    };
    f(pSrcData, SrcDataSize, ppSignatureBlob)
}

#[no_mangle]
pub unsafe extern "system" fn D3DGetInputSignatureBlob(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    ppSignatureBlob: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.get_in_sig else {
        return E_NOTIMPL;
    };
    f(pSrcData, SrcDataSize, ppSignatureBlob)
}

#[no_mangle]
pub unsafe extern "system" fn D3DGetOutputSignatureBlob(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    ppSignatureBlob: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.get_out_sig else {
        return E_NOTIMPL;
    };
    f(pSrcData, SrcDataSize, ppSignatureBlob)
}

#[no_mangle]
pub unsafe extern "system" fn D3DPreprocess(
    pSrcData: LPCVOID,
    SrcDataSize: SIZE_T,
    pSourceName: LPCSTR,
    pDefines: *const c_void,
    pInclude: *const c_void,
    ppCodeText: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.preprocess else {
        log_line("D3DPreprocess: backend has no export => E_NOTIMPL");
        return E_NOTIMPL;
    };
    f(
        pSrcData,
        SrcDataSize,
        pSourceName,
        pDefines,
        pInclude,
        ppCodeText,
        ppErrorMsgs,
    )
}

#[no_mangle]
pub unsafe extern "system" fn D3DReflect(
    pData: LPCVOID,
    DataSize: SIZE_T,
    riid: REFIID,
    ppReflector: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.reflect else {
        return E_NOTIMPL;
    };
    f(pData, DataSize, riid, ppReflector)
}

#[no_mangle]
pub unsafe extern "system" fn D3DStripShader(
    pShaderBytecode: LPCVOID,
    BytecodeLength: SIZE_T,
    uStripFlags: UINT,
    ppStrippedBlob: *mut *mut c_void,
) -> HRESULT {
    let Some(b) = backend() else {
        return E_FAIL;
    };
    let Some(f) = b.strip_shader else {
        return E_NOTIMPL;
    };
    f(pShaderBytecode, BytecodeLength, uStripFlags, ppStrippedBlob)
}

// ------------------------------------------------------------
//  تصديرات قديمة موجودة في d3dcompiler_43 لكنها stubs حتى في Wine.
//  نصدّرها حتى يرى أي برنامج فحص التصديرات أن الغلاف مكتمل.
// ------------------------------------------------------------

#[no_mangle]
pub extern "system" fn DebugSetMute() {}

#[no_mangle]
pub extern "system" fn D3DReturnFailure1() -> HRESULT {
    E_FAIL
}

#[no_mangle]
pub unsafe extern "system" fn D3DCompressShaders(
    _uNumShaders: UINT,
    _pShaderData: *const c_void,
    _uFlags: UINT,
    _ppCompressedData: *mut *mut c_void,
) -> HRESULT {
    E_NOTIMPL
}

#[no_mangle]
pub unsafe extern "system" fn D3DDecompressShaders(
    _uNumShaders: UINT,
    _pShaderData: *const c_void,
    _uSrcDataSize: UINT,
    _uStartIndex: UINT,
    _pIndices: *mut UINT,
    _uFlags: UINT,
    _ppShaders: *mut *mut c_void,
    _pTotalShaders: *mut UINT,
) -> HRESULT {
    E_NOTIMPL
}

#[no_mangle]
pub unsafe extern "system" fn D3DDisassemble10Effect(
    _pEffect: *const c_void,
    _Flags: UINT,
    _ppDisassembly: *mut *mut c_void,
) -> HRESULT {
    E_NOTIMPL
}

/// **بصمة الغلاف** — التطبيق يقرأ هذه السلسلة من الملف ليعرف أن المثبَّت
/// هو غلافنا (لا نسخة مايكروسوفت).
#[no_mangle]
pub static JISR_D3DX_SHIM_V1: [u8; 17] = *b"JISR_D3DX_SHIM_V1";

// نمنع تحذير S_OK غير المستخدم في بعض إعدادات البناء
#[allow(dead_code)]
const _: HRESULT = S_OK;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_wins_over_color_output() {
        // نفس شكل شيدر LIMBO: رؤوس فيه POSITION + COLOR0/TEXCOORD0.
        // الكود القديم كان يختاره ps_3_0 بسبب color0 ⇒ أخطاء X4502.
        let src = br#"
            struct VS_OUT {
                float4 pos : POSITION;
                float4 col : COLOR0;
                float2 uv  : TEXCOORD0;
            };
            VS_OUT main(float4 p : POSITION) {
                VS_OUT o;
                o.pos = p; o.col = 1; o.uv = 0;
                return o;
            }
        "#;
        assert_eq!(profile_from_source(src).0, VS_PROFILE);
    }

    #[test]
    fn pixel_shader_stays_pixel() {
        let src = br#"
            sampler2D s;
            float4 main(float2 uv : TEXCOORD0) : COLOR0 { return tex2D(s, uv); }
        "#;
        assert_eq!(profile_from_source(src).0, PS_PROFILE);
    }

    #[test]
    fn position_in_comment_is_ignored() {
        let src = br#"
            // input : POSITION is handled by the vertex stage
            float4 main(float2 uv : TEXCOORD0) : COLOR0 { return float4(uv, 0, 1); }
        "#;
        assert_eq!(profile_from_source(src).0, PS_PROFILE);
    }

    #[test]
    fn no_space_after_colon_and_indexed_semantic() {
        let src = br#"float4 main(float4 p:POSITION0) : POSITION { return p; }"#;
        assert_eq!(profile_from_source(src).0, VS_PROFILE);
    }

    #[test]
    fn identifier_named_position_is_not_a_semantic() {
        let src = br#"
            float4 main(float2 uv : TEXCOORD0) : COLOR0 {
                float4 position = float4(uv, 0, 1);
                return position;
            }
        "#;
        assert_eq!(profile_from_source(src).0, PS_PROFILE);
    }

    #[test]
    fn vpos_is_pixel() {
        let src = br#"float4 main(float2 p : VPOS) : COLOR0 { return float4(p, 0, 1); }"#;
        assert_eq!(profile_from_source(src).0, PS_PROFILE);
        assert_eq!(profile_from_source(src).1, "SV_Target/VPOS");
    }
}
