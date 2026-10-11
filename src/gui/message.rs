use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{collections::HashMap, sync::Arc};

use futures::FutureExt;
use tokio::{
    sync::mpsc::{self, Sender},
    task::JoinHandle,
};
use tracing::*;

use super::SelfUpdateProgress;
use super::{
    App, SpecFetchProgress, WindowProviderParameters,
    request_counter::{RequestCounter, RequestID},
};
use crate::gui::LastAction;
use crate::integrate::*;
use crate::mod_lints::{LintId, LintReport};
use crate::state::ModOrGroup;
use crate::*;
use crate::{
    providers::{FetchProgress, ModStore},
    state::ModConfig,
};
use mint_lib::error::GenericError;
use mint_lib::mod_info::MetaConfig;
use mint_lib::update::GitHubRelease;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct MessageHandle<S> {
    pub rid: RequestID,
    pub handle: JoinHandle<()>,
    pub state: S,
    pub cancellation: Option<Arc<AtomicBool>>,
}

#[derive(Debug)]
pub enum Message {
    OperationFailed(RequestID),
    ResolveMods(ResolveMods),
    Integrate(Integrate),
    FetchModProgress(FetchModProgress),
    UpdateCache(UpdateCache),
    CheckUpdates(CheckUpdates),
    LintMods(Box<LintMods>),
    SelfUpdate(SelfUpdate),
    FetchSelfUpdateProgress(FetchSelfUpdateProgress),
}

impl Message {
    pub fn handle(self, app: &mut App) {
        match self {
            Self::OperationFailed(rid) => {
                if app.check_updates_rid.as_ref().is_some_and(|h| h.rid == rid) {
                    app.app_update_status = "Could not check for updates. Please try again.".into();
                }
                if app.self_update_rid.as_ref().is_some_and(|h| h.rid == rid) {
                    app.self_update_control = None;
                    app.app_update_status =
                        "Update stopped unexpectedly. See Details and retry.".into();
                }
                fn clear<S>(handle: &mut Option<MessageHandle<S>>, rid: RequestID) -> bool {
                    if handle.as_ref().is_some_and(|handle| handle.rid == rid) {
                        *handle = None;
                        true
                    } else {
                        false
                    }
                }
                let matched = clear(&mut app.resolve_mod_rid, rid)
                    | clear(&mut app.integrate_rid, rid)
                    | clear(&mut app.update_rid, rid)
                    | clear(&mut app.check_updates_rid, rid)
                    | clear(&mut app.lint_rid, rid)
                    | clear(&mut app.self_update_rid, rid);
                if matched {
                    app.last_action = Some(LastAction::failure("A background operation stopped unexpectedly. No successful completion was reported. Inspect the log and retry; restart MINT if the error persists.".into()));
                }
            }
            Self::ResolveMods(msg) => msg.receive(app),
            Self::Integrate(msg) => msg.receive(app),
            Self::FetchModProgress(msg) => msg.receive(app),
            Self::UpdateCache(msg) => msg.receive(app),
            Self::CheckUpdates(msg) => msg.receive(app),
            Self::LintMods(msg) => msg.receive(app),
            Self::SelfUpdate(msg) => msg.receive(app),
            Self::FetchSelfUpdateProgress(msg) => msg.receive(app),
        }
    }
}

#[derive(Debug)]
pub struct ResolveMods {
    rid: RequestID,
    specs: Vec<ModSpecification>,
    result: Result<crate::providers::ResolvedMods, ProviderError>,
    is_dependency: bool,
}

fn spawn_reported(
    rid: RequestID,
    ctx: egui::Context,
    tx: Sender<Message>,
    future: impl std::future::Future<Output = ()> + Send + 'static,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if std::panic::AssertUnwindSafe(future)
            .catch_unwind()
            .await
            .is_err()
        {
            let _ = tx.send(Message::OperationFailed(rid)).await;
            ctx.request_repaint();
        }
    })
}

impl ResolveMods {
    pub fn send(
        app: &mut App,
        ctx: &egui::Context,
        specs: Vec<ModSpecification>,
        is_dependency: bool,
    ) {
        let rid = app.request_counter.next();
        let store = app.state.store.clone();
        let ctx = ctx.clone();
        let tx = app.tx.clone();
        let handle = spawn_reported(rid, ctx.clone(), tx.clone(), async move {
            let result = Ok(store.resolve_mods_partial(&specs, false).await);
            tx.send(Message::ResolveMods(Self {
                rid,
                specs,
                result,
                is_dependency,
            }))
            .await
            .unwrap();
            ctx.request_repaint();
        });
        app.last_action = None;
        app.resolve_mod_rid = Some(MessageHandle {
            rid,
            handle,
            state: (),
            cancellation: None,
        });
    }

    fn receive(self, app: &mut App) {
        if Some(self.rid) == app.resolve_mod_rid.as_ref().map(|r| r.rid) {
            match self.result {
                Ok(resolved_mods) => {
                    let primary_mods = self
                        .specs
                        .into_iter()
                        .collect::<HashSet<ModSpecification>>();
                    let mut additions: Vec<ModConfig> = Vec::new();
                    for (resolved_spec, info) in resolved_mods.mods {
                        let is_dep = self.is_dependency || !primary_mods.contains(&resolved_spec);
                        let add = if is_dep {
                            // if mod is a dependency then check if there is a disabled
                            // mod that satisfies the dependency and enable it. if it
                            // is not a dependency then assume the user explicitly
                            // wants to add a specific mod version.
                            let active_profile = app.state.mod_data.active_profile.clone();
                            !additions
                                .iter()
                                .any(|mc| mc.spec.satisfies_dependency(&resolved_spec))
                                && !app.state.mod_data.any_mod_mut(
                                    &active_profile,
                                    |mc, mod_group_enabled| {
                                        if mc.spec.satisfies_dependency(&resolved_spec) {
                                            mc.enabled = true;
                                            if let Some(mod_group_enabled) = mod_group_enabled {
                                                *mod_group_enabled = true;
                                            }
                                            true
                                        } else {
                                            false
                                        }
                                    },
                                )
                        } else {
                            true
                        };

                        if add {
                            additions.push(ModConfig {
                                spec: info.spec.clone(),
                                required: info.suggested_require,
                                enabled: true,
                                priority: 0,
                            });
                        }
                    }
                    let profile = app.state.mod_data.get_active_profile_mut();
                    let index = profile.standalone_start();
                    profile.mods.splice(
                        index..index,
                        additions.into_iter().map(ModOrGroup::Individual),
                    );
                    app.resolve_mod = resolved_mods
                        .errors
                        .iter()
                        .map(|(spec, _)| spec.url.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    app.last_action = Some(if resolved_mods.errors.is_empty() {
                        LastAction::success("mods successfully resolved".to_string())
                    } else {
                        for (_, error) in &resolved_mods.errors {
                            if let ProviderError::NoProvider { factory, .. } = error {
                                app.window_provider_parameters =
                                    Some(WindowProviderParameters::new(factory, &app.state));
                                break;
                            }
                        }
                        LastAction::failure(format!(
                            "Some mods could not be added. Successful entries were kept. Retry the remaining inputs.\n{}",
                            resolved_mods
                                .errors
                                .iter()
                                .map(|(spec, error)| format!("{}: {error}", spec.url))
                                .collect::<Vec<_>>()
                                .join("\n")
                        ))
                    });
                    let previous_edit = app.history.undo_id();
                    if app.save_mod_data("Add mods")
                        && let Some(action) = &mut app.last_action
                        && matches!(action.status, super::LastActionStatus::Success(_))
                    {
                        action.undo_id = app
                            .history
                            .undo_id()
                            .filter(|id| Some(*id) != previous_edit);
                    }
                }
                Err(ProviderError::NoProvider { url: _, factory }) => {
                    app.window_provider_parameters =
                        Some(WindowProviderParameters::new(factory, &app.state));
                    app.last_action = Some(LastAction::failure("no provider".to_string()));
                }
                Err(e) => {
                    error!("{}", e);
                    app.problematic_mod_id = e.opt_mod_id();
                    app.last_action =
                        Some(LastAction::failure(super::diagnostics::error_details(&e)));
                }
            }
            app.resolve_mod_rid = None;
        }
    }
}

#[derive(Debug)]
pub struct Integrate {
    rid: RequestID,
    result: Result<(), IntegrationError>,
}

impl Integrate {
    pub fn send(
        rc: &mut RequestCounter,
        store: Arc<ModStore>,
        mods: Vec<ModSpecification>,
        fsd_pak: PathBuf,
        config: MetaConfig,
        tx: Sender<Message>,
        ctx: egui::Context,
    ) -> MessageHandle<HashMap<ModSpecification, SpecFetchProgress>> {
        let rid = rc.next();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = Some(cancelled.clone());
        MessageHandle {
            rid,
            handle: spawn_reported(rid, ctx.clone(), tx.clone(), async move {
                let res = integrate_async(
                    store,
                    ctx.clone(),
                    mods,
                    fsd_pak,
                    config,
                    rid,
                    tx.clone(),
                    cancelled,
                )
                .await;
                tx.send(Message::Integrate(Integrate { rid, result: res }))
                    .await
                    .unwrap();
                ctx.request_repaint();
            }),
            state: Default::default(),
            cancellation,
        }
    }

    fn receive(self, app: &mut App) {
        if Some(self.rid) == app.integrate_rid.as_ref().map(|r| r.rid) {
            match self.result {
                Ok(()) => {
                    info!("integration complete");
                    app.last_action = Some(LastAction::success("integration complete".to_string()));
                }
                Err(ref e)
                    if let IntegrationError::ProviderError { source } = e
                        && let ProviderError::NoProvider { url: _, factory } = source =>
                {
                    app.window_provider_parameters =
                        Some(WindowProviderParameters::new(factory, &app.state));
                    app.last_action = Some(LastAction::failure("no provider".to_string()));
                }
                Err(e) => {
                    error!("{}", e);
                    app.problematic_mod_id = e.opt_mod_id();
                    app.last_action =
                        Some(LastAction::failure(super::diagnostics::error_details(&e)));
                }
            }
            app.integrate_rid = None;
        }
    }
}

#[derive(Debug)]
pub struct FetchModProgress {
    rid: RequestID,
    spec: ModSpecification,
    progress: SpecFetchProgress,
}

impl FetchModProgress {
    fn receive(self, app: &mut App) {
        if let Some(MessageHandle { rid, state, .. }) = &mut app.integrate_rid
            && *rid == self.rid
        {
            state.insert(self.spec, self.progress);
        }
    }
}

#[derive(Debug)]
pub struct UpdateCache {
    rid: RequestID,
    result: crate::providers::refresh::RefreshReport,
}

impl UpdateCache {
    pub fn send(app: &mut App, ctx: &egui::Context) {
        Self::send_specs(app, ctx, app.state.store.cached_mod_specs());
    }

    pub fn send_specs(app: &mut App, ctx: &egui::Context, specs: Vec<ModSpecification>) {
        if app.editing_busy() || app.self_update_rid.is_some() {
            return;
        }
        let ctx = ctx.clone();
        let rid = app.request_counter.next();
        let tx = app.tx.clone();
        let store = app.state.store.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = Some(cancelled.clone());
        app.refresh_progress = Default::default();
        let specs = store.refresh_specs(specs);
        app.refresh_total = specs.len();
        let progress = app.refresh_progress.clone();
        let handle = spawn_reported(rid, ctx.clone(), tx.clone(), async move {
            let res = store.refresh_mods(specs, cancelled, progress).await;
            tx.send(Message::UpdateCache(UpdateCache { rid, result: res }))
                .await
                .unwrap();
            ctx.request_repaint();
        });
        app.last_action = None;
        app.update_rid = Some(MessageHandle {
            rid,
            handle,
            state: (),
            cancellation,
        });
    }

    fn receive(self, app: &mut App) {
        if Some(self.rid) == app.update_rid.as_ref().map(|r| r.rid) {
            let mut report = self.result;
            let secrets = app
                .state
                .config
                .provider_parameters
                .values()
                .flat_map(|p| p.values().cloned())
                .collect::<Vec<_>>();
            super::updates::redact_report(&mut report, &secrets);
            let text = report.details();
            let mut action = if report.failures.is_empty() && report.save_error.is_none() {
                info!("{}", report.summary());
                LastAction::success(text)
            } else {
                warn!("{}", report.summary());
                LastAction::failure(text)
            };
            action.report = Some(Arc::new(report));
            app.last_action = Some(action);
            app.update_rid = None;
        }
    }
}

#[derive(Debug)]
pub struct CheckUpdates {
    rid: RequestID,
    result: Result<GitHubRelease, GenericError>,
}

impl CheckUpdates {
    pub fn send(app: &mut App, ctx: &egui::Context, manual: bool) {
        if app.check_updates_rid.is_some() {
            return;
        }
        let rid = app.request_counter.next();
        let tx = app.tx.clone();
        let ctx = ctx.clone();
        app.app_update_status = "Checking for updates...".into();
        let handle = spawn_reported(rid, ctx.clone(), tx.clone(), async move {
            let result = mint_lib::update::get_latest_release().await;
            let _ = tx.send(Message::CheckUpdates(Self { rid, result })).await;
            ctx.request_repaint();
        });
        app.check_updates_rid = Some(MessageHandle {
            rid,
            handle,
            state: manual,
            cancellation: None,
        });
    }

    fn receive(self, app: &mut App) {
        if !app
            .check_updates_rid
            .as_ref()
            .is_some_and(|r| r.rid == self.rid)
        {
            return;
        }
        let manual = app.check_updates_rid.take().unwrap().state;
        match self.result {
            Ok(release) => match semver::Version::parse(release.tag_name.trim_start_matches('v')) {
                Ok(version)
                    if version > semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap() =>
                {
                    app.app_update_status = format!("Update available: {}", release.tag_name);
                    app.available_update = Some(release);
                    if manual || !app.update_dismissed {
                        app.show_app_update = true;
                    }
                }
                Ok(_) => {
                    app.available_update = None;
                    app.app_update_status = "You are up to date.".into();
                }
                Err(_) => {
                    app.app_update_status =
                        "Could not check for updates: invalid release version. Try again later."
                            .into()
                }
            },
            Err(error) => {
                tracing::warn!("Application update check failed: {error}");
                app.app_update_status =
                    "Could not check for updates. Check your connection and try again.".into();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn integrate_async(
    store: Arc<ModStore>,
    ctx: egui::Context,
    mod_specs: Vec<ModSpecification>,
    fsd_pak: PathBuf,
    config: MetaConfig,
    rid: RequestID,
    message_tx: Sender<Message>,
    cancelled: Arc<AtomicBool>,
) -> Result<(), IntegrationError> {
    let update = false;

    let mods = tokio::select! {
        result = store.resolve_mods(&mod_specs, update) => result?,
        _ = wait_for_cancel(&cancelled) => return Err(IntegrationError::Cancelled),
    };

    let to_integrate = mod_specs
        .iter()
        .map(|u| mods[u].clone())
        .collect::<Vec<_>>();
    let res_map: HashMap<ModResolution, ModSpecification> = mods
        .iter()
        .map(|(spec, info)| (info.resolution.clone(), spec.clone()))
        .collect();
    let urls = to_integrate
        .iter()
        .map(|m| &m.resolution)
        .collect::<Vec<_>>();

    let (tx, mut rx) = mpsc::channel::<FetchProgress>(10);

    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            if let Some(spec) = res_map.get(progress.resolution()) {
                message_tx
                    .send(Message::FetchModProgress(FetchModProgress {
                        rid,
                        spec: spec.clone(),
                        progress: progress.into(),
                    }))
                    .await
                    .unwrap();
                ctx.request_repaint();
            }
        }
    });

    let paths = tokio::select! {
        result = store.fetch_mods_ordered(&urls, update, Some(tx)) => result?,
        _ = wait_for_cancel(&cancelled) => return Err(IntegrationError::Cancelled),
    };

    tokio::task::spawn_blocking(move || {
        crate::integrate::integrate_with_cancel(
            fsd_pak,
            config,
            to_integrate.into_iter().zip(paths).collect(),
            &cancelled,
        )
    })
    .await??;

    Ok(())
}

async fn wait_for_cancel(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[derive(Debug)]
pub struct LintMods {
    rid: RequestID,
    result: Result<LintReport, IntegrationError>,
}

impl LintMods {
    pub fn send(
        rc: &mut RequestCounter,
        store: Arc<ModStore>,
        mods: Vec<ModSpecification>,
        enabled_lints: BTreeSet<LintId>,
        game_pak_path: Option<PathBuf>,
        tx: Sender<Message>,
        ctx: egui::Context,
    ) -> MessageHandle<()> {
        let rid = rc.next();

        let handle = spawn_reported(rid, ctx.clone(), tx.clone(), async move {
            let paths_res =
                resolve_async_ordered(store, ctx.clone(), mods.clone(), rid, tx.clone()).await;
            let mod_path_pairs_res =
                paths_res.map(|paths| mods.into_iter().zip(paths).collect::<Vec<_>>());

            let report_res = match mod_path_pairs_res {
                Ok(pairs) => tokio::task::spawn_blocking(move || {
                    crate::mod_lints::run_lints(
                        &enabled_lints,
                        pairs.into_iter().collect(),
                        game_pak_path,
                    )
                })
                .await
                .map_err(IntegrationError::from)
                .and_then(|result| result.map_err(Into::into)),
                Err(e) => Err(e),
            };

            tx.send(Message::LintMods(
                LintMods {
                    rid,
                    result: report_res,
                }
                .into(),
            ))
            .await
            .unwrap();
            ctx.request_repaint();
        });

        MessageHandle {
            rid,
            handle,
            state: Default::default(),
            cancellation: None,
        }
    }

    fn receive(self, app: &mut App) {
        if Some(self.rid) == app.lint_rid.as_ref().map(|r| r.rid) {
            match self.result {
                Ok(report) => {
                    info!("lint mod report complete");
                    app.lint_report = Some(report);
                    app.last_action = Some(LastAction::success(
                        "Mod file checks complete. Open the report to review the findings."
                            .to_string(),
                    ));
                }
                Err(ref e)
                    if let IntegrationError::ProviderError { source } = e
                        && let ProviderError::NoProvider { url: _, factory } = source =>
                {
                    app.window_provider_parameters =
                        Some(WindowProviderParameters::new(factory, &app.state));
                    app.last_action = Some(LastAction::failure("no provider".to_string()));
                }
                Err(e) => {
                    error!("{}", e);
                    app.problematic_mod_id = e.opt_mod_id();
                    app.last_action =
                        Some(LastAction::failure(super::diagnostics::error_details(&e)));
                }
            }
            app.lint_rid = None;
        }
    }
}

async fn resolve_async_ordered(
    store: Arc<ModStore>,
    ctx: egui::Context,
    mod_specs: Vec<ModSpecification>,
    rid: RequestID,
    message_tx: Sender<Message>,
) -> Result<Vec<PathBuf>, IntegrationError> {
    let update = false;

    let mods = store.resolve_mods(&mod_specs, update).await?;

    let to_integrate = mod_specs
        .iter()
        .map(|u| mods[u].clone())
        .collect::<Vec<_>>();
    let res_map: HashMap<ModResolution, ModSpecification> = mods
        .iter()
        .map(|(spec, info)| (info.resolution.clone(), spec.clone()))
        .collect();
    let urls = to_integrate
        .iter()
        .map(|m| &m.resolution)
        .collect::<Vec<&ModResolution>>();

    let (tx, mut rx) = mpsc::channel::<FetchProgress>(10);

    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            if let Some(spec) = res_map.get(progress.resolution()) {
                message_tx
                    .send(Message::FetchModProgress(FetchModProgress {
                        rid,
                        spec: spec.clone(),
                        progress: progress.into(),
                    }))
                    .await
                    .unwrap();
                ctx.request_repaint();
            }
        }
    });

    Ok(store.fetch_mods_ordered(&urls, update, Some(tx)).await?)
}

#[derive(Debug)]
pub struct SelfUpdate {
    rid: RequestID,
    result: anyhow::Result<Option<PathBuf>>,
}

impl SelfUpdate {
    pub fn send(
        rc: &mut RequestCounter,
        tx: Sender<Message>,
        ctx: egui::Context,
        tag: &str,
        control: Arc<crate::app_update::Control>,
    ) -> MessageHandle<SelfUpdateProgress> {
        let rid = rc.next();
        let url = mint_lib::update::version_download_url(tag, crate::app_update::asset_name());
        #[cfg(test)]
        let url = std::env::var("MINT_NATIVE_UPDATE_URL").unwrap_or(url);
        MessageHandle {
            rid,
            handle: spawn_reported(rid, ctx.clone(), tx.clone(), async move {
                let progress_tx = tx.clone();
                let progress_ctx = ctx.clone();
                let result = crate::app_update::install(url, control, move |progress| {
                    let _ = progress_tx.try_send(Message::FetchSelfUpdateProgress(
                        FetchSelfUpdateProgress { rid, progress },
                    ));
                    progress_ctx.request_repaint();
                })
                .await;
                let _ = tx
                    .send(Message::SelfUpdate(SelfUpdate { rid, result }))
                    .await;
                ctx.request_repaint();
            }),
            state: SelfUpdateProgress::Downloading {
                downloaded: 0,
                total: None,
            },
            cancellation: None,
        }
    }

    fn receive(self, app: &mut App) {
        if !app
            .self_update_rid
            .as_ref()
            .is_some_and(|r| r.rid == self.rid)
        {
            return;
        }
        match self.result {
            Ok(Some(path)) => {
                tracing::info!("Application update installed; restart required");
                app.original_exe_path = Some(path);
                app.self_update_rid = None;
                app.self_update_control = None;
                app.app_update_status = "Update installed. Restart MINT to use it.".into();
                app.last_action = Some(LastAction::success(
                    "Application update installed. Restart MINT to use it.".into(),
                ));
            }
            Ok(None) => {
                app.self_update_rid = None;
                app.self_update_control = None;
                app.app_update_status = "Update cancelled. The application was not changed.".into();
            }
            Err(error) => {
                tracing::error!("Application update failed: {error:#}");
                app.self_update_rid = None;
                app.self_update_control = None;
                app.app_update_status =
                    "Update failed. See Details for the cause and retry instructions.".into();
                app.last_action = Some(LastAction::failure(format!(
                    "Application update failed: {error:#}"
                )));
            }
        }
    }
}

#[derive(Debug)]
pub struct FetchSelfUpdateProgress {
    rid: RequestID,
    progress: SelfUpdateProgress,
}

impl FetchSelfUpdateProgress {
    fn receive(self, app: &mut App) {
        if let Some(MessageHandle { rid, state, .. }) = &mut app.self_update_rid
            && *rid == self.rid
        {
            *state = self.progress;
        }
    }
}
