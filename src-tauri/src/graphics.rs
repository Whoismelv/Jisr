// ============================================================
//  جسر — طبقة الرسومات (DXVK-macOS)
//  • DXVK يحوّل DirectX 10/11 → Vulkan → MoltenVK → Metal
//    (نفس المسار المعتمد في Whisky، وأسرع من WineD3D في أغلب الألعاب)
//  • نستخدم نسخة Gcenx الرسمية «builtin» لأنها مبنية لمجلدات Wine
//    (x86_64-windows / i386-windows) — تُحمَّل عبر WINEDLLPATH_PREPEND
//    بلا لمس ملفات القارورة، وإزالتها ترجّع WineD3D فوراً
//  • اختيار المستخدم: تلقائي (DXVK إن وُجد) / DXVK / WineD3D
//  • لا نلمس D3D9 أو DXGI: Gcenx يشيلهم عمداً على macOS
// ============================================================
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::engine::{self, ExtErr};

/// معرّف الأصل في كتالوج التنزيل (download.rs)
pub const ASSET_ID: &str = "dxvk-macos-d3d9-1.10.3";
/// معرّف الطبقة القديمة (بدون d3d9) — ننظّفها عند التثبيت الجديد
const OLD_ASSET_ID: &str = "dxvk-macos-1.10.3";
/// وضع الرسومات الافتراضي: DXVK إن كان منزّلاً، وإلا WineD3D
pub const MODE_AUTO: &str = "auto";
pub const MODE_DXVK: &str = "dxvk";
pub const MODE_WINED3D: &str = "wined3d";

fn graphics_dir() -> PathBuf {
    crate::data_dir().join("graphics")
}

/// مكان الطبقة النهائي — خارج engines حتى لا يختلط بمسح محرّك Wine
pub fn install_dir() -> PathBuf {
    graphics_dir().join(ASSET_ID)
}

fn staging_dir() -> PathBuf {
    graphics_dir().join(format!("staging-{ASSET_ID}"))
}

fn mode_path() -> PathBuf {
    graphics_dir().join("mode.txt")
}

/// DXVK جاهز؟ (d3d10core + d3d11 للنسختين 64 و32 بت)
pub fn is_installed() -> bool {
    let d = install_dir();
    ["x86_64-windows", "i386-windows"].iter().all(|arch| {
        ["d3d10core.dll", "d3d11.dll"]
            .iter()
            .all(|file| d.join(arch).join(file).is_file())
    })
}

/// DXVK فيه دعم **DirectX 9** (d3d9.dll)؟ بدونه، ألعاب D3D9 تسقط على
/// WineD3D ⇒ ترجمة GLSL ⇒ مُصرّف GLSL من Apple يرفضها
/// (`Pixel shader error … E5000: syntax error`) ⇒ شاشة سوداء.
pub fn d3d9_available() -> bool {
    let d = install_dir();
    ["x86_64-windows", "i386-windows"]
        .iter()
        .all(|arch| d.join(arch).join("d3d9.dll").is_file())
}

/// تحقّق من قيمة الوضع (نفس القيم في الواجهة والملف)
fn parse_mode(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        MODE_AUTO => Some(MODE_AUTO),
        MODE_DXVK => Some(MODE_DXVK),
        MODE_WINED3D => Some(MODE_WINED3D),
        _ => None,
    }
}

/// وضع المستخدم الحالي — «تلقائي» عند أي غياب/تلف (سلوك آمن)
pub fn mode() -> String {
    std::fs::read_to_string(mode_path())
        .ok()
        .and_then(|s| parse_mode(&s).map(String::from))
        .unwrap_or_else(|| MODE_AUTO.to_string())
}

/// هل نشغّل DXVK فعلاً؟
/// • wined3d = لا، مهما كان منزّلاً
/// • auto/dxvk = نعم فقط إذا الملفات موجودة (وإلا WineD3D بلا أخطاء)
pub fn dxvk_enabled() -> bool {
    mode() != MODE_WINED3D && is_installed()
}

/// الطبقة الفعلية للعرض في الواجهة/التقرير
pub fn active_layer() -> &'static str {
    if dxvk_enabled() { "DXVK" } else { "WineD3D" }
}

/// يضبط متغيّرات DXVK على أمر Wine. يرجع true لو فُعّل.
/// • WINEDLLPATH_PREPEND: يقدّم نسختنا على نسخة Wine المدمجة
/// • d3d10core,d3d11=b: نمنع أي d3d11.dll قديم داخل مجلد اللعبة
/// • DXVK_ASYNC: النسخة async تبني pipelines بالخلفية (أقل تقطيع)
/// • steam=true: نضيف `steamservice=d` — **ضروري** لأن ألعاب ستيم ترث بيئة
///   عميل ستيم نفسه، مو بيئة أمر `-applaunch` القصير.
/// • d3d9=b (إن وُجد): **مهم** — بدونه تسقط ألعاب DirectX 9 على WineD3D
///   (ترجمة GLSL يرفضها مُصرّف GLSL من Apple ⇒ شاشة سوداء). مع DXVK
///   يصير المسار: D3D9 → Vulkan → MoltenVK → Metal.
pub(crate) fn apply_env(cmd: &mut tokio::process::Command, steam: bool) -> bool {
    if !dxvk_enabled() {
        return false;
    }
    let mut overrides = String::from("winemenubuilder.exe=d");
    if steam {
        overrides.push_str(";steamservice=d");
    }
    overrides.push_str(";d3d10core,d3d11=b");
    if d3d9_available() {
        overrides.push_str(";d3d9=b");
    }
    cmd.env("WINEDLLPATH_PREPEND", install_dir())
        .env("DXVK_ASYNC", "1")
        // HUD صغير في زاوية اللعبة (اسم DXVK + الإصدار) — **دليل ميداني** إن
        // مسار DXVK شغّال فعلاً. يجاوب على سؤال: «هل اللعبة تسير على Vulkan؟»
        .env("DXVK_HUD", "devinfo")
        .env("WINEDLLOVERRIDES", overrides);
    true
}

/// تثبيت الطبقة: فك آمن في staging ثم نقل ذرّي للمكان النهائي.
/// يرجع (عدد الملفات، الحجم بعد الفك).
pub fn install(
    archive: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64, u64) + Send + Sync),
) -> Result<(u64, u64), ExtErr> {
    let staging = staging_dir();
    let _ = std::fs::remove_dir_all(&staging);

    let (files, bytes, _skipped) = engine::extract_tar_gz(archive, &staging, cancel, progress)?;

    // لازم بنية DXVK الحقيقية (نسختا 64 و32) — أي شي ثاني = رفض
    let root = find_root(&staging).ok_or_else(|| {
        ExtErr::Failed("بنية غير متوقعة داخل أرشيف DXVK — ما لقينا d3d11.dll".into())
    })?;

    let target = install_dir();
    if target.exists() {
        let _ = std::fs::remove_dir_all(&target);
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ExtErr::Failed(format!("تعذّر تجهيز مجلد الطبقة: {e}")))?;
    }
    std::fs::rename(&root, &target)
        .map_err(|e| ExtErr::Failed(format!("تعذّر تثبيت طبقة DXVK: {e}")))?;

    // الطبقة القديمة (بدون d3d9) ما نايدة — ننظّفها حتى ما تاخذ مساحة
    let old = graphics_dir().join(OLD_ASSET_ID);
    if old.exists() {
        let _ = std::fs::remove_dir_all(&old);
    }

    let _ = std::fs::remove_dir_all(&staging);
    Ok((files, bytes))
}

fn find_root(dir: &Path) -> Option<PathBuf> {
    fn has_layout(d: &Path) -> bool {
        ["x86_64-windows", "i386-windows"].iter().all(|arch| {
            ["d3d10core.dll", "d3d11.dll"]
                .iter()
                .all(|file| d.join(arch).join(file).is_file())
        })
    }
    fn walk(d: &Path, depth: usize) -> Option<PathBuf> {
        if has_layout(d) {
            return Some(d.to_path_buf());
        }
        if depth == 0 {
            return None;
        }
        let rd = std::fs::read_dir(d).ok()?;
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && let Some(found) = walk(&e.path(), depth - 1)
            {
                return Some(found);
            }
        }
        None
    }
    walk(dir, 4)
}

/// نظّف بقايا فك DXVK من جلسة سابقة (تشتغل عند بدء التطبيق)
pub fn cleanup_staging() {
    let Ok(rd) = std::fs::read_dir(graphics_dir()) else {
        return;
    };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with("staging-") {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

// ============================================================
//  أوامر Tauri
// ============================================================

/// الوضع الحالي: auto | dxvk | wined3d
#[tauri::command]
pub fn graphics_mode() -> String {
    mode()
}

/// تغيير الوضع — يُحفظ فوراً ويُطبَّق على التشغيل التالي
#[tauri::command]
pub fn graphics_set_mode(mode: String) -> Result<(), String> {
    let _trace = crate::debuglog::trace("graphics", "graphics_set_mode", format!("mode={mode}"));
    let value = parse_mode(&mode).ok_or_else(|| "وضع رسومات غير معروف".to_string())?;
    let path = mode_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("تعذّر تجهيز مجلد الإعدادات: {e}"))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, format!("{value}\n"))
        .map_err(|e| format!("تعذّر حفظ وضع الرسومات: {e}"))?;
    if std::fs::rename(&tmp, &path).is_err() {
        // ويندوز ما يستبدل موجوداً — نفس نمط حفظ الألعاب
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path).map_err(|e| format!("تعذّر إتمام حفظ وضع الرسومات: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_known_modes() {
        assert_eq!(parse_mode("auto"), Some(MODE_AUTO));
        assert_eq!(parse_mode(" DXVK \n"), Some(MODE_DXVK));
        assert_eq!(parse_mode("wined3d"), Some(MODE_WINED3D));
        assert_eq!(parse_mode("dxmt"), None);
        assert_eq!(parse_mode("../evil"), None);
        assert_eq!(parse_mode(""), None);
    }

    #[test]
    fn finds_dxvk_root_with_both_arches() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("jisr-dxvk-{}-{stamp}", std::process::id()));
        let root = base.join("dxvk-builtin");
        for arch in ["x86_64-windows", "i386-windows"] {
            std::fs::create_dir_all(root.join(arch)).unwrap();
            std::fs::write(root.join(arch).join("d3d10core.dll"), b"10").unwrap();
            std::fs::write(root.join(arch).join("d3d11.dll"), b"11").unwrap();
        }

        assert_eq!(find_root(&base), Some(root));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// تحقق يدوي من الأرشيف الحقيقي (2.8MB) — لا يعمل في CI بلا ملف:
    /// PowerShell: `$env:JISR_DXVK_ARCHIVE="C:\...\builtin.tar.gz"; cargo test real_dxvk -- --ignored`
    #[test]
    #[ignore]
    fn real_dxvk_archive_extracts_and_has_layout() {
        let Ok(path) = std::env::var("JISR_DXVK_ARCHIVE") else {
            return;
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("jisr-dxvk-real-{stamp}"));
        let dest = base.join("out");
        let cancel = AtomicBool::new(false);
        let (_files, _bytes, _skipped) =
            engine::extract_tar_gz(Path::new(&path), &dest, &cancel, &|_, _, _| {}).unwrap();
        assert!(find_root(&dest).is_some(), "بنية DXVK غير متوقعة");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// الأرشيف الحقيقي اللي نشحنه (DXVK + d3d9) — نفس الفحص اليدوي
    #[test]
    #[ignore]
    fn real_dxvk_d3d9_archive_has_d3d9() {
        let Ok(path) = std::env::var("JISR_DXVK_ARCHIVE") else {
            return;
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("jisr-dxvk-d3d9-{stamp}"));
        let dest = base.join("out");
        let cancel = AtomicBool::new(false);
        let (_files, _bytes, _skipped) =
            engine::extract_tar_gz(Path::new(&path), &dest, &cancel, &|_, _, _| {}).unwrap();
        let root = find_root(&dest).expect("بنية DXVK غير متوقعة");
        for arch in ["x86_64-windows", "i386-windows"] {
            assert!(
                root.join(arch).join("d3d9.dll").is_file(),
                "d3d9.dll ناقص في {arch} — بدونه ألعاب DirectX 9 ما تشتغل"
            );
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
