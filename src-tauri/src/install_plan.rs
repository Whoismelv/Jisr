//! Install planning engine.
//!
//! A pure, testable planner that turns install profiles into an ordered task
//! graph:
//!
//! * The task graph is derived from the install-profile graph.
//! * Every application becomes one install task.
//! * Download tasks, media (CD/DVD) tasks, bottle-creation tasks and
//!   post-install tasks are additional nodes in that graph.
//! * Dependencies between install profiles become dependencies between tasks.
//! * Cycles must be detected and reported, never executed.
//!
//! This module does **not** execute anything. It produces a deterministic,
//! inspectable plan; the caller decides how to run each step.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// What a task does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskKind {
    /// Create the bottle the install runs in.
    CreateBottle,
    /// Fetch the installer from the network.
    Download {
        url: String,
        checksum: Option<String>,
    },
    /// Ask the user to insert physical media.
    InsertMedia,
    /// Release previously inserted media.
    ReleaseMedia,
    /// Run the installer itself.
    RunInstaller { command: Vec<String> },
    /// Unpack a downloaded archive before running it.
    Uncompress {
        archive: String,
        destination: String,
    },
    /// Copy files into the bottle.
    CopyFiles { source: String, destination: String },
    /// Apply registry entries after the installer ran.
    ApplyRegistry,
    /// Register COM/DLL components.
    RegisterDlls,
    /// Restart the bottle so the changes take effect.
    Reboot,
    /// Sync the desktop menu with the bottle.
    SyncMenu,
    /// Sync file associations with the bottle.
    SyncAssociations,
}

impl TaskKind {
    /// A short, stable label used in logs and in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            TaskKind::CreateBottle => "create-bottle",
            TaskKind::Download { .. } => "download",
            TaskKind::InsertMedia => "insert-media",
            TaskKind::ReleaseMedia => "release-media",
            TaskKind::RunInstaller { .. } => "run-installer",
            TaskKind::Uncompress { .. } => "uncompress",
            TaskKind::CopyFiles { .. } => "copy-files",
            TaskKind::ApplyRegistry => "apply-registry",
            TaskKind::RegisterDlls => "register-dlls",
            TaskKind::Reboot => "reboot",
            TaskKind::SyncMenu => "sync-menu",
            TaskKind::SyncAssociations => "sync-associations",
        }
    }
}

/// One node of the install plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Stable identifier, unique inside a plan.
    pub id: String,
    pub kind: TaskKind,
    /// Tasks that must finish before this one starts.
    pub depends_on: Vec<String>,
    /// Lower values run first when several tasks are ready.
    pub niceness: i32,
}

impl Task {
    fn new(id: impl Into<String>, kind: TaskKind, niceness: i32) -> Self {
        Task {
            id: id.into(),
            kind,
            depends_on: Vec::new(),
            niceness,
        }
    }
}

/// Why a plan could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The install-profile graph contains a cycle.
    Cycle { apps: Vec<String> },
    /// A dependency refers to an application nobody asked for.
    UnknownDependency { app: String, dependency: String },
    /// The same application was requested twice.
    DuplicateApp { app: String },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::Cycle { apps } => {
                write!(
                    f,
                    "install profile graph has a cycle: {}",
                    apps.join(" -> ")
                )
            }
            PlanError::UnknownDependency { app, dependency } => {
                write!(
                    f,
                    "{app} depends on {dependency}, which is not part of the plan"
                )
            }
            PlanError::DuplicateApp { app } => {
                write!(f, "{app} was requested more than once")
            }
        }
    }
}

impl std::error::Error for PlanError {}

/// What we know about one application before installing it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppRequest {
    /// Stable application id (e.g. `jisr.app.3`).
    pub appid: String,
    /// Human readable name, used for logs only.
    pub name: String,
    /// Applications that must be installed first.
    pub dependencies: Vec<String>,
    /// Where the installer comes from, when it is downloadable.
    pub download: Option<(String, Option<String>)>,
    /// The installer command, when it is known.
    pub installer_command: Vec<String>,
    /// Files that must be copied into the bottle.
    pub files_to_copy: Vec<(String, String)>,
    /// Archive to unpack before running the installer (url, destination).
    pub uncompress: Option<(String, String)>,
    /// Registry entries to write after the install.
    pub registry_entries: usize,
    /// Whether the bottle must be rebooted afterwards.
    pub reboot_after: bool,
}

/// Inputs for one planning run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanRequest {
    /// The applications to install, in user-visible order.
    pub apps: Vec<AppRequest>,
    /// Whether the target bottle already exists.
    pub bottle_exists: bool,
    /// Whether the installer media must be asked from the user.
    pub needs_media: bool,
}

/// A validated, ordered install plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    /// Tasks in execution order.
    pub tasks: Vec<Task>,
}

impl InstallPlan {
    /// Total number of tasks.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Task ids grouped by kind, for quick inspection.
    pub fn counts_by_kind(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for task in &self.tasks {
            *counts.entry(task.kind.label()).or_insert(0) += 1;
        }
        counts
    }

    /// Verify that every dependency is satisfied by an earlier task.
    pub fn verify_order(&self) -> Result<(), String> {
        let mut seen: HashSet<&str> = HashSet::new();
        for task in &self.tasks {
            for dep in &task.depends_on {
                if !seen.contains(dep.as_str()) {
                    return Err(format!(
                        "task {} runs before its dependency {}",
                        task.id, dep
                    ));
                }
            }
            seen.insert(task.id.as_str());
        }
        Ok(())
    }
}

/// Niceness values, defining the deterministic ordering of the plan.
const NICENESS_DOWNLOAD: i32 = 0;
const NICENESS_MEDIA: i32 = 10;
const NICENESS_BOTTLE: i32 = 20;
const NICENESS_INSTALL: i32 = 30;
const NICENESS_POST: i32 = 40;

/// Build an install plan.
///
/// The planner is deterministic: the same request always yields the same
/// task list in the same order.
pub fn plan(request: &PlanRequest) -> Result<InstallPlan, PlanError> {
    let mut apps: BTreeMap<String, AppRequest> = BTreeMap::new();
    for app in &request.apps {
        if apps.insert(app.appid.clone(), app.clone()).is_some() {
            return Err(PlanError::DuplicateApp {
                app: app.appid.clone(),
            });
        }
    }

    // 1. Validate the dependency graph and detect cycles.
    let order = topological_order(&apps)?;

    // 2. Expand the graph into tasks.
    let mut tasks: Vec<Task> = Vec::new();

    if !request.bottle_exists {
        let mut task = Task::new("bottle::create", TaskKind::CreateBottle, NICENESS_BOTTLE);
        task.depends_on = Vec::new();
        tasks.push(task);
    }
    // Only depend on the bottle task when it actually exists.
    let bottle_task = tasks
        .iter()
        .find(|t| t.kind == TaskKind::CreateBottle)
        .map(|t| t.id.clone());

    for appid in &order {
        let app = &apps[appid];
        let install_id = format!("install::{appid}");

        // Download task (if the installer is remote).
        if let Some((url, checksum)) = &app.download {
            let mut task = Task::new(
                format!("download::{appid}"),
                TaskKind::Download {
                    url: url.clone(),
                    checksum: checksum.clone(),
                },
                NICENESS_DOWNLOAD,
            );
            if let Some(bottle) = &bottle_task {
                task.depends_on.push(bottle.clone());
            }
            tasks.push(task);
        }

        // Media tasks (only when the profile needs physical media).
        if request.needs_media && app.download.is_none() {
            let mut insert = Task::new(
                format!("media::insert::{appid}"),
                TaskKind::InsertMedia,
                NICENESS_MEDIA,
            );
            if let Some(bottle) = &bottle_task {
                insert.depends_on.push(bottle.clone());
            }
            let insert_id = insert.id.clone();
            tasks.push(insert);

            let mut release = Task::new(
                format!("media::release::{appid}"),
                TaskKind::ReleaseMedia,
                NICENESS_MEDIA,
            );
            release.depends_on.push(insert_id);
            release.niceness = NICENESS_INSTALL + 5;
            tasks.push(release);
        }

        // Unpack the archive (their `_main_uncompress` step) before the install.
        if let Some((archive, destination)) = &app.uncompress {
            let mut task = Task::new(
                format!("uncompress::{appid}"),
                TaskKind::Uncompress {
                    archive: archive.clone(),
                    destination: destination.clone(),
                },
                NICENESS_INSTALL - 1,
            );
            if let Some(download) = tasks.iter().find(|t| t.id == format!("download::{appid}")) {
                task.depends_on.push(download.id.clone());
            }
            tasks.push(task);
        }

        // The installer itself.
        let mut install = Task::new(
            install_id.clone(),
            TaskKind::RunInstaller {
                command: app.installer_command.clone(),
            },
            NICENESS_INSTALL,
        );
        for dependency in &app.dependencies {
            install.depends_on.push(format!("install::{dependency}"));
        }
        if let Some(task) = tasks.iter().find(|t| t.id == format!("download::{appid}")) {
            install.depends_on.push(task.id.clone());
        }
        if let Some(task) = tasks
            .iter()
            .find(|t| t.id == format!("uncompress::{appid}"))
        {
            install.depends_on.push(task.id.clone());
        }
        if let Some(task) = tasks
            .iter()
            .find(|t| t.id == format!("media::insert::{appid}"))
        {
            install.depends_on.push(task.id.clone());
        }
        tasks.push(install);

        // Files copied next to the install.
        for (index, (source, destination)) in app.files_to_copy.iter().enumerate() {
            let mut task = Task::new(
                format!("copy::{appid}::{index}"),
                TaskKind::CopyFiles {
                    source: source.clone(),
                    destination: destination.clone(),
                },
                NICENESS_INSTALL + 1,
            );
            task.depends_on.push(install_id.clone());
            tasks.push(task);
        }

        // Registry + DLL registration.
        if app.registry_entries > 0 {
            let mut task = Task::new(
                format!("registry::{appid}"),
                TaskKind::ApplyRegistry,
                NICENESS_POST - 5,
            );
            task.depends_on.push(install_id.clone());
            tasks.push(task);
        }

        let mut register = Task::new(
            format!("register::{appid}"),
            TaskKind::RegisterDlls,
            NICENESS_POST - 4,
        );
        register.depends_on.push(install_id.clone());
        tasks.push(register);
    }

    // 3. Post-install tasks: menu + associations, then optional reboot.
    let installed: Vec<String> = order.iter().map(|a| format!("install::{a}")).collect();
    let mut menu = Task::new("post::menu", TaskKind::SyncMenu, NICENESS_POST);
    menu.depends_on = installed.clone();
    tasks.push(menu);

    let mut assoc = Task::new(
        "post::associations",
        TaskKind::SyncAssociations,
        NICENESS_POST + 1,
    );
    assoc.depends_on = installed.clone();
    tasks.push(assoc);

    if request.apps.iter().any(|a| a.reboot_after) {
        let mut reboot = Task::new("post::reboot", TaskKind::Reboot, NICENESS_POST + 2);
        reboot.depends_on = vec!["post::menu".to_string(), "post::associations".to_string()];
        tasks.push(reboot);
    }

    // 4. Sort into a runnable order (stable, deterministic).
    let sorted = sort_tasks(tasks);

    Ok(InstallPlan { tasks: sorted })
}

/// Topological order of the requested applications.
///
/// Dependencies are listed in `AppRequest::dependencies`; a dependency that is
/// not part of the request is an error: the planner never installs an app
/// whose prerequisites are missing.
fn topological_order(apps: &BTreeMap<String, AppRequest>) -> Result<Vec<String>, PlanError> {
    for (appid, app) in apps {
        for dependency in &app.dependencies {
            if !apps.contains_key(dependency) {
                return Err(PlanError::UnknownDependency {
                    app: appid.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }

    let mut visiting: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut order: Vec<String> = Vec::new();
    let mut stack: Vec<String> = Vec::new();

    for appid in apps.keys() {
        visit(
            appid,
            apps,
            &mut visiting,
            &mut visited,
            &mut order,
            &mut stack,
        )?;
    }
    Ok(order)
}

fn visit(
    appid: &str,
    apps: &BTreeMap<String, AppRequest>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    order: &mut Vec<String>,
    stack: &mut Vec<String>,
) -> Result<(), PlanError> {
    if visited.contains(appid) {
        return Ok(());
    }
    if visiting.contains(appid) {
        // Report the cycle from the first repeated node onwards.
        let start = stack.iter().position(|a| a == appid).unwrap_or(0);
        let mut cycle: Vec<String> = stack[start..].to_vec();
        cycle.push(appid.to_string());
        return Err(PlanError::Cycle { apps: cycle });
    }
    visiting.insert(appid.to_string());
    stack.push(appid.to_string());

    if let Some(app) = apps.get(appid) {
        for dependency in &app.dependencies {
            visit(dependency, apps, visiting, visited, order, stack)?;
        }
    }

    stack.pop();
    visiting.remove(appid);
    visited.insert(appid.to_string());
    order.push(appid.to_string());
    Ok(())
}

/// Order tasks so that dependencies come first; ties break by niceness then id.
fn sort_tasks(tasks: Vec<Task>) -> Vec<Task> {
    let mut by_id: HashMap<String, Task> = HashMap::new();
    for task in tasks {
        by_id.insert(task.id.clone(), task);
    }

    let mut remaining: BTreeSet<String> = by_id.keys().cloned().collect();
    let mut done: HashSet<String> = HashSet::new();
    let mut sorted: Vec<Task> = Vec::new();

    while !remaining.is_empty() {
        // Ready = all dependencies are already scheduled.
        let mut ready: Vec<String> = remaining
            .iter()
            .filter(|id| {
                by_id[*id]
                    .depends_on
                    .iter()
                    .all(|dep| done.contains(dep) || !by_id.contains_key(dep))
            })
            .cloned()
            .collect();

        if ready.is_empty() {
            // Dependency cycle between tasks: fall back to a stable order.
            let rest: Vec<String> = remaining.iter().cloned().collect();
            for id in rest {
                if let Some(task) = by_id.remove(&id) {
                    sorted.push(task);
                }
            }
            break;
        }

        ready.sort_by(|a, b| {
            let ta = &by_id[a];
            let tb = &by_id[b];
            ta.niceness
                .cmp(&tb.niceness)
                .then_with(|| ta.id.cmp(&tb.id))
        });

        for id in ready {
            remaining.remove(&id);
            done.insert(id.clone());
            if let Some(task) = by_id.remove(&id) {
                sorted.push(task);
            }
        }
    }

    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(appid: &str, deps: &[&str]) -> AppRequest {
        AppRequest {
            appid: appid.to_string(),
            name: appid.to_string(),
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn builds_a_bottle_task_when_the_prefix_is_missing() {
        let request = PlanRequest {
            apps: vec![app("game", &[])],
            bottle_exists: false,
            needs_media: false,
        };
        let plan = plan(&request).expect("plan");
        assert!(plan.tasks.iter().any(|t| t.kind == TaskKind::CreateBottle));
        assert!(plan.verify_order().is_ok());
    }

    #[test]
    fn skips_the_bottle_task_when_the_prefix_exists() {
        let request = PlanRequest {
            apps: vec![app("game", &[])],
            bottle_exists: true,
            needs_media: false,
        };
        let plan = plan(&request).expect("plan");
        assert!(!plan.tasks.iter().any(|t| t.kind == TaskKind::CreateBottle));
    }

    #[test]
    fn dependencies_run_before_their_dependents() {
        let request = PlanRequest {
            apps: vec![app("game", &["runtime"]), app("runtime", &[])],
            bottle_exists: true,
            needs_media: false,
        };
        let plan = plan(&request).expect("plan");
        let runtime = plan
            .tasks
            .iter()
            .position(|t| t.id == "install::runtime")
            .expect("runtime task");
        let game = plan
            .tasks
            .iter()
            .position(|t| t.id == "install::game")
            .expect("game task");
        assert!(runtime < game, "runtime must be installed first");
        assert!(plan.verify_order().is_ok());
    }

    #[test]
    fn download_runs_before_the_installer() {
        let mut game = app("game", &[]);
        game.download = Some((
            "https://example.invalid/game.exe".into(),
            Some("abc".into()),
        ));
        let request = PlanRequest {
            apps: vec![game],
            bottle_exists: true,
            needs_media: false,
        };
        let plan = plan(&request).expect("plan");
        let download = plan
            .tasks
            .iter()
            .position(|t| t.id == "download::game")
            .expect("download task");
        let install = plan
            .tasks
            .iter()
            .position(|t| t.id == "install::game")
            .expect("install task");
        assert!(download < install);
        assert!(plan.verify_order().is_ok());
    }

    #[test]
    fn media_tasks_bracket_the_installer() {
        let request = PlanRequest {
            apps: vec![app("game", &[])],
            bottle_exists: true,
            needs_media: true,
        };
        let plan = plan(&request).expect("plan");
        let insert = plan
            .tasks
            .iter()
            .position(|t| t.id == "media::insert::game")
            .expect("insert task");
        let install = plan
            .tasks
            .iter()
            .position(|t| t.id == "install::game")
            .expect("install task");
        let release = plan
            .tasks
            .iter()
            .position(|t| t.id == "media::release::game")
            .expect("release task");
        assert!(insert < install);
        assert!(install < release);
        assert!(plan.verify_order().is_ok());
    }

    #[test]
    fn cycles_are_reported_instead_of_looping() {
        let request = PlanRequest {
            apps: vec![app("a", &["b"]), app("b", &["a"])],
            bottle_exists: true,
            needs_media: false,
        };
        let error = plan(&request).expect_err("cycle must fail");
        match error {
            PlanError::Cycle { apps } => {
                assert!(apps.len() >= 3, "cycle path should be reported: {apps:?}");
            }
            other => panic!("expected a cycle, got {other:?}"),
        }
    }

    #[test]
    fn missing_dependencies_are_refused() {
        let request = PlanRequest {
            apps: vec![app("game", &["runtime"])],
            bottle_exists: true,
            needs_media: false,
        };
        let error = plan(&request).expect_err("missing dependency must fail");
        assert_eq!(
            error,
            PlanError::UnknownDependency {
                app: "game".into(),
                dependency: "runtime".into()
            }
        );
    }

    #[test]
    fn duplicate_requests_are_refused() {
        let request = PlanRequest {
            apps: vec![app("game", &[]), app("game", &[])],
            bottle_exists: true,
            needs_media: false,
        };
        let error = plan(&request).expect_err("duplicate must fail");
        assert_eq!(error, PlanError::DuplicateApp { app: "game".into() });
    }

    #[test]
    fn reboot_is_planned_only_when_needed() {
        let mut game = app("game", &[]);
        game.reboot_after = true;
        let request = PlanRequest {
            apps: vec![game],
            bottle_exists: true,
            needs_media: false,
        };
        let plan = plan(&request).expect("plan");
        let reboot = plan
            .tasks
            .iter()
            .position(|t| t.kind == TaskKind::Reboot)
            .expect("reboot task");
        let menu = plan
            .tasks
            .iter()
            .position(|t| t.kind == TaskKind::SyncMenu)
            .expect("menu task");
        assert!(menu < reboot);
        assert!(plan.verify_order().is_ok());
    }

    #[test]
    fn planning_is_deterministic() {
        let request = PlanRequest {
            apps: vec![app("b", &["a"]), app("a", &[]), app("c", &["a"])],
            bottle_exists: false,
            needs_media: false,
        };
        let first = plan(&request).expect("plan");
        let second = plan(&request).expect("plan");
        assert_eq!(first, second);
    }

    #[test]
    fn counts_by_kind_summarises_the_plan() {
        let request = PlanRequest {
            apps: vec![app("game", &[])],
            bottle_exists: true,
            needs_media: false,
        };
        let plan = plan(&request).expect("plan");
        let counts = plan.counts_by_kind();
        assert_eq!(counts.get("run-installer"), Some(&1));
        assert_eq!(counts.get("sync-menu"), Some(&1));
    }
}
