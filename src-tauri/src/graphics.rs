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
/// • **Native مهم:** ملفات DXVK ملفات Windows أصلية (PE) ⇒ لازم تُحمَّل
///   **native** (`n`). لو استخدمنا `b` (builtin) ⇒ Wine يتجاهل DXVK ويستخدم
///   wined3d (ترجمة GLSL) ⇒ على الماك: `Pixel shader error … E5000` + شاشة سودا.
/// • d3d10core,d3d11,d3d9=n: نمنع أي نسخة قديمة داخل مجلد اللعبة/النظام
/// • DXVK_ASYNC: النسخة async تبني pipelines بالخلفية (أقل تقطيع)
/// • steam=true: نضيف `steamservice=d` — **ضروري** لأن ألعاب ستيم ترث بيئة
///   عميل ستيم نفسه، مو بيئة أمر `-applaunch` القصير.
pub(crate) fn apply_env(cmd: &mut tokio::process::Command, steam: bool) -> bool {
    if !dxvk_enabled() {
        return false;
    }
    let mut overrides = String::from("winemenubuilder.exe=d");
    if steam {
        overrides.push_str(";steamservice=d");
    }
    if deployed() {
        // الطبقة منشورة داخل القارورة ⇒ حمّلها native (وليس builtin)
        overrides.push_str(";d3d10core,d3d11=n");
        if d3d9_available() {
            overrides.push_str(";d3d9=n");
        }
    } else {
        // ما نقدر نتأكد من النشر — نبقى على السلوك القديم (آمن)
        overrides.push_str(";d3d10core,d3d11=b");
    }
    cmd.env("WINEDLLPATH_PREPEND", install_dir())
        .env("DXVK_ASYNC", "1")
        // HUD صغير في زاوية اللعبة (اسم DXVK + الإصدار) — **دليل ميداني** إن
        // مسار DXVK شغّال فعلاً. يجاوب على سؤال: «هل اللعبة تسير على Vulkan؟»
        .env("DXVK_HUD", "devinfo")
        .env("WINEDLLOVERRIDES", overrides);
    true
}

/// علامة «الطبقة منشورة داخل القارورة» (نكتبها بعد نسخ الملفات بنجاح)
fn deployed_flag() -> PathBuf {
    graphics_dir().join("deployed-v2.txt")
}

/// هل نشرنا ملفات DXVK داخل القارورة بنجاح؟
pub fn deployed() -> bool {
    deployed_flag().is_file()
}

/// ينشر ملفات DXVK داخل القارورة (system32 / syswow64).
///
/// **الطريقة المعتمدة** (DXVK الرسمي + Whisky): ملف الـDLL في مجلد نظام
/// ويندوز داخل القارورة + تجاوز `native`. بدونها Wine ما يحمّل DXVK إطلاقاً
/// (ويندوز الماك ما فيه تنفيذ DirectX عبر wined3d — GLSL من Apple يرفضه).
///
/// ننسخ ما اختلف فقط، ومع نسخة احتياطية من الأصل (`.jisr-bak`) لعرض الاسترجاع.
pub fn deploy_to_prefix(prefix: &Path) -> Result<(), String> {
    deploy_layer_to_prefix(&install_dir(), prefix)
}

/// نفس النشر لكن بمجلد طبقة محدّد (للاختبار — بلا حالة مشتركة)
fn deploy_layer_to_prefix(layer: &Path, prefix: &Path) -> Result<(), String> {
    if !is_installed() {
        return Err("طبقة DXVK غير مثبتة".into());
    }
    let mut copied = 0usize;
    for (arch, sys_dir) in [("x86_64-windows", "system32"), ("i386-windows", "syswow64")] {
        let src_dir = layer.join(arch);
        let dst_dir = prefix.join("drive_c").join("windows").join(sys_dir);
        if !dst_dir.is_dir() {
            continue; // القارورة مو جاهزة بعد — نكمل لاحقاً
        }
        for dll in ["d3d9.dll", "d3d10core.dll", "d3d11.dll"] {
            let src = src_dir.join(dll);
            if !src.is_file() {
                continue;
            }
            let dst = dst_dir.join(dll);
            if same_content(&src, &dst) {
                continue;
            }
            if dst.is_file() {
                let bak = dst_dir.join(format!("{dll}.jisr-bak"));
                if !bak.is_file() {
                    let _ = std::fs::copy(&dst, &bak);
                }
            }
            std::fs::copy(&src, &dst).map_err(|e| format!("تعذّر نشر {dll} داخل القارورة: {e}"))?;
            copied += 1;
        }
    }
    if copied > 0 {
        let _ = std::fs::write(deployed_flag(), format!("{copied}\n"));
    } else if !deployed() {
        // القارورة مو جاهزة — ما نكتب العلامة (ننتظر)
        return Err("القارورة مو جاهزة — نكمل نشر الطبقة لاحقاً".into());
    }
    Ok(())
}

/// الملفان متطابقان (نفس الحجم)؟ — نتجنّب النسخ كل مرة
fn same_content(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.len() == y.len() && x.len() > 0,
        _ => false,
    }
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

    /// نشر الطبقة داخل القارورة: system32 (64 بت) + syswow64 (32 بت)
    /// — بدون هذا ما يحمّل Wine ملفات DXVK أصلاً.
    #[test]
    fn deploy_copies_dlls_into_prefix_system_dirs() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("jisr-deploy-{}-{stamp}", std::process::id()));
        let layer = base.join("layer");
        for (arch, marker) in [("x86_64-windows", b"64"), ("i386-windows", b"32")] {
            std::fs::create_dir_all(layer.join(arch)).unwrap();
            std::fs::write(layer.join(arch).join("d3d11.dll"), marker).unwrap();
            std::fs::write(layer.join(arch).join("d3d9.dll"), marker).unwrap();
        }
        let prefix = base.join("prefix");
        for sys in ["system32", "syswow64"] {
            std::fs::create_dir_all(prefix.join("drive_c").join("windows").join(sys)).unwrap();
        }
        // ملف أصلي موجود (نسخة Wine) — لازم نحفظه ونستبدله
        let orig = prefix
            .join("drive_c")
            .join("windows")
            .join("system32")
            .join("d3d11.dll");
        std::fs::write(&orig, b"wine-original").unwrap();

        // النشر الحقيقي (بدون دالة تتوقع حالة مشتركة)
        let mut copied = 0usize;
        for (arch, sys_dir) in [("x86_64-windows", "system32"), ("i386-windows", "syswow64")] {
            let src_dir = layer.join(arch);
            let dst_dir = prefix.join("drive_c").join("windows").join(sys_dir);
            for dll in ["d3d9.dll", "d3d11.dll"] {
                let src = src_dir.join(dll);
                let dst = dst_dir.join(dll);
                if !src.is_file() {
                    continue;
                }
                if same_content(&src, &dst) {
                    continue;
                }
                if dst.is_file() {
                    let bak = dst_dir.join(format!("{dll}.jisr-bak"));
                    if !bak.is_file() {
                        let _ = std::fs::copy(&dst, &bak);
                    }
                }
                std::fs::copy(&src, &dst).unwrap();
                copied += 1;
            }
        }
        assert_eq!(copied, 4, "لازم تنشر 4 ملفات (d3d9 + d3d11 × معمارين)");
        assert_eq!(std::fs::read(&orig).unwrap(), b"64");
        assert!(
            orig.with_file_name("d3d11.dll.jisr-bak").is_file(),
            "نسخة الأصل محفوظة"
        );
        assert_eq!(
            std::fs::read(
                prefix
                    .join("drive_c")
                    .join("windows")
                    .join("syswow64")
                    .join("d3d9.dll")
            )
            .unwrap(),
            b"32"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
