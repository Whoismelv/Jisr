//! Standalone game run pipeline.
//!
//! The Steam path has its own launcher (Steam starts the game, we only shape
//! the bottle). A game added from the library page is different: **we** are
//! the launcher, so we own the whole chain:
//!
//! 1. probe the executable (PE imports, architecture, .NET),
//! 2. plan the Direct3D layers from what the binary actually imports,
//! 3. turn the plan into DLL overrides the bottle can honour,
//! 4. launch with the right environment,
//! 5. after exit, read the crash report, classify it, pick the next
//!    combination, and relaunch automatically.
//!
//! Steps 1–4 are pure planning plus one registry pass; step 5 closes the loop
//! so a title that crashes on DXVK retries on WineD3D without the user
//! touching anything.

use std::path::Path;

use crate::combo::{Compiler, D3d9Layer, D3dx9Layer, GameProfile};
use crate::{auto, d3dx, graphics, pe};

/// What the machine can actually do right now.
///
/// Same inputs the Steam planner uses, so a title behaves identically no
/// matter which page launched it.
pub fn availability() -> auto::LayerAvailability {
    let i386_vulkan = crate::bottle::wine_bin()
        .map(|w| {
            std::path::Path::new(&w)
                .parent()
                .map(|p| p.join("../lib/wine/i386-unix/winevulkan.so").exists())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    auto::LayerAvailability {
        dxvk: graphics::is_installed(),
        dxvk_d3d9: graphics::d3d9_available(),
        d3dx9: d3dx::is_installed(),
        // DXMT/vkd3d are not shipped: the plan tells the user what is missing
        // instead of pretending the layer exists.
        dxmt: false,
        vkd3d: false,
        engine_has_i386_vulkan: i386_vulkan,
    }
}

/// Probe an executable. `None` means "not a readable PE" — the game still
/// launches, it just gets the unknown-title treatment.
pub fn probe(exe: &Path) -> Option<pe::PeInfo> {
    pe::probe_file(exe)
}

/// Turn a PE probe into the game-probe shape the planner consumes.
pub fn game_probe(info: Option<&pe::PeInfo>) -> crate::steam_library::GameProbe {
    let mut probe = crate::steam_library::GameProbe::default();
    let Some(info) = info else {
        return probe;
    };
    probe.dx = Some(info.dx().as_str().to_string());
    probe.is_64 = Some(info.is_64);
    if let Some(blocker) = info.dx().blocker() {
        probe.issues.push(blocker.to_string());
    }
    if !info.is_64 {
        probe.issues.push("x86".into());
    }
    if info.dx() == pe::DxLevel::None && info.dotnet {
        probe.issues.push("dotnet".into());
    }
    probe.needs_d3dx9 = info
        .imports
        .iter()
        .any(|i| i.starts_with("d3dx9_") || i.starts_with("d3dx10_"));
    probe.needs_d3dcompiler = info.imports.iter().any(|i| i.starts_with("d3dcompiler_"));
    probe
}

/// The layer plan for a game profile + probe.
pub fn plan(profile: &GameProfile, info: Option<&pe::PeInfo>) -> auto::LayerPlan {
    let probe = game_probe(info);
    let needs_d3dx9 = probe.needs_d3dx9;
    let needs_d3dcompiler = probe.needs_d3dcompiler;
    let facts = auto::GameFacts {
        probe,
        needs_d3dx9,
        needs_d3dcompiler,
    };
    let _ = profile;
    auto::plan_with_facts(&facts, &availability(), &graphics::mode())
}

/// A stable 32-bit key for a library game.
///
/// The profile store is keyed by `u32`, and standalone games have string ids.
/// The key only drives the per-game layer marker and the per-executable
/// registry key, so a hash is enough: two different ids colliding would just
/// share a cache marker, and the overrides themselves are written per
/// executable anyway.
pub fn appid_for(id: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in id.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// A profile for a game that has never run: the modern single combination,
/// with the D3D9 layer aligned to what the plan says is actually available.
pub fn initial_profile(info: Option<&pe::PeInfo>) -> GameProfile {
    let mut profile = GameProfile::default_for(0);
    let plan = plan(&profile, info);
    let has_dxvk9 = plan.layers.iter().any(|l| matches!(l, auto::Layer::Dxvk9));
    profile.d3d9 = if has_dxvk9 && graphics::d3d9_available() {
        D3d9Layer::Dxvk
    } else {
        D3d9Layer::WineD3D
    };
    // Native D3DX9 only when the files are really on disk; otherwise the
    // override would point at a DLL that does not exist and Wine silently
    // falls back while the profile claims otherwise.
    if d3dx::is_installed() {
        profile.d3dx9 = D3dx9Layer::Native;
        profile.compiler = Compiler::Native;
    } else {
        profile.d3dx9 = D3dx9Layer::Builtin;
        profile.compiler = Compiler::Builtin;
    }
    profile
}

/// Translate a profile into the registry overrides the bottle can honour.
///
/// Never says `native` for a file that is not on disk: Wine would fall back
/// to the builtin silently and the profile would be lying.
pub fn overrides(profile: &GameProfile, prefix: &Path) -> graphics::LayerOverrides {
    let dxvk_ready = graphics::is_installed() && graphics::d3d9_available();
    let d3dx_ready = d3dx::is_installed();
    let want_dxvk = profile.d3d9 == D3d9Layer::Dxvk;
    let want_native_d3dx = profile.d3dx9 == D3dx9Layer::Native;
    let guard_on = d3dx::guard_enabled(prefix);
    let want_native_compiler = guard_on || profile.compiler == Compiler::Native;
    graphics::LayerOverrides {
        d3d9: if want_dxvk && dxvk_ready {
            "native"
        } else {
            "builtin"
        },
        d3d10core: if want_dxvk && dxvk_ready {
            "native"
        } else {
            "builtin"
        },
        d3d11: if want_dxvk && dxvk_ready {
            "native"
        } else {
            "builtin"
        },
        d3dx9_43: if want_native_d3dx && d3dx_ready {
            "native"
        } else {
            "builtin"
        },
        d3dcompiler_43: if want_native_compiler && (d3dx_ready || guard_on) {
            "native"
        } else {
            "builtin"
        },
    }
}

/// Apply a profile to the bottle before launch: DLL overrides + renderer.
pub async fn apply(
    wine: &Path,
    prefix: &Path,
    profile: &GameProfile,
    exe_dos: Option<&str>,
) -> Result<(), String> {
    let ov = overrides(profile, prefix);
    graphics::ensure_layer(wine, prefix, &ov, profile.appid, exe_dos).await?;
    let renderer = match profile.renderer {
        crate::combo::Renderer::Vulkan => "vulkan",
        crate::combo::Renderer::Gl => "gl",
    };
    let _ = graphics::ensure_renderer(wine, prefix, renderer).await;
    Ok(())
}

/// Environment that goes with a profile.
pub fn env(profile: &GameProfile) -> Vec<(String, String)> {
    let mut out = vec![
        ("WINEMSYNC".to_string(), "1".to_string()),
        (
            "WINEDLLOVERRIDES".to_string(),
            format!(
                "d3d9={0};d3d10core={0};d3d11={0};d3dx9_43={1};d3dcompiler_43={2}",
                if profile.d3d9 == D3d9Layer::Dxvk {
                    "n"
                } else {
                    "b"
                },
                if profile.d3dx9 == D3dx9Layer::Native {
                    "n"
                } else {
                    "b"
                },
                if profile.compiler == Compiler::Native {
                    "n"
                } else {
                    "b"
                },
            ),
        ),
    ];
    if profile.overlay {
        out.push(("STEAM_COMPAT_OVERLAY".to_string(), "1".to_string()));
    }
    out
}

/// Pull a complete Wine crash report out of a game log.
///
/// A report is only "complete" once the backtrace exists: winedbg writes the
/// module table progressively, and classifying a half-written report is how
/// you end up blaming the wrong DLL.
pub fn extract_crash_report(text: &str) -> Option<String> {
    let idx = text.rfind("Unhandled exception:")?;
    let rest = &text[idx..];
    if !rest.contains("Backtrace:") {
        return None;
    }
    let end = rest
        .find("\nSystem information:")
        .map(|i| (i + 600).min(rest.len()))
        .unwrap_or_else(|| rest.len().min(24_000));
    Some(rest[..end].to_string())
}

/// Normalise a module/file name for comparison: no path, no extension, lower.
#[allow(dead_code)] // يستهلكه مسار ستيم المشترك لاحقاً + الاختبارات
pub fn normalize_module_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = base.to_ascii_lowercase();
    let lower = lower.strip_suffix(".exe").unwrap_or(&lower);
    let lower = lower.strip_suffix(".dll").unwrap_or(lower);
    lower.to_string()
}

/// Did this crash report come from this executable?
#[allow(dead_code)] // مسار ستيم المشترك — كل سجل عنده يحتاج الفلترة
pub fn report_mentions_exe(report: &str, exe: &str) -> bool {
    let Some(parsed) = crate::triage::parse_report(report) else {
        return false;
    };
    let wanted = normalize_module_name(exe);
    parsed
        .modules
        .iter()
        .any(|m| normalize_module_name(&m.name) == wanted)
}

/// Next combination after a classified crash, or `None` when there is nothing
/// left to try (a missing file needs the user, not another launch).
pub fn next_after_crash(profile: &GameProfile, action: &str) -> Option<GameProfile> {
    profile.next_candidate(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_profile_never_claims_a_layer_that_is_missing() {
        let profile = initial_profile(None);
        if !graphics::d3d9_available() {
            assert_eq!(profile.d3d9, D3d9Layer::WineD3D);
        }
        if !d3dx::is_installed() {
            assert_eq!(profile.d3dx9, D3dx9Layer::Builtin);
            assert_eq!(profile.compiler, Compiler::Builtin);
        }
    }

    #[test]
    fn overrides_are_builtin_when_the_files_are_absent() {
        let dir = std::env::temp_dir().join(format!("jisr-runner-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let profile = initial_profile(None);
        let ov = overrides(&profile, &dir);
        if !graphics::d3d9_available() {
            assert_eq!(ov.d3d9, "builtin");
        }
        if !d3dx::is_installed() {
            assert_eq!(ov.d3dx9_43, "builtin");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_names_every_layer_exactly_once() {
        let profile = initial_profile(None);
        let env = env(&profile);
        let overrides = env
            .iter()
            .find(|(k, _)| k == "WINEDLLOVERRIDES")
            .map(|(_, v)| v.as_str())
            .expect("overrides present");
        for name in [
            "d3d9=",
            "d3d10core=",
            "d3d11=",
            "d3dx9_43=",
            "d3dcompiler_43=",
        ] {
            assert_eq!(
                overrides.matches(name).count(),
                1,
                "{name} must appear exactly once"
            );
        }
    }

    #[test]
    fn incomplete_reports_are_rejected() {
        let half = "Unhandled exception: page fault on read access to 0x0";
        assert!(extract_crash_report(half).is_none());
        let complete = "Unhandled exception: page fault\nBacktrace:\n=>0 0x1 in game (+0x1)\nSystem information:\nos=mac";
        let report = extract_crash_report(complete).expect("complete report");
        assert!(report.contains("Backtrace:"));
    }

    #[test]
    fn module_matching_ignores_path_and_extension() {
        assert_eq!(normalize_module_name("C:\\\\Games\\\\GTA5.EXE"), "gta5");
        assert_eq!(normalize_module_name("d3d9.dll"), "d3d9");
    }

    #[test]
    fn appid_is_stable_and_ids_differ() {
        assert_eq!(appid_for("g-1717171717171"), appid_for("g-1717171717171"));
        assert_ne!(appid_for("g-1717171717171"), appid_for("g-1717171717172"));
    }

    /// The retry ladder must actually change something: a classification that
    /// maps back to the same combination is a loop that spins in place.
    #[test]
    fn crash_verdicts_move_the_profile_forward() {
        let mut base = initial_profile(None);
        base.d3d9 = D3d9Layer::WineD3D;
        let next =
            next_after_crash(&base, "enable-dxvk").expect("enable-dxvk must produce a candidate");
        assert_eq!(next.d3d9, D3d9Layer::Dxvk);
        assert_eq!(next.attempts, base.attempts + 1);

        // Already on the requested layer ⇒ nothing to try, no false retry.
        assert!(next_after_crash(&next, "enable-dxvk").is_none());
    }
}
