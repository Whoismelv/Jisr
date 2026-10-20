//! Jisr — غلاف تشخيص لمترجم الـHLSL (`d3dcompiler_43`)
//!
//! **لماذا:** ألعاب DirectX 9 القديمة (LIMBO مثلًا) تنهار داخل مترجم
//! Microsoft's `d3dcompiler_43` نفسه (page fault على مؤشر فارغ)، وكذلك تنهار
//! داخل نسخة Wine المدمجة. أي نصلّح طبقة D3DX ⇒ ينتقل الانهيار.
//!
//! **هذا الغلاف** يجلس بين اللعبة والمترجم الحقيقي:
//! 1. **مِجهر**: يسجّل كل استدعاء (المصدر وطوله، الـdefines، الـinclude، الهدف
//!    والـflags) ⇒ نعرف بالضبط ما الذي يعطّل المترجم.
//! 2. **حارس**: لو المدخل باطل (مصدر NULL أو طول صفر) ⇒ يرجع خطأ نظيف
//!    بدل قفزة الذاكرة (انهيار قابل للقراءة بدل قفزة عشوائية).
//! 3. **مِطرقة**: يوجّه الاستدعاء لمترجم **آخر** عبر المتغيّر
//!    `JISR_D3DCOMPILER_BACKEND` (مثلاً d3dcompiler_47) ⇒ نجرّب بديل
//!    بدون ما نلمس اللعبة.
//!
//! المرجع الحقيقي يُحمَّل من `d3dcompiler_43_ms.dll` (نسخة مايكروسوفت)
//! أو من المسار المعطى في `JISR_D3DCOMPILER_BACKEND`.
//!
//! **كل الاستدعاءات تمرّ كما هي** — ما نغيّر سلوك المترجم إلا في حالة
//! المدخل الفارغ (وهو بالضبط ما كان يfoil الانهيار).

#![allow(non_snake_case)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
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

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn GetModuleFileNameA(module: *mut c_void, buf: *mut c_char, size: DWORD) -> DWORD;
    fn GetLastError() -> DWORD;
}

// ============================================================
//  سجل الاستدعاءات
// ============================================================

/// مسار السجل: من `JISR_D3DX_LOG` أو بجانب الـDLL
fn log_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JISR_D3DX_LOG") {
        let path = PathBuf::from(p);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        return Some(path);
    }
    None
}

/// نكتب سطراً واحداً (best-effort — ما نبيّن فشل التسجيل للعبة)
fn log_line(line: &str) {
    let Some(path) = log_path() else { return };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// أول ٦٤ بايت من المصدر (كـhex) — تكفي للتعرّف على الشيدر
fn source_head(p: *const c_void, size: usize) -> String {
    if p.is_null() || size == 0 {
        return String::from("<null>");
    }
    let bytes = unsafe { std::slice::from_raw_parts(p as *const u8, size.min(64)) };
    let hex: String = bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("");
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
//  تحميل المترجم الحقيقي
// ============================================================

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

type Compile2Fn = unsafe extern "system" fn(
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
    *mut UINT,
    *mut DWORD,
) -> HRESULT;

type CompileFromFileFn = unsafe extern "system" fn(
    LPCWSTR,
    LPCSTR,
    LPCSTR,
    UINT,
    UINT,
    *mut *mut c_void,
    *mut *mut c_void,
) -> HRESULT;

type CompileFromFile2Fn = unsafe extern "system" fn(
    LPCWSTR,
    LPCSTR,
    LPCSTR,
    UINT,
    UINT,
    *mut *mut c_void,
    *mut *mut c_void,
    *mut UINT,
    *mut DWORD,
) -> HRESULT;

type DisassembleFn =
    unsafe extern "system" fn(LPCVOID, SIZE_T, UINT, *mut *mut c_void, *mut *mut c_void) -> HRESULT;

type ReflectFn =
    unsafe extern "system" fn(LPCVOID, SIZE_T, REFIID, *mut *mut c_void) -> HRESULT;

type GetBlobSizeFn = unsafe extern "system" fn(LPCVOID) -> SIZE_T;
type GetErrorFn = unsafe extern "system" fn(LPCVOID) -> *const c_char;

/// الوحدة الحقيقية للمترجم + عناوين الدوال
struct Backend {
    module: *mut c_void,
    compile: Option<CompileFn>,
    compile2: Option<Compile2Fn>,
    compile_from_file: Option<CompileFromFileFn>,
    compile_from_file2: Option<CompileFromFile2Fn>,
    disassemble: Option<DisassembleFn>,
    reflect: Option<ReflectFn>,
    get_blob_size: Option<GetBlobSizeFn>,
    get_error: Option<GetErrorFn>,
}

unsafe impl Send for Backend {}
unsafe impl Sync for Backend {}

/// مسار الـDLL الحقيقي: `JISR_D3DCOMPILER_BACKEND` أو `d3dcompiler_43_ms.dll` بجانبنا
fn real_backend_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JISR_D3DCOMPILER_BACKEND") {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p.trim()));
        }
    }
    // مجلد هذا الـDLL + اسم ملف النسخة الأصلية
    let mut buf = [0i8; 260];
    unsafe {
        let h = GetModuleFileNameA(std::ptr::null_mut(), buf.as_mut_ptr(), 260);
        if h == 0 {
            return None;
        }
        let bytes: Vec<u8> = buf
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8)
            .collect();
        let path = PathBuf::from(String::from_utf8_lossy(&bytes).to_string());
        let dir = path.parent()?.to_path_buf();
        return Some(dir.join("d3dcompiler_43_ms.dll"));
    }
}

fn backend() -> &'static Backend {
    static BACKEND: OnceLock<Backend> = OnceLock::new();
    BACKEND.get_or_init(|| {
        let path = real_backend_path();
        let module = match &path {
            Some(p) => {
                let c = CString::new(p.to_string_lossy().as_bytes()).unwrap_or_default();
                unsafe { LoadLibraryA(c.as_ptr()) }
            }
            None => std::ptr::null_mut(),
        };
        if module.is_null() {
            let err = unsafe { GetLastError() };
            log_line(&format!(
                "[jisr] d3dx-shim: تعذّر تحميل المترجم الحقيقي (GetLastError={err}) — path={:?}",
                path
            ));
        } else {
            log_line(&format!(
                "[jisr] d3dx-shim: المترجم الحقيقي = {:?}",
                path.as_ref().map(|p| p.display().to_string())
            ));
        }
        let sym = |name: &str| -> *mut c_void {
            if module.is_null() {
                return std::ptr::null_mut();
            }
            let c = CString::new(name).unwrap_or_default();
            unsafe { GetProcAddress(module, c.as_ptr()) }
        };
        Backend {
            module,
            compile: unsafe { std::mem::transmute(sym("D3DCompile")) },
            compile2: unsafe { std::mem::transmute(sym("D3DCompile2")) },
            compile_from_file: unsafe { std::mem::transmute(sym("D3DCompileFromFile")) },
            compile_from_file2: unsafe { std::mem::transmute(sym("D3DCompileFromFile2")) },
            disassemble: unsafe { std::mem::transmute(sym("D3DDisassemble")) },
            reflect: unsafe { std::mem::transmute(sym("D3DReflect")) },
            get_blob_size: unsafe { std::mem::transmute(sym("D3DGetBlobSize")) },
            get_error: unsafe { std::mem::transmute(sym("D3DGetError")) },
        }
    })
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
//  الدوال المصدَّرة (نفس أسماء d3dcompiler_43)
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
    log_line(&format!(
        "[jisr] D3DCompile src={} size={} entry={} target={} flags1={} flags2={} defines={} include={}",
        if pSrcData.is_null() { "NULL" } else { "ok" },
        SrcDataSize,
        cstr(pEntrypoint),
        cstr(pTarget),
        Flags1,
        Flags2,
        count_defines(pDefines),
        if pInclude.is_null() { "null" } else { "set" },
    ));
    // حارس: مدخل فارغ ⇒ خطأ نظيف بدل قفزة الذاكرة
    if pSrcData.is_null() || SrcDataSize == 0 {
        log_line(&format!(
            "[jisr] D3DCompile: مصدر فارغ ⇒ نرجع E_FAIL (بدل انهيار). head={}",
            source_head(pSrcData, SrcDataSize)
        ));
        return E_FAIL;
    }
    log_line(&format!(
        "[jisr] D3DCompile head={}",
        source_head(pSrcData, SrcDataSize)
    ));
    let b = backend();
    match b.compile {
        Some(f) => f(
            pSrcData,
            SrcDataSize,
            pSourceName,
            pDefines,
            pInclude,
            pEntrypoint,
            pTarget,
            Flags1,
            Flags2,
            ppCode,
            ppErrorMsgs,
        ),
        None => {
            log_line("[jisr] D3DCompile: المترجم الحقيقي ما فيه D3DCompile");
            E_FAIL
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn D3DCompile2(
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
    pNumErrors: *mut UINT,
    pFlags3: *mut DWORD,
) -> HRESULT {
    log_line(&format!(
        "[jisr] D3DCompile2 src={} size={} target={}",
        if pSrcData.is_null() { "NULL" } else { "ok" },
        SrcDataSize,
        cstr(pTarget),
    ));
    if pSrcData.is_null() || SrcDataSize == 0 {
        return E_FAIL;
    }
    let b = backend();
    match b.compile2 {
        Some(f) => f(
            pSrcData,
            SrcDataSize,
            pSourceName,
            pDefines,
            pInclude,
            pEntrypoint,
            pTarget,
            Flags1,
            Flags2,
            ppCode,
            ppErrorMsgs,
            pNumErrors,
            pFlags3,
        ),
        None => E_FAIL,
    }
}

#[no_mangle]
pub unsafe extern "system" fn D3DCompileFromFile(
    pSourceFile: LPCWSTR,
    pEntrypoint: LPCSTR,
    pTarget: LPCSTR,
    Flags1: UINT,
    Flags2: UINT,
    ppCode: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
) -> HRESULT {
    log_line(&format!(
        "[jisr] D3DCompileFromFile entry={} target={}",
        cstr(pEntrypoint),
        cstr(pTarget)
    ));
    let b = backend();
    match b.compile_from_file {
        Some(f) => f(
            pSourceFile,
            pEntrypoint,
            pTarget,
            Flags1,
            Flags2,
            ppCode,
            ppErrorMsgs,
        ),
        None => E_FAIL,
    }
}

#[no_mangle]
pub unsafe extern "system" fn D3DCompileFromFile2(
    pSourceFile: LPCWSTR,
    pEntrypoint: LPCSTR,
    pTarget: LPCSTR,
    Flags1: UINT,
    Flags2: UINT,
    ppCode: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
    pNumErrors: *mut UINT,
    pFlags3: *mut DWORD,
) -> HRESULT {
    log_line(&format!(
        "[jisr] D3DCompileFromFile2 entry={} target={}",
        cstr(pEntrypoint),
        cstr(pTarget)
    ));
    let b = backend();
    match b.compile_from_file2 {
        Some(f) => f(
            pSourceFile,
            pEntrypoint,
            pTarget,
            Flags1,
            Flags2,
            ppCode,
            ppErrorMsgs,
            pNumErrors,
            pFlags3,
        ),
        None => E_FAIL,
    }
}

#[no_mangle]
pub unsafe extern "system" fn D3DDisassemble(
    pCode: LPCVOID,
    Size: SIZE_T,
    Flags: UINT,
    ppDisasm: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
) -> HRESULT {
    let b = backend();
    match b.disassemble {
        Some(f) => f(pCode, Size, Flags, ppDisasm, ppErrorMsgs),
        None => E_FAIL,
    }
}

#[no_mangle]
pub unsafe extern "system" fn D3DReflect(
    pData: LPCVOID,
    DataSize: SIZE_T,
    riid: REFIID,
    ppReflector: *mut *mut c_void,
) -> HRESULT {
    let b = backend();
    match b.reflect {
        Some(f) => f(pData, DataSize, riid, ppReflector),
        None => E_FAIL,
    }
}

#[no_mangle]
pub unsafe extern "system" fn D3DGetBlobSize(pBlob: LPCVOID) -> SIZE_T {
    let b = backend();
    match b.get_blob_size {
        Some(f) => f(pBlob),
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "system" fn D3DGetError(pErrorData: LPCVOID) -> *const c_char {
    let b = backend();
    match b.get_error {
        Some(f) => f(pErrorData),
        None => std::ptr::null(),
    }
}

/// رمز نجاح (D3DCompile يرجّع S_OK) — نصدّره حتى لا complains المحلّل
#[no_mangle]
pub extern "system" fn D3DCompile_Ok() -> HRESULT {
    S_OK
}
