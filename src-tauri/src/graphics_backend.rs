//! Graphics backend selection.
//!
//! A bottle stores its graphics backend in its own configuration and applies
//! it in two places:
//!
//! 1. the bottle registry (`Software\Wine\Direct3D\renderer`, DllOverrides),
//! 2. the process environment (`JISR_GRAPHICS_BACKEND`, `JISR_ACTIVE_GRAPHICS_BACKEND`),
//!
//! and [`crate::compat_rules`] can override the choice **per application** from
//! a rule database. The design:
//!
//! * the backend choice lives in the bottle's own `jisr.conf` (INI style),
//! * [`apply`] writes the registry bits that actually change Wine's behaviour,
//! * [`env`] produces the environment for the launch command,
//! * [`crate::compat_rules`] can override the choice per game.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The graphics stack a bottle should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsBackend {
    /// Let the rules engine decide, falling back to our default.
    Auto,
    /// DXVK (D3D9/10/11 → Vulkan → MoltenVK → Metal).
    Dxvk,
    /// Wine's own D3D → OpenGL (slow) or Vulkan, depending on the renderer.
    WineD3d,
    /// DXMT (D3D10/11 → Metal directly, CodeWeavers' fork / 3Shain's DXMT).
    Dxmt,
    /// Apple's D3DMetal (from the Game Porting Toolkit the user installs).
    D3DMetal,
}

impl GraphicsBackend {
    /// Stable name used in the configuration file and in reports.
    pub fn as_str(self) -> &'static str {
        match self {
            GraphicsBackend::Auto => "auto",
            GraphicsBackend::Dxvk => "dxvk",
            GraphicsBackend::WineD3d => "wined3d",
            GraphicsBackend::Dxmt => "dxmt",
            GraphicsBackend::D3DMetal => "d3dmetal",
        }
    }

    /// Parse a stored value. Unknown values are rejected (like their
    /// `parse_mode`, which refuses anything it does not know).
    pub fn parse(raw: &str) -> Option<GraphicsBackend> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(GraphicsBackend::Auto),
            "dxvk" => Some(GraphicsBackend::Dxvk),
            "wined3d" | "wine" => Some(GraphicsBackend::WineD3d),
            "dxmt" => Some(GraphicsBackend::Dxmt),
            "d3dmetal" => Some(GraphicsBackend::D3DMetal),
            _ => None,
        }
    }

    /// All backends, in menu order.
    pub fn all() -> &'static [GraphicsBackend] {
        &[
            GraphicsBackend::Auto,
            GraphicsBackend::Dxvk,
            GraphicsBackend::WineD3d,
            GraphicsBackend::Dxmt,
            GraphicsBackend::D3DMetal,
        ]
    }

    /// Does this backend need the D3D/DXGI DLLs to come from a native layer?
    fn uses_native_d3d(self) -> bool {
        matches!(
            self,
            GraphicsBackend::Dxvk | GraphicsBackend::Dxmt | GraphicsBackend::D3DMetal
        )
    }
}

/// Where a bottle keeps its own settings.
///
/// Same idea: one config file inside the prefix, written
/// atomically, that belongs to that bottle alone.
pub fn bottle_config_path(prefix: &Path) -> PathBuf {
    crate::bottle_settings::settings_path(prefix)
}

/// Read the stored backend of a bottle.
pub fn read_backend(prefix: &Path) -> GraphicsBackend {
    crate::bottle_settings::read_setting(prefix, "GraphicsBackend")
        .and_then(|raw| GraphicsBackend::parse(&raw))
        .unwrap_or(GraphicsBackend::Auto)
}

/// Store the backend of a bottle (writes the whole file, atomically).
pub fn write_backend(prefix: &Path, backend: GraphicsBackend) -> Result<(), String> {
    crate::bottle_settings::write_setting(prefix, "GraphicsBackend", backend.as_str())
}

/// The registry bits a backend needs inside the bottle.
///
/// `renderer` is the key that actually matters on our engine: `vulkan` sends
/// WineD3D through `winevulkan` → MoltenVK → Metal (measured 4× faster than the
/// legacy OpenGL path, and it removes the 32-bit `wow64cpu` crash).
pub fn registry_plan(backend: GraphicsBackend) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut plan = Vec::new();
    match backend {
        GraphicsBackend::Dxvk => {
            plan.push(("HKCU\\Software\\Wine\\Direct3D", "renderer", "vulkan"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d9", "native"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d10core", "native"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d11", "native"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "dxgi", "native"));
        }
        GraphicsBackend::Dxmt => {
            plan.push(("HKCU\\Software\\Wine\\Direct3D", "renderer", "vulkan"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d11", "native"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "dxgi", "native"));
        }
        GraphicsBackend::D3DMetal => {
            plan.push(("HKCU\\Software\\Wine\\Direct3D", "renderer", "vulkan"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d11", "native"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d12", "native"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "dxgi", "native"));
        }
        GraphicsBackend::WineD3d => {
            plan.push(("HKCU\\Software\\Wine\\Direct3D", "renderer", "vulkan"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d9", "builtin"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d10core", "builtin"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "d3d11", "builtin"));
            plan.push(("HKCU\\Software\\Wine\\DllOverrides", "dxgi", "builtin"));
        }
        GraphicsBackend::Auto => {
            plan.push(("HKCU\\Software\\Wine\\Direct3D", "renderer", "vulkan"));
        }
    }
    plan
}

/// Environment for the launch command.
///
/// `JISR_GRAPHICS_BACKEND` is the variable our own launcher, diagnostics and
/// (in future) a patched Wine read to know which stack is active.
pub fn env(backend: GraphicsBackend) -> Vec<(String, String)> {
    let mut env = Vec::new();
    match backend {
        GraphicsBackend::Dxvk => {
            env.push(("DXVK_ASYNC".into(), "1".into()));
            env.push(("DXVK_HUD".into(), "devinfo".into()));
            env.push(("MVK_CONFIG_USE_METAL_ARGUMENT_BUFFERS".into(), "1".into()));
        }
        GraphicsBackend::WineD3d => {
            env.push(("MVK_CONFIG_USE_METAL_ARGUMENT_BUFFERS".into(), "1".into()));
        }
        GraphicsBackend::Dxmt | GraphicsBackend::D3DMetal | GraphicsBackend::Auto => {}
    }
    env.push(("JISR_GRAPHICS_BACKEND".into(), backend.as_str().into()));
    env.push((
        "JISR_ACTIVE_GRAPHICS_BACKEND".into(),
        backend.as_str().into(),
    ));
    env
}

/// Where a user-installed Game Porting Toolkit / D3DMetal may live.
///
/// We never bundle Apple's D3DMetal: the user installs the toolkit from Apple
/// (free Developer ID) and we only *use* what is on their machine.
pub fn d3dmetal_framework_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in [
        "/Applications/Game Porting Toolkit.app/Contents/Resources",
        "/Applications/Game Porting Toolkit 4.app/Contents/Resources",
        "/Library/Developer/GamePortingKit",
        "/usr/local/share/game-porting-toolkit",
    ] {
        let candidate = Path::new(root).join("D3DMetal.framework");
        if candidate.exists() {
            paths.push(candidate);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let candidate = Path::new(&home)
            .join("Applications/Game Porting Toolkit.app/Contents/Resources/D3DMetal.framework");
        if candidate.exists() {
            paths.push(candidate);
        }
    }
    paths
}

/// Is a user-installed D3DMetal available on this machine?
pub fn d3dmetal_available() -> bool {
    !d3dmetal_framework_candidates().is_empty()
}

/// Marker file that records which backend a prefix was prepared with.
fn marker_path(prefix: &Path) -> PathBuf {
    prefix.join(".jisr-graphics-backend.ok")
}

/// Has the prefix already been prepared for this backend?
pub fn is_prepared(prefix: &Path, backend: GraphicsBackend) -> bool {
    let current = std::fs::read_to_string(marker_path(prefix)).unwrap_or_default();
    current.trim() == backend.as_str()
}

/// Write the registry bits for a backend into a bottle.
///
/// `wine` is the Wine loader of the active engine. The write is idempotent and
/// recorded in a marker file so we do not touch the registry on every launch.
pub async fn apply(wine: &Path, prefix: &Path, backend: GraphicsBackend) -> Result<(), String> {
    if is_prepared(prefix, backend) {
        return Ok(());
    }
    for (key, name, value) in registry_plan(backend) {
        write_reg(wine, prefix, key, name, value).await?;
    }
    let _ = std::fs::write(marker_path(prefix), format!("{}\n", backend.as_str()));
    Ok(())
}

/// Write explicit per-DLL overrides into a bottle registry.
///
/// Used by the rules engine when a title needs something the backend plan does
/// not cover (their `dll_overrides` hack).
pub async fn apply_dll_overrides(
    wine: &Path,
    prefix: &Path,
    overrides: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (dll, value) in overrides {
        write_reg(
            wine,
            prefix,
            "HKCU\\Software\\Wine\\DllOverrides",
            dll,
            value,
        )
        .await?;
    }
    Ok(())
}

/// Does the backend need native D3D DLLs on the search path?
pub fn needs_native_d3d(backend: GraphicsBackend) -> bool {
    backend.uses_native_d3d()
}

async fn write_reg(
    wine: &Path,
    prefix: &Path,
    key: &str,
    name: &str,
    value: &str,
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(wine);
    cmd.env("WINEPREFIX", prefix)
        .arg("reg")
        .arg("add")
        .arg(key)
        .arg("/v")
        .arg(name)
        .arg("/t")
        .arg("REG_SZ")
        .arg("/d")
        .arg(value)
        .arg("/f")
        .stdin(std::process::Stdio::null());
    let ok = match tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(out)) => out.status.success(),
        Ok(Err(err)) => return Err(format!("تعذّر تنفيذ wine reg: {err}")),
        Err(_) => return Err("انتهت مهلة كتابة الريجستري".to_string()),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("فشل كتابة {key}\\{name}"))
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Which graphics backend does this bottle use?
#[tauri::command]
pub fn graphics_backend_get(bottle: String) -> String {
    if !crate::bottle::valid_bottle_id(&bottle) {
        return GraphicsBackend::Auto.as_str().to_string();
    }
    let prefix = crate::bottle::bottle_dir(&bottle);
    read_backend(&prefix).as_str().to_string()
}

/// Change the graphics backend of a bottle (per-bottle switch).
#[tauri::command]
pub fn graphics_backend_set(bottle: String, backend: String) -> Result<String, String> {
    if !crate::bottle::valid_bottle_id(&bottle) {
        return Err("معرّف القارورة غير صالح".to_string());
    }
    let parsed = GraphicsBackend::parse(&backend)
        .ok_or_else(|| format!("محرك رسومي غير معروف: {backend}"))?;
    let prefix = crate::bottle::bottle_dir(&bottle);
    write_backend(&prefix, parsed)?;
    Ok(parsed.as_str().to_string())
}

/// Is a user-installed D3DMetal (Game Porting Toolkit) available?
#[tauri::command]
pub fn d3dmetal_present() -> bool {
    d3dmetal_available()
}

/// List the backends the UI can offer, with a note whether D3DMetal is usable.
#[tauri::command]
pub fn graphics_backends() -> Vec<serde_json::Value> {
    let d3dmetal = d3dmetal_available();
    GraphicsBackend::all()
        .iter()
        .map(|backend| {
            serde_json::json!({
                "id": backend.as_str(),
                "available": match backend {
                    GraphicsBackend::D3DMetal => d3dmetal,
                    _ => true,
                },
                "note": match backend {
                    GraphicsBackend::D3DMetal => {
                        if d3dmetal { "D3DMetal مثبّت على جهازك" }
                        else { "يتطلب Game Porting Toolkit من Apple (مجاني) — يُثبّته المستخدم" }
                    }
                    _ => "",
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("jisr-gfx-{name}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_only_known_backends() {
        assert_eq!(GraphicsBackend::parse("dxvk"), Some(GraphicsBackend::Dxvk));
        assert_eq!(
            GraphicsBackend::parse(" D3DMETAL "),
            Some(GraphicsBackend::D3DMetal)
        );
        assert_eq!(
            GraphicsBackend::parse("wine"),
            Some(GraphicsBackend::WineD3d)
        );
        assert_eq!(GraphicsBackend::parse(""), Some(GraphicsBackend::Auto));
        assert_eq!(GraphicsBackend::parse("../evil"), None);
        assert_eq!(GraphicsBackend::parse("vkd3d"), None);
    }

    #[test]
    fn every_backend_round_trips_through_its_name() {
        for backend in GraphicsBackend::all() {
            assert_eq!(GraphicsBackend::parse(backend.as_str()), Some(*backend));
        }
    }

    #[test]
    fn bottle_config_round_trips() {
        let dir = temp_dir("config");
        assert_eq!(read_backend(&dir), GraphicsBackend::Auto);
        write_backend(&dir, GraphicsBackend::Dxmt).expect("write");
        assert_eq!(read_backend(&dir), GraphicsBackend::Dxmt);
        write_backend(&dir, GraphicsBackend::D3DMetal).expect("rewrite");
        assert_eq!(read_backend(&dir), GraphicsBackend::D3DMetal);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bottle_config_keeps_unrelated_sections() {
        let dir = temp_dir("keep");
        std::fs::write(
            bottle_config_path(&dir),
            "[Jisr]\nGraphicsBackend=dxvk\n\n[Custom]\nKey=Value\n",
        )
        .unwrap();
        write_backend(&dir, GraphicsBackend::WineD3d).expect("write");
        let text = std::fs::read_to_string(bottle_config_path(&dir)).unwrap();
        assert!(text.contains("[Custom]"));
        assert!(text.contains("Key=Value"));
        assert!(text.contains("GraphicsBackend=wined3d"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dxvk_plan_uses_native_dlls_and_vulkan() {
        let plan = registry_plan(GraphicsBackend::Dxvk);
        assert!(
            plan.iter()
                .any(|(k, n, v)| k.ends_with("DllOverrides") && *n == "d3d9" && *v == "native")
        );
        assert!(
            plan.iter()
                .any(|(k, n, v)| k.ends_with("Direct3D") && *n == "renderer" && *v == "vulkan")
        );
    }

    #[test]
    fn wined3d_plan_forces_builtin_dlls() {
        let plan = registry_plan(GraphicsBackend::WineD3d);
        assert!(
            plan.iter()
                .any(|(k, n, v)| k.ends_with("DllOverrides") && *n == "d3d9" && *v == "builtin")
        );
    }

    #[test]
    fn env_carries_the_backend_pair() {
        let env = env(GraphicsBackend::Dxmt);
        let map: BTreeMap<_, _> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(map.get("JISR_GRAPHICS_BACKEND"), Some(&"dxmt"));
        assert_eq!(map.get("JISR_ACTIVE_GRAPHICS_BACKEND"), Some(&"dxmt"));
    }

    #[test]
    fn dxvk_env_enables_async() {
        let env = env(GraphicsBackend::Dxvk);
        assert!(env.iter().any(|(k, v)| k == "DXVK_ASYNC" && v == "1"));
    }

    #[test]
    fn marker_tracks_the_prepared_backend() {
        let dir = temp_dir("marker");
        assert!(!is_prepared(&dir, GraphicsBackend::Dxvk));
        std::fs::write(marker_path(&dir), "dxvk\n").unwrap();
        assert!(is_prepared(&dir, GraphicsBackend::Dxvk));
        assert!(!is_prepared(&dir, GraphicsBackend::WineD3d));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn native_d3d_is_only_needed_for_the_accelerated_backends() {
        assert!(needs_native_d3d(GraphicsBackend::Dxvk));
        assert!(needs_native_d3d(GraphicsBackend::D3DMetal));
        assert!(!needs_native_d3d(GraphicsBackend::WineD3d));
    }
}
