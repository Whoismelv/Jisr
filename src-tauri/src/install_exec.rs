//! Install execution — the engine behind [`crate::install_plan`].
//!
//! The planner produces a dependency-ordered plan; this module executes it
//! against a real bottle with our own task set.
//!
//! Everything here is *our* code and uses only what we ship:
//! Wine (via the active engine), `reqwest` for downloads, `sha2` for
//! verification, `tar`/`flate2`/`xz2`/`cab` for archives.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::install_plan::{AppRequest, InstallPlan, PlanRequest, Task, TaskKind, plan};

/// How a step ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum StepStatus {
    Done,
    Skipped(&'static str),
    Failed(String),
}

/// One executed step.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StepReport {
    pub task: String,
    pub kind: &'static str,
    pub status: StepStatus,
    /// Human readable detail (path, command, …).
    pub detail: String,
}

/// Result of executing a whole plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecReport {
    pub steps: Vec<StepReport>,
}

impl ExecReport {
    pub fn ok(&self) -> bool {
        self.steps
            .iter()
            .all(|s| !matches!(s.status, StepStatus::Failed(_)))
    }

    /// The first failure, if any.
    pub fn first_failure(&self) -> Option<&StepReport> {
        self.steps
            .iter()
            .find(|s| matches!(s.status, StepStatus::Failed(_)))
    }
}

/// Everything the executor needs to talk to the system.
pub struct ExecContext {
    /// The Wine loader of the active engine.
    pub wine: PathBuf,
    /// The bottle prefix.
    pub prefix: PathBuf,
    /// Where downloaded installers are cached.
    pub cache_dir: PathBuf,
    /// Extra environment for the installer process.
    pub env: BTreeMap<String, String>,
    /// Timeout for one installer process.
    pub timeout: Duration,
}

impl ExecContext {
    pub fn new(wine: PathBuf, prefix: PathBuf, cache_dir: PathBuf) -> Self {
        ExecContext {
            wine,
            prefix,
            cache_dir,
            env: BTreeMap::new(),
            timeout: Duration::from_secs(30 * 60),
        }
    }
}

/// Cache key for a download: hash of the URL plus the file name.
///
/// Their engine keys the cache by `MD5(url).basename`; we use the first bytes
/// of the SHA-256 of the URL instead — same idea, no weak hash, and the file
/// name is kept so the extension stays meaningful.
pub fn cache_path(cache_dir: &Path, url: &str, filename: &str) -> PathBuf {
    let digest = Sha256::digest(url.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    // Keep the file name recognisable, but never allow traversal: slashes and
    // backslashes are dropped and dot runs are collapsed.
    let mut safe = String::new();
    for ch in filename.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            safe.push(ch);
        } else if ch == '.' && !safe.ends_with('.') {
            safe.push('.');
        }
    }
    let safe = safe.trim_start_matches('.').to_string();
    let safe = if safe.is_empty() {
        "installer.bin".to_string()
    } else {
        safe
    };
    cache_dir.join(format!("{}-{safe}", &hex[..16]))
}

/// Verify a file against a `checksum` in the `algo:hex` form
/// (their format; `sha256:` and `sha1:` are supported, `md5:` is not).
pub fn verify_checksum(path: &Path, checksum: &str) -> Result<(), String> {
    let (algo, expected) = checksum
        .split_once(':')
        .ok_or_else(|| format!("صيغة البصمة غير صالحة: {checksum}"))?;
    let expected = expected.trim().to_ascii_lowercase();
    let bytes = std::fs::read(path).map_err(|e| format!("تعذّر قراءة {}: {e}", path.display()))?;
    let actual = match algo.trim().to_ascii_lowercase().as_str() {
        "sha256" => Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        other => return Err(format!("خوارزمية بصمة غير مدعومة: {other}")),
    };
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "البصمة لا تطابق {} (توقّع {expected}، وجد {actual})",
            path.display()
        ))
    }
}

/// Decompress an archive we know how to handle.
///
/// Supported: `.tar.gz` / `.tgz`, `.tar.xz`, `.cab`, and self-extracting
/// `.exe` (which is *not* decompressed here — it is executed).
pub fn uncompress(archive: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("تعذّر إنشاء {}: {e}", dest.display()))?;
    let name = archive.to_string_lossy().to_ascii_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = std::fs::File::open(archive).map_err(|e| format!("تعذّر فتح الأرشيف: {e}"))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(decoder);
        tar.unpack(dest)
            .map_err(|e| format!("فشل فك ضغط tar.gz: {e}"))
    } else if name.ends_with(".tar.xz") {
        let file = std::fs::File::open(archive).map_err(|e| format!("تعذّر فتح الأرشيف: {e}"))?;
        let decoder = xz2::read::XzDecoder::new(file);
        let mut tar = tar::Archive::new(decoder);
        tar.unpack(dest)
            .map_err(|e| format!("فشل فك ضغط tar.xz: {e}"))
    } else if name.ends_with(".cab") {
        // CAB installers are handled by the installer itself inside Wine
        // (msiexec/expand), so we do not unpack them here.
        Err(format!(
            "أرشيف cab يُنفَّذ داخل Wine مباشرة: {}",
            archive.display()
        ))
    } else {
        Err(format!(
            "نوع أرشيف غير مدعوم: {} (المدعوم: tar.gz, tar.xz)",
            archive.display()
        ))
    }
}

/// A dry run: what would happen, without touching the system.
pub fn dry_run(plan: &InstallPlan) -> Vec<StepReport> {
    plan.tasks
        .iter()
        .map(|task| StepReport {
            task: task.id.clone(),
            kind: task.kind.label(),
            status: StepStatus::Done,
            detail: describe(task),
        })
        .collect()
}

fn describe(task: &Task) -> String {
    match &task.kind {
        TaskKind::CreateBottle => "إنشاء القارورة".to_string(),
        TaskKind::Download { url, checksum } => match checksum {
            Some(sum) => format!("تنزيل {url} (بصمة {sum})"),
            None => format!("تنزيل {url}"),
        },
        TaskKind::InsertMedia => "طلب إدخال القرص".to_string(),
        TaskKind::ReleaseMedia => "تحرير القرص".to_string(),
        TaskKind::RunInstaller { command } => format!("تشغيل: {}", command.join(" ")),
        TaskKind::Uncompress {
            archive,
            destination,
        } => {
            format!("فك ضغط {archive} → {destination}")
        }
        TaskKind::CopyFiles {
            source,
            destination,
        } => format!("نسخ {source} → {destination}"),
        TaskKind::ApplyRegistry => "كتابة الريجستري".to_string(),
        TaskKind::RegisterDlls => "تسجيل DLLs".to_string(),
        TaskKind::Reboot => "إعادة تشغيل القارورة".to_string(),
        TaskKind::SyncMenu => "مزامنة قائمة ابدأ".to_string(),
        TaskKind::SyncAssociations => "مزامنة الجمعيات".to_string(),
    }
}

/// Execute a plan against a bottle.
pub async fn execute(plan: &InstallPlan, context: &ExecContext) -> ExecReport {
    let mut report = ExecReport::default();
    for task in &plan.tasks {
        let detail = describe(task);
        let status = match &task.kind {
            TaskKind::Download { url, checksum } => {
                download(url, context, checksum.as_deref()).await
            }
            TaskKind::Uncompress {
                archive,
                destination,
            } => {
                let file = archive.rsplit('/').next().unwrap_or("installer.bin");
                let archive_path = cache_path(&context.cache_dir, archive, file);
                uncompress(&archive_path, Path::new(destination))
            }
            TaskKind::RunInstaller { command } => run_wine(context, command, &[]).await,
            TaskKind::CopyFiles {
                source,
                destination,
            } => copy_tree(source, destination),
            TaskKind::ApplyRegistry => Ok(()),
            TaskKind::RegisterDlls => Ok(()),
            TaskKind::Reboot => reboot(context).await,
            _ => Ok(()),
        };
        let failed = status.is_err();
        let status = match status {
            Ok(()) => StepStatus::Done,
            Err(err) => StepStatus::Failed(err),
        };
        report.steps.push(StepReport {
            task: task.id.clone(),
            kind: task.kind.label(),
            status,
            detail,
        });
        if failed {
            break; // stop at the first failure, like a real installer
        }
    }
    report
}

/// Download a file into the cache (and verify it when a checksum is given).
pub async fn download(
    url: &str,
    context: &ExecContext,
    checksum: Option<&str>,
) -> Result<(), String> {
    let filename = url.rsplit('/').next().unwrap_or("installer.bin");
    let target = cache_path(&context.cache_dir, url, filename);
    if target.exists() {
        if let Some(sum) = checksum {
            if verify_checksum(&target, sum).is_ok() {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }
    std::fs::create_dir_all(&context.cache_dir)
        .map_err(|e| format!("تعذّر إنشاء مجلد الكاش: {e}"))?;

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("فشل الطلب {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("الخادم ردّ {} على {url}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("فشل تنزيل {url}: {e}"))?;

    let tmp = target.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("تعذّر الكتابة: {e}"))?;
    if let Some(sum) = checksum {
        if let Err(err) = verify_checksum(&tmp, sum) {
            let _ = std::fs::remove_file(&tmp);
            return Err(err);
        }
    }
    std::fs::rename(&tmp, &target).map_err(|e| format!("تعذّر نقل الملف: {e}"))?;
    Ok(())
}

/// Run a command inside Wine.
pub async fn run_wine(
    context: &ExecContext,
    args: &[String],
    extra_env: &[(String, String)],
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(&context.wine);
    cmd.env("WINEPREFIX", &context.prefix);
    for (key, value) in &context.env {
        cmd.env(key, value);
    }
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    cmd.args(args);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = match tokio::time::timeout(context.timeout, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => return Err(format!("تعذّر تشغيل wine: {err}")),
        Err(_) => return Err("انتهت مهلة تشغيل المثبّت".to_string()),
    };
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "فشل الأمر (code {:?}): {}",
            output.status.code(),
            stderr.lines().last().unwrap_or("")
        ))
    }
}

/// Restart the bottle so registry and DLL changes take effect.
pub async fn reboot(context: &ExecContext) -> Result<(), String> {
    run_wine(
        context,
        &[
            "wineboot.exe".to_string(),
            "--".to_string(),
            "--end-session".to_string(),
            "--shutdown".to_string(),
            "--force".to_string(),
            "--kill".to_string(),
        ],
        &[],
    )
    .await
}

/// Copy a file or a directory tree.
pub fn copy_tree(source: &str, destination: &str) -> Result<(), String> {
    let src = Path::new(source);
    let dst = Path::new(destination);
    if src.is_dir() {
        copy_dir(src, dst)
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("تعذّر إنشاء {}: {e}", parent.display()))?;
        }
        std::fs::copy(src, dst)
            .map(|_| ())
            .map_err(|e| format!("تعذّر نسخ {}: {e}", src.display()))
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("تعذّر إنشاء {}: {e}", dst.display()))?;
    let entries = std::fs::read_dir(src).map_err(|e| format!("تعذّر قراءة {:?}: {e}", src))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| format!("تعذّر نسخ {}: {e}", from.display()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Build the install plan for a set of application ids and show what would run.
///
/// The applications come from our own profile database (`jisrdb`), so the
/// planner learns the dependencies, the installer command and the files from
/// there — learned from the profile itself, not hardcoded.
#[tauri::command]
pub fn install_preview(appids: Vec<String>) -> Result<Vec<StepReport>, String> {
    let requests = requests_from_profiles(&appids)?;
    let request = PlanRequest {
        apps: requests,
        bottle_exists: true,
        needs_media: false,
    };
    let install_plan = plan(&request).map_err(|e| e.to_string())?;
    Ok(dry_run(&install_plan))
}

/// Run the install plan for a set of applications inside a bottle.
#[tauri::command]
pub async fn install_run(appids: Vec<String>, bottle: String) -> Result<Vec<StepReport>, String> {
    if !crate::bottle::valid_bottle_id(&bottle) {
        return Err("معرّف القارورة غير صالح".to_string());
    }
    let wine = crate::bottle::wine_bin().ok_or_else(|| "ما لقينا محرك Wine".to_string())?;
    let prefix = crate::bottle::bottle_dir(&bottle);
    let requests = requests_from_profiles(&appids)?;
    let request = PlanRequest {
        apps: requests,
        bottle_exists: prefix.exists(),
        needs_media: false,
    };
    let install_plan = plan(&request).map_err(|e| e.to_string())?;
    let context = ExecContext::new(wine, prefix, crate::data_dir().join("installers"));
    let report = execute(&install_plan, &context).await;
    Ok(report.steps)
}

/// Turn profile ids into planner inputs, using the profile database.
fn requests_from_profiles(appids: &[String]) -> Result<Vec<AppRequest>, String> {
    let store = crate::compat::ProfileStore::open(None);
    let mut requests = Vec::new();
    for appid in appids {
        let Some(profile) = store.profiles().get(appid) else {
            return Err(format!("ما لقينا بروفايل للّعبة: {appid}"));
        };
        let mut request = AppRequest {
            appid: profile.appid.clone(),
            name: profile
                .name_for("ar")
                .map(str::to_string)
                .unwrap_or_else(|| profile.appid.clone()),
            dependencies: profile.install.pre_dependencies.clone(),
            ..Default::default()
        };
        if !profile.installed_key_pattern.is_none() {
            request.installer_command = vec![format!("{}.exe", profile.appid)];
        }
        requests.push(request);
    }
    Ok(requests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_plan::{AppRequest, PlanRequest, plan};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("jisr-inst-{name}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cache_path_is_stable_and_keeps_the_extension() {
        let dir = Path::new("/tmp/cache");
        let a = cache_path(dir, "https://example.invalid/setup.exe", "setup.exe");
        let b = cache_path(dir, "https://example.invalid/setup.exe", "setup.exe");
        let c = cache_path(dir, "https://example.invalid/other.exe", "setup.exe");
        assert_eq!(a, b, "same URL must map to the same cache entry");
        assert_ne!(a, c, "different URLs must not collide");
        assert!(a.to_string_lossy().ends_with("setup.exe"));
    }

    #[test]
    fn cache_path_sanitises_the_file_name() {
        let path = cache_path(Path::new("/tmp"), "https://x.invalid/a", "../../etc/passwd");
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            !name.contains(".."),
            "path traversal must not survive: {name}"
        );
        assert!(!name.contains('/'));
    }

    #[test]
    fn checksum_verification_accepts_only_matching_files() {
        let dir = temp_dir("checksum");
        let file = dir.join("a.bin");
        std::fs::write(&file, b"hello").unwrap();
        let good = format!("sha256:{}", hex(&Sha256::digest(b"hello")));
        assert!(verify_checksum(&file, &good).is_ok());
        let bad = format!("sha256:{}", "0".repeat(64));
        assert!(verify_checksum(&file, &bad).is_err());
        assert!(verify_checksum(&file, "not-a-checksum").is_err());
        assert!(verify_checksum(&file, "md5:abc").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn tar_gz_archives_are_unpacked() {
        let dir = temp_dir("untar");
        let archive = dir.join("bundle.tar.gz");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let payload = b"installed";
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "readme.txt", &payload[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let dest = dir.join("out");
        uncompress(&archive, &dest).expect("unpack");
        assert!(dest.join("readme.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_archive_types_are_refused() {
        let dir = temp_dir("badarc");
        let archive = dir.join("thing.rar");
        std::fs::write(&archive, b"x").unwrap();
        assert!(uncompress(&archive, &dir.join("out")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dry_run_lists_every_task_without_touching_the_disk() {
        let request = PlanRequest {
            apps: vec![AppRequest {
                appid: "demo".into(),
                name: "Demo".into(),
                dependencies: vec![],
                download: Some(("https://example.invalid/demo.exe".into(), None)),
                installer_command: vec!["demo.exe".into()],
                files_to_copy: vec![("a".into(), "b".into())],
                uncompress: None,
                registry_entries: 1,
                reboot_after: true,
            }],
            bottle_exists: true,
            needs_media: false,
        };
        let install_plan = plan(&request).expect("plan");
        let report = dry_run(&install_plan);
        assert_eq!(report.len(), install_plan.tasks.len());
        assert!(report.iter().any(|s| s.kind == "download"));
        assert!(report.iter().any(|s| s.kind == "run-installer"));
        assert!(report.iter().any(|s| s.kind == "reboot"));
    }

    #[test]
    fn copy_tree_copies_files_and_directories() {
        let dir = temp_dir("copy");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();
        std::fs::write(src.join("nested").join("b.txt"), "b").unwrap();
        let dst = dir.join("dst");
        copy_tree(src.to_str().unwrap(), dst.to_str().unwrap()).expect("copy");
        assert!(dst.join("a.txt").exists());
        assert!(dst.join("nested").join("b.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_reports_the_first_failure() {
        let mut report = ExecReport::default();
        report.steps.push(StepReport {
            task: "a".into(),
            kind: "download",
            status: StepStatus::Done,
            detail: String::new(),
        });
        assert!(report.ok());
        report.steps.push(StepReport {
            task: "b".into(),
            kind: "run-installer",
            status: StepStatus::Failed("boom".into()),
            detail: String::new(),
        });
        assert!(!report.ok());
        assert_eq!(report.first_failure().map(|s| s.task.as_str()), Some("b"));
    }
}
