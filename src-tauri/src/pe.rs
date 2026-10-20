//! فاحص ملفات Windows التنفيذية (PE) — قراءة بايتات فقط، بلا تنفيذ.
//!
//! نستخرج منه:
//! • معمارية اللعبة: 32-بت (x86) أو 64-بت (x64)
//! • مستوى DirectX المطلوب من **جدول الاستيراد** (الحقيقة الفعلية، لا التخمين)
//! • هل تحتاج .NET
//!
//! كل القراءات محمية بالحدود: أي ملف تالف يرجع `None` بدل ما ينهار التطبيق.

use std::io::Read;
use std::path::Path;

/// نقرأ أول ٣٢ ميجا فقط — الرؤوس وجدول الاستيراد تكون في البداية دائماً.
const MAX_READ: u64 = 32 * 1024 * 1024;
/// سقف عدد مكتبات الاستيراد اللي نقرأها (ملفات ضخمة قد تكون مشوّهة).
const MAX_IMPORTS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxLevel {
    D3d8,
    D3d9,
    D3d10,
    D3d11,
    D3d12,
    Vulkan,
    OpenGl,
    None,
}

impl DxLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            DxLevel::D3d8 => "d3d8",
            DxLevel::D3d9 => "d3d9",
            DxLevel::D3d10 => "d3d10",
            DxLevel::D3d11 => "d3d11",
            DxLevel::D3d12 => "d3d12",
            DxLevel::Vulkan => "vulkan",
            DxLevel::OpenGl => "opengl",
            DxLevel::None => "none",
        }
    }

    /// أعلى مستوى DirectX موجود في قائمة الاستيراد.
    pub fn from_imports(imports: &[String]) -> DxLevel {
        let has = |n: &str| imports.iter().any(|i| i == n);
        if has("d3d12.dll") || has("d3d12core.dll") {
            DxLevel::D3d12
        } else if has("d3d11.dll") {
            DxLevel::D3d11
        } else if has("d3d10.dll") || has("d3d10_1.dll") || has("d3d10core.dll") {
            DxLevel::D3d10
        } else if has("d3d9.dll") {
            DxLevel::D3d9
        } else if has("d3d8.dll") {
            DxLevel::D3d8
        } else if has("vulkan-1.dll") {
            DxLevel::Vulkan
        } else if has("opengl32.dll") {
            DxLevel::OpenGl
        } else {
            DxLevel::None
        }
    }

    /// هل نستطيع تشغيلها على الماك بمساراتنا الحالية؟
    /// القيمة: `None` = مدعومة، وإلا رمز المانع (تشرحه الواجهة).
    pub fn blocker(self) -> Option<&'static str> {
        match self {
            // D3D8 ما له مسار على الماك (WineD3D/GLSL مكسور، وDXVK ما يدعم d3d8)
            DxLevel::D3d8 => Some("dx8"),
            // D3D12 يحتاج D3DMetal من Apple (ما نقدر نوزّعه) أو vkd3d (تغطية ضعيفة)
            DxLevel::D3d12 => Some("dx12"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeInfo {
    pub machine: u16,
    pub is_64: bool,
    pub dotnet: bool,
    pub imports: Vec<String>,
}

impl PeInfo {
    pub fn dx(&self) -> DxLevel {
        DxLevel::from_imports(&self.imports)
    }

    /// وصف قصير للسجل: `x64 · d3d9 · 12 imports`
    pub fn summary(&self) -> String {
        format!(
            "{} · {} · {} imports{}",
            if self.is_64 { "x64" } else { "x86" },
            self.dx().as_str(),
            self.imports.len(),
            if self.dotnet { " · .NET" } else { "" }
        )
    }
}

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// نص ASCII منتهٍ بصفر (أسماء مكتبات الاستيراد دائماً ASCII).
fn cstr(b: &[u8], off: usize, max: usize) -> Option<String> {
    let end = off.checked_add(max)?.min(b.len());
    let slice = b.get(off..end)?;
    let len = slice.iter().position(|&c| c == 0)?;
    let text = std::str::from_utf8(&slice[..len]).ok()?;
    if text.is_empty() || !text.is_ascii() {
        return None;
    }
    Some(text.to_string())
}

/// يقرأ رؤوس PE وجدول الاستيراد من الذاكرة.
pub fn parse(bytes: &[u8]) -> Option<PeInfo> {
    if bytes.len() < 0x40 || bytes.get(0..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(bytes, 0x3C)? as usize;
    if bytes.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let coff = pe.checked_add(4)?;
    let machine = u16_at(bytes, coff)?;
    let sections_count = u16_at(bytes, coff + 2)? as usize;
    let opt_size = u16_at(bytes, coff + 16)? as usize;
    let opt = coff.checked_add(20)?;
    let is_64 = match u16_at(bytes, opt)? {
        0x20B => true,
        0x10B => false,
        _ => return None,
    };
    if sections_count == 0 || sections_count > 96 {
        return None;
    }

    // مجلدات البيانات (Data Directories): نريد الاستيراد (1) وCLR (14)
    let dirs_at = opt.checked_add(if is_64 { 112 } else { 96 })?;
    let dir_count = u32_at(bytes, dirs_at.checked_sub(4)?)? as usize;
    let (import_rva, import_size) = if dir_count > 1 {
        (u32_at(bytes, dirs_at + 8)?, u32_at(bytes, dirs_at + 12)?)
    } else {
        (0, 0)
    };
    let dotnet = if dir_count > 14 {
        let at = dirs_at + 14 * 8;
        u32_at(bytes, at)? != 0 && u32_at(bytes, at + 4)? != 0
    } else {
        false
    };

    // جدول المقاطع — نحتاجه لتحويل RVA إلى موضع في الملف
    let table = opt.checked_add(opt_size)?;
    let mut sections: Vec<(u32, u32, u32)> = Vec::with_capacity(sections_count);
    for i in 0..sections_count {
        let s = table.checked_add(i.checked_mul(40)?)?;
        let vsize = u32_at(bytes, s + 8)?;
        let vaddr = u32_at(bytes, s + 12)?;
        let raw_size = u32_at(bytes, s + 16)?;
        let raw_ptr = u32_at(bytes, s + 20)?;
        sections.push((vaddr, vsize.max(raw_size), raw_ptr));
    }

    let rva_to_off = |rva: u32| -> Option<usize> {
        for &(va, size, ptr) in &sections {
            if rva >= va && rva < va.checked_add(size)? {
                return (ptr as usize).checked_add((rva - va) as usize);
            }
        }
        None
    };

    let mut imports = Vec::new();
    if import_rva != 0 && import_size != 0 {
        let mut off = rva_to_off(import_rva)?;
        for _ in 0..MAX_IMPORTS {
            let original = u32_at(bytes, off)?;
            let name_rva = u32_at(bytes, off + 12)?;
            let first = u32_at(bytes, off + 16)?;
            // نهاية الجدول: وصف صفري كامل
            if original == 0 && name_rva == 0 && first == 0 {
                break;
            }
            if let Some(name_off) = rva_to_off(name_rva) {
                if let Some(name) = cstr(bytes, name_off, 260) {
                    imports.push(name.to_ascii_lowercase());
                }
            }
            off = off.checked_add(20)?;
        }
    }

    Some(PeInfo {
        machine,
        is_64,
        dotnet,
        imports,
    })
}

/// يفحص ملفاً على القرص (بدون تنفيذه).
pub fn probe_file(path: &Path) -> Option<PeInfo> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() < 0x40 {
        return None;
    }
    let take = meta.len().min(MAX_READ) as usize;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; take];
    file.read_exact(&mut buf).ok()?;
    parse(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// يبني ملف PE اصطناعي صالحاً بجدول استيراد بسيط.
    fn build_pe(imports: &[&str], is_64: bool) -> Vec<u8> {
        const RAW: usize = 0x400;
        const VA: u32 = 0x1000;
        let opt_size: usize = if is_64 { 0xF0 } else { 0xE0 };
        let mut b = vec![0u8; RAW + 0x400];
        b[0] = b'M';
        b[1] = b'Z';
        b[0x3C..0x40].copy_from_slice(&(0x80u32).to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        let machine: u16 = if is_64 { 0x8664 } else { 0x014C };
        b[coff..coff + 2].copy_from_slice(&machine.to_le_bytes());
        b[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes()); // عدد المقاطع
        b[coff + 16..coff + 18].copy_from_slice(&(opt_size as u16).to_le_bytes());
        let opt = coff + 20;
        let magic: u16 = if is_64 { 0x20B } else { 0x10B };
        b[opt..opt + 2].copy_from_slice(&magic.to_le_bytes());
        let dirs = opt + if is_64 { 112 } else { 96 };
        b[dirs - 4..dirs].copy_from_slice(&16u32.to_le_bytes());
        // دليل الاستيراد: RVA 0x1000، وحجمه حسب عدد المكتبات
        b[dirs + 8..dirs + 12].copy_from_slice(&VA.to_le_bytes());
        let size = (20 * (imports.len() + 1)) as u32;
        b[dirs + 12..dirs + 16].copy_from_slice(&size.to_le_bytes());
        // مقطع واحد
        let sec = opt + opt_size;
        b[sec + 8..sec + 12].copy_from_slice(&0x400u32.to_le_bytes());
        b[sec + 12..sec + 16].copy_from_slice(&VA.to_le_bytes());
        b[sec + 16..sec + 20].copy_from_slice(&0x400u32.to_le_bytes());
        b[sec + 20..sec + 24].copy_from_slice(&(RAW as u32).to_le_bytes());
        // أوصاف الاستيراد + الأسماء
        for (i, name) in imports.iter().enumerate() {
            let desc = RAW + i * 20;
            let name_off = RAW + 0x100 + i * 0x40;
            let name_rva = VA + (name_off - RAW) as u32;
            b[desc + 12..desc + 16].copy_from_slice(&name_rva.to_le_bytes());
            b[desc + 16..desc + 20].copy_from_slice(&1u32.to_le_bytes());
            b[name_off..name_off + name.len()].copy_from_slice(name.as_bytes());
        }
        b
    }

    #[test]
    fn reads_x64_d3d11_imports() {
        let bytes = build_pe(&["KERNEL32.dll", "d3d11.dll", "dxgi.dll"], true);
        let info = parse(&bytes).expect("PE صالح");
        assert!(info.is_64);
        assert_eq!(info.machine, 0x8664);
        assert_eq!(info.dx(), DxLevel::D3d11);
        assert!(info.imports.contains(&"d3d11.dll".to_string()));
        assert!(!info.dotnet);
    }

    #[test]
    fn reads_x86_d3d9_and_marks_arch() {
        let bytes = build_pe(&["d3d9.dll"], false);
        let info = parse(&bytes).expect("PE صالح");
        assert!(!info.is_64);
        assert_eq!(info.dx(), DxLevel::D3d9);
        assert!(info.summary().contains("x86"));
    }

    #[test]
    fn dx12_is_the_strongest_level() {
        let bytes = build_pe(&["d3d12.dll", "d3d11.dll"], true);
        assert_eq!(parse(&bytes).unwrap().dx(), DxLevel::D3d12);
    }

    #[test]
    fn dx12_has_an_explicit_blocker() {
        assert_eq!(DxLevel::D3d12.blocker(), Some("dx12"));
        assert_eq!(DxLevel::D3d9.blocker(), None);
        assert_eq!(DxLevel::D3d11.blocker(), None);
    }

    #[test]
    fn garbage_and_truncated_files_are_rejected() {
        assert!(parse(b"").is_none());
        assert!(parse(b"MZ").is_none());
        assert!(parse(&vec![0u8; 4096]).is_none());
        let mut broken = build_pe(&["d3d9.dll"], false);
        broken[0x80] = b'X'; // توقيع تالف
        assert!(parse(&broken).is_none());
    }
}
