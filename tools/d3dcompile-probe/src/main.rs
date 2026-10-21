//! مُسبار: يستدعي `D3DCompile` من **مترجم مايكروسوفت الأصلي** بمدخلات مختلفة
//! ليعرف أيّها يُسقط العملية (page fault) — نفس انهيار LIMBO.
//!
//! **الفرضية:** اللعبة تمرّر مصدراً فارغاً (NULL) أو طول صفر، ومترجم
//! مايكروسوفت لا يتحقق من المدخل ⇒ يقرأ من 0x0 وينهار.
//!
//! التشغيل: `d3dcompile-probe.exe <case>` حيث case:
//! `null_src` · `empty_src` · `zero_len` · `null_entry` · `null_target` · `ok`
//!
//! نطبع عنوان الخطأ (ExceptionAddress) وقاعدة الـDLL ⇒ نقارنه بـ
//! `d3dcompiler_43+0x1e96a9` من تقرير LIMBO.

use std::ffi::c_void;

type HResult = i32;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn AddVectoredExceptionHandler(first: u32, handler: usize) -> *mut c_void;
    fn ExitProcess(code: u32) -> !;
}

/// `EXCEPTION_POINTERS { ExceptionRecord, ContextRecord }`
#[repr(C)]
struct ExceptionPointers {
    record: *mut u8,     // EXCEPTION_RECORD*
    context: *mut c_void, // CONTEXT*
}

/// أول معالج استثناء: نطبع العنوان ونخرج (بدل حوار «توقف البرنامج»).
extern "system" fn on_exception(info: *mut ExceptionPointers) -> i32 {
    unsafe {
        let rec = (*info).record;
        // EXCEPTION_RECORD: code(0) flags(4) nested(8) address(12)
        let addr = *(rec.add(12) as *const usize);
        // ExceptionInformation[0] = read/write, [1] = العنوان المعطوب
        let bad = *(rec.add(20 + std::mem::size_of::<usize>()) as *const usize);
        println!("CRASH at 0x{addr:08x} (bad=0x{bad:08x})");
    }
    unsafe { ExitProcess(99) }
}

type CompileFn = unsafe extern "system" fn(
    *const c_void, // pSrcData
    usize,         // SrcDataSize
    *const u8,     // pSourceName
    *const c_void, // pDefines
    *const c_void, // pInclude
    *const u8,     // pEntrypoint
    *const u8,     // pTarget
    u32,           // Flags1
    u32,           // Flags2
    *mut *mut c_void,
    *mut *mut c_void,
) -> HResult;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let case = args.get(1).map(String::as_str).unwrap_or("null_src");
    let dll = args.get(2).cloned().unwrap_or_else(|| {
        r"C:\Users\WhoisMelv\AppData\Local\Temp\opencode\dxredist\dll\D3DCompiler_43.dll"
            .to_string()
    });

    unsafe {
        AddVectoredExceptionHandler(1, on_exception as usize);
        let cpath = std::ffi::CString::new(dll.clone()).unwrap();
        let module = LoadLibraryA(cpath.as_ptr() as *const u8);
        if module.is_null() {
            println!("LOAD FAILED: {dll}");
            return;
        }
        println!("module base = 0x{:08x}", module as usize);
        let sym = GetProcAddress(module, b"D3DCompile\0".as_ptr());
        if sym.is_null() {
            println!("D3DCompile NOT FOUND");
            return;
        }
        let compile: CompileFn = std::mem::transmute(sym);

        let ps_src: &[u8] = b"float4 main() : SV_Target { return float4(1,0,0,1); }\n";
        let vs_src: &[u8] = b"struct VS_OUT { float4 pos : POSITION; float4 col : COLOR0; float2 uv : TEXCOORD0; };\nVS_OUT main(float4 p : POSITION) { VS_OUT o; o.pos = p; o.col = 1; o.uv = 0; return o; }\n";
        let src: &[u8] = if case == "null_target_vs" || case == "vs_ok" {
            vs_src
        } else {
            ps_src
        };
        let (p_src, len) = match case {
            "null_src" => (std::ptr::null(), 0usize),
            "empty_src" => (b"\0".as_ptr() as *const c_void, 0usize),
            "zero_len" => (src.as_ptr() as *const c_void, 0usize),
            _ => (src.as_ptr() as *const c_void, src.len()),
        };
        let entry: *const u8 = if case == "null_entry" {
            std::ptr::null()
        } else {
            b"main\0".as_ptr()
        };
        let target: *const u8 = match case {
            "null_target" | "null_target_vs" => std::ptr::null(),
            "vs_ok" => b"vs_3_0\0".as_ptr(),
            _ => b"ps_3_0\0".as_ptr(),
        };

        let mut code: *mut c_void = std::ptr::null_mut();
        let mut errs: *mut c_void = std::ptr::null_mut();
        let hr = compile(
            p_src,
            len,
            b"probe.hlsl\0".as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            entry,
            target,
            0,
            0,
            &mut code,
            &mut errs,
        );
        println!(
            "case={case} hr=0x{:08x} code={:?} errs={:?}",
            hr as u32, code, errs
        );
    }
}
