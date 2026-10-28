mod app_update;
mod details;
mod diagnostics;
mod drag_drop;
mod editing;
mod filters;
mod find_string;
mod fonts;
mod group_colors;
mod group_view;
mod groups;
mod history;
mod icons;
mod inputs;
mod installation;
mod lint;
mod log_window;
mod message;
mod mod_list;
mod named_combobox;
#[cfg(windows)]
mod native_popup;
mod notifications;
mod request_counter;
mod settings;
mod sorting;
mod thumbnails;
mod toggle_switch;
mod updates;

#[cfg(test)]
mod tests;

fn dismiss_popups(ctx: &egui::Context) {
    ctx.memory_mut(|memory| memory.close_popup());
    ctx.data_mut(|data| data.remove_by_type::<egui::menu::BarState>());
}

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, RangeInclusive};
use std::time::{Duration, Instant};
use std::{
    collections::{HashMap, HashSet},
    ops::DerefMut,
    path::PathBuf,
};

use eframe::egui::RichText;
use eframe::{
    egui::{FontSelection, Layout, TextFormat, Ui},
    emath::{Align, Align2},
    epaint::{Color32, Stroke, text::LayoutJob},
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use mint_lib::error::ResultExt as _;
use mint_lib::mod_info::RequiredStatus;
use mint_lib::update::GitHubRelease;
use strum::IntoEnumIterator;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};
use tracing::{debug, trace, warn};

use crate::Dirs;
use crate::gui::find_string::searchable_text;
use crate::mod_lints::{LintId, LintReport, SplitAssetPair};
use crate::providers::ProviderError;
use crate::state::SortingConfig;
use crate::{
    MintError,
    integrate::uninstall,
    is_drg_pak,
    providers::{
        ApprovalStatus, FetchProgress, ModInfo, ModSpecification, ModStore, ProviderFactory,
    },
    state::{ModConfig, ModData_v0_1_0 as ModData, ModOrGroup, ModProfile, State},
};
use icons::Icon;
use message::MessageHandle;
use request_counter::{RequestCounter, RequestID};

use self::toggle_switch::toggle_switch;
pub use crate::state::{GuiTheme, ModListView, PriorityDisplay, RowStriping, SortBy};
use settings::{WindowProviderParameters, WindowSettings};

pub fn gui(dirs: Dirs, args: Option<Vec<String>>) -> Result<(), MintError> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icons::app_icon())
            .with_inner_size([900.0, 500.0])
            .with_min_inner_size([480.0, 320.0])
            .with_drag_and_drop(true),
        // The main window's size, position and maximized state are restored from here and
        // clamped to the connected monitors. Kept in the data directory so `--appdata` isolates it.
        persist_window: true,
        persistence_path: Some(window_state_path(&dirs)),
        ..Default::default()
    };
    eframe::run_native(
        &format!("MINT Maintained {}", mint_lib::built_info::version()),
        options,
        Box::new(|cc| Ok(Box::new(StartupApp::new(cc, dirs, args)))),
    )
    .with_generic(|e| format!("{e}"))?;
    Ok(())
}

struct StartupApp {
    app: Option<App>,
    dirs: Dirs,
    args: Option<Vec<String>>,
    error: String,
}

impl StartupApp {
    fn new(cc: &eframe::CreationContext, dirs: Dirs, args: Option<Vec<String>>) -> Self {
        #[cfg(windows)]
        native_popup::install(cc);
        let result = App::new(cc, dirs.clone(), args.clone());
        let (app, error) = match result {
            Ok(app) => (Some(app), String::new()),
            Err(error) => (None, diagnostics::error_details(&error)),
        };
        Self {
            app,
            dirs,
            args,
            error,
        }
    }
}

/// eframe's window state file: only the main window geometry is stored in it.
fn window_state_path(dirs: &Dirs) -> PathBuf {
    dirs.data_dir.join("window.ron")
}

impl eframe::App for StartupApp {
    /// Dialog positions and other egui UI state are not restored between sessions.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.app {
            app.update(ctx, frame);
            return;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MINT could not load its saved data");
            ui.label("Your files have not been reset. Back up the affected file before repairing it, then retry. For a write error, check folder permissions and whether another application has locked the file.");
            ui.label(format!("Config: {}\nCache: {}", self.dirs.config_dir.display(), self.dirs.cache_dir.display()));
            ui.horizontal(|ui| {
                if ui.button("Open config folder").clicked() && let Err(error) = opener::open(&self.dirs.config_dir) { warn!("Could not open the config folder: {error}"); }
                if ui.button("Open cache folder").clicked() && let Err(error) = opener::open(&self.dirs.cache_dir) { warn!("Could not open the cache folder: {error}"); }
                if ui.button("Retry").clicked() {
                    match App::load(self.dirs.clone(), self.args.clone()) {
                        Ok(app) => self.app = Some(app),
                        Err(error) => self.error = diagnostics::error_details(&error),
                    }
                }
            });
            diagnostics::details_ui(ui, &self.error);
        });
    }
}

const MODIO_LOGO_PNG: &[u8] = include_bytes!("../../assets/modio-cog-blue.png");

pub struct App {
    args: Option<Vec<String>>,
    tx: Sender<message::Message>,
    rx: Receiver<message::Message>,
    state: State,
    resolve_mod: String,
    resolve_mod_rid: Option<MessageHandle<()>>,
    integrate_rid: Option<MessageHandle<HashMap<ModSpecification, SpecFetchProgress>>>,
    update_rid: Option<MessageHandle<()>>,
    check_updates_rid: Option<MessageHandle<bool>>,
    has_run_init: bool,
    request_counter: RequestCounter,
    window_provider_parameters: Option<WindowProviderParameters>,
    search_string: String,
    scroll_to_match: bool,
    focus_search: bool,
    settings_window: Option<WindowSettings>,
    modio_texture_handle: Option<egui::TextureHandle>,
    last_action: Option<LastAction>,
    notifications: notifications::Notifications,
    history: history::History,
    installation_status: installation::InstallationStatus,
    refresh_progress: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    refresh_total: usize,
    available_update: Option<GitHubRelease>,
    show_app_update: bool,
    update_dismissed: bool,
    app_update_status: String,
    self_update_control: Option<std::sync::Arc<crate::app_update::Control>>,
    open_profiles: HashSet<String>,
    lint_rid: Option<MessageHandle<()>>,
    lint_report_window: Option<WindowLintReport>,
    lint_report: Option<LintReport>,
    lints_toggle_window: Option<WindowLintsToggle>,
    lint_options: LintOptions,
    cache: CommonMarkCache,
    needs_restart: bool,
    close_after_update: bool,
    restart_arguments: Vec<std::ffi::OsString>,
    self_update_rid: Option<MessageHandle<SelfUpdateProgress>>,
    original_exe_path: Option<PathBuf>,
    problematic_mod_id: Option<u32>,
    show_error_details: bool,
    delete_confirmation: Option<editing::DeleteConfirmation>,
    groups_window: Option<groups::GroupsWindow>,
    log_window: Option<log_window::LogWindow>,
    group_views: group_view::GroupViews,
    /// Heights of mod rows, for skipping rows outside the visible part of the list.
    row_heights: mod_list::RowHeights,
    /// Duplicate and dependency index of the shown profile's enabled mods.
    dependency_index: Option<mod_list::DependencyIndex>,
    /// Whether rows outside the visible part of the list are skipped. Tests turn it off to
    /// compare with a fully drawn list.
    cull_rows: bool,
    details: Option<details::DetailsTarget>,
    /// Priority editor whose change is saved once it is released.
    priority_edit: Option<egui::Id>,
    /// Metadata facts for filters and search.
    facts: filters::FactsCache,
    mod_filters: filters::ModFilters,
    thumbnails: thumbnails::Thumbnails,
}

#[derive(Default)]
struct LintOptions {
    archive_with_multiple_paks: bool,
    archive_with_only_non_pak_files: bool,
    asset_register_bin: bool,
    conflicting: bool,
    empty_archive: bool,
    outdated_pak_version: bool,
    shader_files: bool,
    non_asset_files: bool,
    split_asset_pairs: bool,
    unmodified_game_assets: bool,
}

struct LastAction {
    timestamp: Instant,
    status: LastActionStatus,
    notified: bool,
    undo_id: Option<u64>,
    report: Option<std::sync::Arc<crate::providers::refresh::RefreshReport>>,
}
impl LastAction {
    fn success(msg: String) -> Self {
        Self {
            timestamp: Instant::now(),
            status: LastActionStatus::Success(msg),
            notified: false,
            undo_id: None,
            report: None,
        }
    }
    fn failure(msg: String) -> Self {
        // The footer is often the only report of a failure; keep it in the log as well.
        // Log output is redacted centrally by mint_lib.
        warn!("{msg}");
        Self {
            timestamp: Instant::now(),
            status: LastActionStatus::Failure(msg),
            notified: false,
            undo_id: None,
            report: None,
        }
    }
    fn timeago(&self) -> String {
        Self::timeago_after(Instant::now().duration_since(self.timestamp))
    }
    fn timeago_after(duration: Duration) -> String {
        let seconds = duration.as_secs();
        if seconds < 60 {
            format!("{seconds}s ago")
        } else if seconds < 3600 {
            format!("{}m ago", seconds / 60)
        } else {
            ">1h ago".into()
        }
    }
    /// Time until [`Self::timeago`] shows a different text, or `None` once it no longer changes.
    fn timeago_changes_in(&self) -> Option<Duration> {
        Self::timeago_changes_after(Instant::now().duration_since(self.timestamp))
    }
    fn timeago_changes_after(duration: Duration) -> Option<Duration> {
        let step = match duration.as_secs() {
            0..60 => 1,
            60..3600 => 60,
            _ => return None,
        };
        let next = (duration.as_secs() / step + 1) * step;
        Some(Duration::from_secs(next) - duration)
    }
}

enum LastActionStatus {
    Success(String),
    Failure(String),
}

impl App {
    /// Mod metadata or the installed mods may have changed, so cached facts are read again.
    pub(super) fn metadata_changed(&mut self) {
        self.installation_status.invalidate();
        self.facts.clear();
    }

    fn new(
        cc: &eframe::CreationContext,
        dirs: Dirs,
        args: Option<Vec<String>>,
    ) -> Result<Self, MintError> {
        fonts::install(&cc.egui_ctx);
        Self::load(dirs, args)
    }

    fn load(dirs: Dirs, args: Option<Vec<String>>) -> Result<Self, MintError> {
        let (tx, rx) = mpsc::channel(10);
        let state = State::init(dirs)?;
        let history = history::History::new(&state.mod_data);
        let thumbnails = thumbnails::Thumbnails::new(state.dirs.cache_dir.join("thumbnails"));

        Ok(Self {
            args,
            tx,
            rx,
            request_counter: Default::default(),
            state,
            resolve_mod: Default::default(),
            resolve_mod_rid: None,
            integrate_rid: None,
            update_rid: None,
            check_updates_rid: None,
            has_run_init: false,
            window_provider_parameters: None,
            search_string: Default::default(),
            scroll_to_match: false,
            focus_search: false,
            settings_window: None,
            modio_texture_handle: None,
            last_action: None,
            notifications: Default::default(),
            history,
            installation_status: Default::default(),
            refresh_progress: Default::default(),
            refresh_total: 0,
            available_update: None,
            show_app_update: false,
            update_dismissed: false,
            app_update_status: String::new(),
            self_update_control: None,
            open_profiles: Default::default(),
            lint_rid: None,
            lint_report_window: None,
            lint_report: None,
            lints_toggle_window: None,
            lint_options: LintOptions::default(),
            cache: Default::default(),
            needs_restart: false,
            close_after_update: false,
            restart_arguments: std::env::args_os().skip(1).collect(),
            self_update_rid: None,
            original_exe_path: None,
            problematic_mod_id: None,
            show_error_details: false,
            delete_confirmation: None,
            groups_window: None,
            log_window: None,
            group_views: Default::default(),
            row_heights: Default::default(),
            dependency_index: None,
            cull_rows: true,
            details: None,
            priority_edit: None,
            facts: Default::default(),
            mod_filters: Default::default(),
            thumbnails,
        })
    }

    /// Fetches details missing from older caches and trims the thumbnail cache, off the UI thread.
    fn start_metadata_maintenance(&self, ctx: &egui::Context) {
        let store = self.state.store.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            match store.backfill_details().await {
                Ok(true) => ctx.request_repaint(),
                Ok(false) => {}
                Err(error) => warn!("Could not fetch missing mod details: {error}"),
            }
        });
        let dir = self.state.dirs.cache_dir.join("thumbnails");
        tokio::task::spawn_blocking(move || {
            if let Err(error) = thumbnails::prune(&dir, thumbnails::MAX_CACHE_BYTES) {
                warn!("Could not trim the thumbnail cache: {error}");
            }
        });
    }

    fn report_save(&mut self, result: Result<(), crate::state::StateError>) -> bool {
        if let Err(error) = result {
            self.last_action = Some(LastAction::failure(format!(
                "Changes could not be saved. Keep the app open and correct the file permissions before trying again.\n{}",
                diagnostics::error_details(&error)
            )));
            false
        } else {
            true
        }
    }

    fn show_error_window(&mut self, ctx: &egui::Context) {
        let enabled = !self.editing_busy();
        let mut retry = None;
        if let Some(LastAction {
            status: LastActionStatus::Failure(details) | LastActionStatus::Success(details),
            report,
            ..
        }) = &mut self.last_action
        {
            *details = diagnostics::redact(
                details,
                self.state
                    .config
                    .provider_parameters
                    .values()
                    .flat_map(|parameters| parameters.values().cloned()),
            );
            if self.show_error_details {
                dialog(
                    if report.is_some() {
                        "Update check details"
                    } else {
                        "Error details"
                    },
                    ctx,
                    [520.0, 280.0],
                )
                .open(&mut self.show_error_details)
                .show(ctx, |ui| {
                    if let Some(report) = report {
                        retry = updates::report_ui(ui, report, enabled);
                    } else {
                        diagnostics::details_ui(ui, details);
                    }
                });
            }
        }
        if let Some(specs) = retry {
            self.show_error_details = false;
            message::UpdateCache::send_specs(self, ctx, specs);
        }
    }

    fn parse_mods(&self) -> Vec<ModSpecification> {
        self.resolve_mod
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| ModSpecification::new(l.to_string()))
            .collect()
    }

    fn build_mod_string(mods: &Vec<ModConfig>) -> String {
        let mut string = String::new();
        for m in mods {
            if m.enabled {
                string.push_str(&m.spec.url);
                string.push('\n');
            }
        }
        string
    }

    fn show_profile_windows(&mut self, ctx: &egui::Context) {
        let mut to_remove = vec![];
        for profile in &self.open_profiles.clone() {
            let mut open = true;
            dialog(format!("Profile \"{profile}\""), ctx, [640.0, 420.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    self.ui_profile(ui, profile);
                });
            if !open {
                to_remove.push(profile.clone());
            }
        }
        for r in to_remove {
            self.open_profiles.remove(&r);
        }
    }
}

struct WindowLintReport;

struct WindowLintsToggle;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| {
            !i.focused
                || i.events
                    .iter()
                    .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        }) {
            dismiss_popups(ctx);
        }
        if self.needs_restart
            && let Some(original_exe_path) = self.original_exe_path.clone()
        {
            debug!("needs restart");
            self.needs_restart = false;

            if !self.report_save(self.state.config.save()) {
                return;
            }

            debug!("restarting...");
            match std::process::Command::new(original_exe_path)
                .args(&self.restart_arguments)
                .spawn()
            {
                Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(error) => {
                    self.last_action = Some(LastAction::failure(format!(
                        "Could not restart MINT: {error}. Close and reopen the application manually."
                    )))
                }
            }
        }

        // do some init things that depend on ctx so cannot be done earlier
        if !self.has_run_init {
            self.has_run_init = true;

            let theme = GuiTheme::into_egui_theme(self.state.config.gui_theme);
            ctx.memory_mut(|m| m.options.theme_preference = theme);

            message::CheckUpdates::send(self, ctx, false);
            self.start_metadata_maintenance(ctx);
        }

        // message handling
        let mut handled = false;
        while let Ok(msg) = self.rx.try_recv() {
            msg.handle(self);
            handled = true;
        }
        // Background work is what changes mod metadata and the installed mods.
        if handled {
            self.metadata_changed();
        }

        if ctx.input(|input| input.viewport().close_requested()) && self.self_update_rid.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_after_update = true;
            if let Some(control) = &self.self_update_control {
                control.cancel();
            }
        }
        if self.close_after_update && self.self_update_rid.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // begin draw
        self.thumbnails.poll(ctx);
        self.history_shortcuts(ctx);
        let busy = self.editing_busy();
        self.installation_status.poll(&self.state, ctx, busy);

        self.show_update_window(ctx);
        self.show_provider_parameters(ctx);
        self.show_profile_windows(ctx);
        self.show_settings(ctx);
        self.show_lints_toggle(ctx);
        self.show_lint_report(ctx);
        self.show_error_window(ctx);
        self.show_groups(ctx);

        let footer = egui::TopBottomPanel::bottom("bottom_panel").frame(
            egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(8, 8)),
        );
        footer.show(ctx, |ui| {
            self.status_row(ui);
            ui.add(egui::Separator::default().spacing(6.0));
            let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            let layout = egui::Layout::right_to_left(Align::Center);
            ui.allocate_ui_with_layout(size, layout, |ui| {
                ui.add_enabled_ui(
                    self.integrate_rid.is_none()
                        && self.update_rid.is_none()
                        && self.lint_rid.is_none()
                        && self.self_update_rid.is_none()
                        && self.state.config.drg_pak_path.is_some(),
                    |ui| {
                        if let Some(args) = &self.args
                            && ui
                                .button("Launch game")
                                .on_hover_ui(|ui| {
                                    for arg in args {
                                        ui.label(arg);
                                    }
                                })
                                .clicked()
                        {
                            let args = args.clone();
                            std::thread::spawn(move || {
                                let mut iter = args.iter();
                                std::process::Command::new(iter.next().unwrap())
                                    .args(iter)
                                    .spawn()
                                    .unwrap()
                                    .wait()
                                    .unwrap();
                            });
                        }

                        ui.add_enabled_ui(self.state.config.drg_pak_path.is_some(), |ui| {
                            // Highlighted while the active profile differs from the installed mods.
                            let pending = self.installation_status.needs_install;
                            let mut button = icons::primary_button(ui, "Install mods", pending);
                            if pending {
                                button = button.on_hover_text(
                                    "Profile changes are not installed yet. Click to apply them.",
                                );
                            }
                            if self.state.config.drg_pak_path.is_none() {
                                button = button.on_disabled_hover_text(
                                    "DRG install not found. Configure it in the settings menu.",
                                );
                            }

                            if button.clicked() {
                                let active_profile = self.state.mod_data.active_profile.clone();
                                let mods =
                                    match self.state.mod_data.enabled_mods_ordered(&active_profile)
                                    {
                                        Ok(mods) => mods,
                                        Err(error) => {
                                            self.last_action = Some(LastAction::failure(
                                                diagnostics::error_details(&error),
                                            ));
                                            return;
                                        }
                                    };

                                self.last_action = None;
                                self.integrate_rid = Some(message::Integrate::send(
                                    &mut self.request_counter,
                                    self.state.store.clone(),
                                    mods,
                                    self.state.config.drg_pak_path.as_ref().unwrap().clone(),
                                    self.state.config.deref().into(),
                                    self.tx.clone(),
                                    ctx.clone(),
                                ));
                                self.problematic_mod_id = None;
                            }
                        });

                        ui.add_enabled_ui(self.state.config.drg_pak_path.is_some(), |ui| {
                            let mut button = ui.button("Uninstall mods");
                            if self.state.config.drg_pak_path.is_none() {
                                button = button.on_disabled_hover_text(
                                    "DRG install not found. Configure it in the settings menu.",
                                );
                            }
                            if button.clicked() {
                                self.last_action = None;
                                if let Some(pak_path) = &self.state.config.drg_pak_path {
                                    let mut mods = HashSet::default();
                                    let active_profile = self.state.mod_data.active_profile.clone();
                                    self.state.mod_data.for_each_enabled_mod(
                                        &active_profile,
                                        |mc| {
                                            if let Some(modio_id) = self
                                                .state
                                                .store
                                                .get_mod_info(&mc.spec)
                                                .and_then(|i| i.modio_id)
                                            {
                                                mods.insert(modio_id);
                                            }
                                        },
                                    );

                                    debug!("uninstalling mods: pak_path = {}", pak_path.display());
                                    self.last_action = Some(match uninstall(pak_path, mods) {
                                        Ok(()) => LastAction::success(
                                            "Successfully uninstalled mods".to_string(),
                                        ),
                                        Err(e) => LastAction::failure(format!(
                                            "Failed to uninstall mods: {e}"
                                        )),
                                    })
                                }
                            }
                        });
                    },
                );
                ui.add_enabled_ui(!self.editing_busy(), |ui| {
                    ui.menu_button("Check mod updates", |ui| {
                        if ui.button("Current profile").clicked() {
                            message::UpdateCache::send_specs(
                                self,
                                ctx,
                                self.profile_update_specs(),
                            );
                            ui.close_menu();
                        }
                        if ui.button("All cached mods").clicked() {
                            message::UpdateCache::send(self, ctx);
                            ui.close_menu();
                        }
                    });
                });
                if let Some(integration) = &self.integrate_rid {
                    let cancelled = integration.cancellation.as_ref().unwrap();
                    if ui
                        .add_enabled(
                            !cancelled.load(std::sync::atomic::Ordering::Acquire),
                            egui::Button::new("Cancel"),
                        )
                        .clicked()
                    {
                        cancelled.store(true, std::sync::atomic::Ordering::Release);
                    }
                    ui.spinner();
                }
                if let Some(update) = &self.update_rid {
                    let cancelling = update
                        .cancellation
                        .as_ref()
                        .is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire));
                    if ui
                        .add_enabled(
                            !cancelling && !update.handle.is_finished(),
                            egui::Button::new(if cancelling {
                                "Cancelling..."
                            } else {
                                "Cancel"
                            }),
                        )
                        .clicked()
                        && let Some(flag) = update.cancellation.as_ref()
                    {
                        flag.store(true, std::sync::atomic::Ordering::Release);
                    }
                    ui.spinner();
                }
                if ui.button("Check mod files").clicked() {
                    self.lints_toggle_window = Some(WindowLintsToggle);
                }
                if icons::button(ui, Icon::Settings, "Open settings").clicked() {
                    self.settings_window = Some(WindowSettings::new(&self.state));
                }
                if let Some(available_update) = &self.available_update
                    && icons::button(
                        ui,
                        Icon::Warning,
                        &format!(
                            "Update available: {}\n{}",
                            available_update.tag_name, available_update.html_url
                        ),
                    )
                    .clicked()
                {
                    self.show_app_update = true;
                }
            });
        });
        self.show_details_panel(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.editing_busy() || self.delete_confirmation.is_some() {
                ui.disable();
            }
            // profile selection

            let mut open_groups = false;
            let mut delete_profile = false;
            let mut copied_count = None;
            let mut history_request = None;
            let history_enabled = self.history_available();
            let history = &self.history;
            let original_profile = self.state.mod_data.active_profile.clone();
            let buttons = |ui: &mut Ui, mod_data: &mut ModData| {
                if ui.button("Groups").clicked() {
                    open_groups = true;
                }
                if icons::button(ui, Icon::Copy, "Copy profile mods").clicked() {
                    let mut mods = Vec::new();
                    let active_profile = mod_data.active_profile.clone();
                    mod_data.for_each_enabled_mod(&active_profile, |mc| {
                        mods.push(mc.clone());
                    });
                    copied_count = Some(mods.len());
                    if !mods.is_empty() {
                        ui.ctx().copy_text(Self::build_mod_string(&mods));
                    }
                }
                history_request = history.controls(ui, history_enabled);

                // TODO find better icon, flesh out multiple-view usage, fix GUI locking
                /*
                if ui
                    .button("pop out")
                    .on_hover_text_at_pointer("pop out")
                    .clicked()
                {
                    self.open_profiles.insert(mod_data.active_profile.clone());
                }
                */
            };

            if let Some(change) = named_combobox::ui(
                ui,
                "profile",
                self.state.mod_data.deref_mut().deref_mut(),
                &mut delete_profile,
                Some(buttons),
            ) {
                let text = change.notification();
                let saved = self.save_mod_data(text.as_deref().unwrap_or("Select profile"));
                if saved && let Some(text) = text {
                    self.notify_edit(text);
                }
            }
            if let Some(count) = copied_count {
                self.notifications.success(if count == 0 {
                    "No active mod links to copy.".to_owned()
                } else {
                    format!(
                        "Copied {count} active mod {}.\nDisabled mods and groups were excluded.",
                        if count == 1 { "link" } else { "links" }
                    )
                });
            }
            if let Some(redo) = history_request {
                self.restore_history(redo, None);
            }
            if open_groups {
                self.groups_window = Some(groups::GroupsWindow::default());
            }
            if delete_profile {
                self.request_edit(
                    crate::state::edits::Edit::DeleteProfile(original_profile),
                    editing::skip_delete_confirmation(ctx),
                );
            }

            ui.separator();

            ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                if self.resolve_mod_rid.is_some() {
                    ui.spinner();
                }
                ui.with_layout(ui.layout().with_main_justify(true), |ui| {
                    // define multiline layouter to be able to show multiple lines in a single line widget
                    let font_id = FontSelection::default().resolve(ui.style());
                    let text_color = ui.visuals().widgets.inactive.text_color();
                    let mut multiline_layouter = move |ui: &Ui, text: &str, wrap_width: f32| {
                        let layout_job = LayoutJob::simple(
                            text.to_string(),
                            font_id.clone(),
                            text_color,
                            wrap_width,
                        );
                        ui.fonts(|f| f.layout_job(layout_job))
                    };

                    let resolve = ui.add_enabled(
                        self.resolve_mod_rid.is_none(),
                        inputs::bordered(
                            egui::TextEdit::singleline(&mut self.resolve_mod)
                                .layouter(&mut multiline_layouter)
                                .hint_text("Add mod..."),
                        ),
                    );
                    if is_committed(&resolve) {
                        message::ResolveMods::send(self, ctx, self.parse_mods(), false);
                        self.problematic_mod_id = None;
                    }
                });
            });

            let profile = self.state.mod_data.active_profile.clone();

            ui.horizontal(|ui| {
                self.ui_sort_controls(ui);
                ui.add_space(16.);
                self.ui_view_controls(ui);

                ui.add_space(16.);
                let any_matches = self.search_has_matches(&profile, &self.search_string);
                let search_string = &mut self.search_string;

                let mut text_edit = egui::TextEdit::singleline(search_string)
                    .hint_text("Search")
                    .desired_width(ui.available_width());
                if !any_matches {
                    text_edit = text_edit.text_color(ui.visuals().error_fg_color);
                }
                let res = ui
                    .scope_builder(
                        egui::UiBuilder::new().layout(egui::Layout::bottom_up(Align::RIGHT)),
                        |ui| ui.add(inputs::bordered(text_edit)),
                    )
                    .inner;
                if res.changed() {
                    self.scroll_to_match = true;
                }
                if res.lost_focus()
                    && ui.input(|i| {
                        i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)
                    })
                {
                    *search_string = String::new();
                    self.scroll_to_match = false;
                } else if self.focus_search {
                    res.request_focus();
                    self.focus_search = false;
                }
            });
            self.ui_filter_chips(ui, &profile);
            ui.add_space(4.);

            if self
                .state
                .mod_data
                .profiles
                .get(&profile)
                .is_some_and(|p| p.mods.is_empty())
            {
                ui.weak("This profile has no mods yet. Paste a mod.io link, URL or file path into Add mod, or drop .pak files onto this window.");
            }
            self.ui_profile(ui, &profile);

            // must access memory outside of input lock to prevent deadlock
            let is_anything_focused = ctx.memory(|m| m.focused().is_some());
            ctx.input(|i| {
                if self.delete_confirmation.is_some() || self.editing_busy() || self.show_app_update
                {
                    return;
                }
                if !i.raw.dropped_files.is_empty()
                    && self.integrate_rid.is_none()
                    && self.update_rid.is_none()
                {
                    let mut mods = String::new();
                    for f in i
                        .raw
                        .dropped_files
                        .iter()
                        .filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy()))
                    {
                        mods.push_str(&f);
                        mods.push('\n');
                    }

                    self.resolve_mod = mods.trim().to_string();
                    message::ResolveMods::send(self, ctx, self.parse_mods(), false);
                    self.problematic_mod_id = None;
                }
                for e in &i.events {
                    match e {
                        egui::Event::Paste(s) => {
                            if self.integrate_rid.is_none()
                                && self.update_rid.is_none()
                                && self.lint_rid.is_none()
                                && !is_anything_focused
                            {
                                self.resolve_mod = s.trim().to_string();
                                message::ResolveMods::send(self, ctx, self.parse_mods(), false);
                            }
                        }
                        egui::Event::Text(text) if !is_anything_focused => {
                            self.search_string = text.to_string();
                            self.scroll_to_match = true;
                            self.focus_search = true;
                        }
                        _ => {}
                    }
                }
            });
        });
        self.show_delete_confirmation(ctx);
        if !self.show_app_update {
            self.show_notifications(ctx);
        }
        if let Some(mut window) = self.log_window.take()
            && window.show(ctx)
        {
            self.log_window = Some(window);
        }
    }
}

/// Shared dialog behavior: movable and resizable inside the main window, never collapsed to
/// its title, and scrolling instead of being cut off when the main window is smaller.
/// Confirmation prompts use `egui::Modal` instead.
fn dialog<'a>(
    title: impl Into<egui::WidgetText>,
    ctx: &egui::Context,
    default_size: [f32; 2],
) -> egui::Window<'a> {
    let available = ctx.screen_rect().size() - egui::vec2(32.0, 64.0);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .constrain(true)
        .vscroll(true)
        .min_size([240.0, 120.0])
        .default_size([
            default_size[0].min(available.x).max(240.0),
            default_size[1].min(available.y).max(120.0),
        ])
}

fn is_committed(res: &egui::Response) -> bool {
    res.lost_focus() && res.ctx.input(|i| i.key_pressed(egui::Key::Enter))
}

/// Name-entry popups stay open while editing and close on outside clicks.
fn custom_popup_above_or_below_widget<R>(
    ui: &Ui,
    popup_id: egui::Id,
    widget_response: &egui::Response,
    above_or_below: egui::AboveOrBelow,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    if ui.memory(|mem| mem.is_popup_open(popup_id)) {
        let (pos, pivot) = match above_or_below {
            egui::AboveOrBelow::Above => (widget_response.rect.left_top(), Align2::LEFT_BOTTOM),
            egui::AboveOrBelow::Below => (widget_response.rect.left_bottom(), Align2::LEFT_TOP),
        };

        let inner = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .constrain(true)
            .fixed_pos(pos)
            .pivot(pivot)
            .show(ui.ctx(), |ui| {
                // Note: we use a separate clip-rect for this area, so the popup can be outside the parent.
                // See https://github.com/emilk/egui/issues/825
                let frame = egui::Frame::popup(ui.style());
                let frame_margin = frame.total_margin();
                frame
                    .show(ui, |ui| {
                        ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                            ui.set_width(widget_response.rect.width() - frame_margin.sum().x);
                            add_contents(ui)
                        })
                        .inner
                    })
                    .inner
            });

        if ui.input(|i| {
            i.key_pressed(egui::Key::Escape)
                || (i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| {
                        !widget_response.rect.contains(p) && !inner.response.rect.contains(p)
                    }))
        }) {
            ui.memory_mut(|mem| mem.close_popup());
        }
        Some(inner.inner)
    } else {
        None
    }
}

#[derive(Debug)]
pub enum SpecFetchProgress {
    Progress { progress: u64, size: u64 },
    Complete,
}

impl From<FetchProgress> for SpecFetchProgress {
    fn from(value: FetchProgress) -> Self {
        match value {
            FetchProgress::Progress { progress, size, .. } => Self::Progress { progress, size },
            FetchProgress::Complete { .. } => Self::Complete,
        }
    }
}

use crate::app_update::Progress as SelfUpdateProgress;
