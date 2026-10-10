mod diagnostics;
mod drag_drop;
mod editing;
mod find_string;
mod group_view;
mod groups;
mod icons;
mod inputs;
mod lint;
mod log_window;
mod message;
mod mod_list;
mod named_combobox;
#[cfg(windows)]
mod native_popup;
mod request_counter;
mod settings;
mod sorting;
mod toggle_switch;

#[cfg(test)]
mod tests;

fn dismiss_popups(ctx: &egui::Context) {
    ctx.memory_mut(|memory| memory.close_popup());
    ctx.data_mut(|data| data.remove_by_type::<egui::menu::BarState>());
}

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, RangeInclusive};
use std::time::{Duration, Instant, SystemTime};
use std::{
    collections::{HashMap, HashSet},
    ops::DerefMut,
    path::PathBuf,
};

use eframe::egui::{CollapsingHeader, RichText};
use eframe::epaint::{Pos2, Vec2};
use eframe::{
    egui::{FontSelection, Layout, TextFormat, Ui},
    emath::{Align, Align2},
    epaint::{Color32, Stroke, text::LayoutJob},
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use mint_lib::error::ResultExt as _;
use mint_lib::mod_info::{ModioTags, RequiredStatus};
use mint_lib::update::GitHubRelease;
use strum::IntoEnumIterator;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};
use tracing::{debug, trace};

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
pub use crate::state::{GuiTheme, SortBy};
use settings::{WindowProviderParameters, WindowSettings};
use sorting::sorted_mod_indices;

pub fn gui(dirs: Dirs, args: Option<Vec<String>>) -> Result<(), MintError> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 500.0])
            .with_drag_and_drop(true),
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

impl eframe::App for StartupApp {
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
                if ui.button("Open config folder").clicked() { let _ = opener::open(&self.dirs.config_dir); }
                if ui.button("Open cache folder").clicked() { let _ = opener::open(&self.dirs.cache_dir); }
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
    check_updates_rid: Option<MessageHandle<()>>,
    has_run_init: bool,
    request_counter: RequestCounter,
    window_provider_parameters: Option<WindowProviderParameters>,
    search_string: String,
    scroll_to_match: bool,
    focus_search: bool,
    settings_window: Option<WindowSettings>,
    modio_texture_handle: Option<egui::TextureHandle>,
    last_action: Option<LastAction>,
    available_update: Option<GitHubRelease>,
    show_update_time: Option<SystemTime>,
    open_profiles: HashSet<String>,
    lint_rid: Option<MessageHandle<()>>,
    lint_report_window: Option<WindowLintReport>,
    lint_report: Option<LintReport>,
    lints_toggle_window: Option<WindowLintsToggle>,
    lint_options: LintOptions,
    cache: CommonMarkCache,
    needs_restart: bool,
    self_update_rid: Option<MessageHandle<SelfUpdateProgress>>,
    original_exe_path: Option<PathBuf>,
    problematic_mod_id: Option<u32>,
    show_error_details: bool,
    delete_confirmation: Option<editing::DeleteConfirmation>,
    groups_window: Option<groups::GroupsWindow>,
    log_window: Option<log_window::LogWindow>,
    group_views: group_view::GroupViews,
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
}
impl LastAction {
    fn success(msg: String) -> Self {
        Self {
            timestamp: Instant::now(),
            status: LastActionStatus::Success(msg),
        }
    }
    fn failure(msg: String) -> Self {
        Self {
            timestamp: Instant::now(),
            status: LastActionStatus::Failure(msg),
        }
    }
    fn timeago(&self) -> String {
        let duration = Instant::now().duration_since(self.timestamp);
        let seconds = duration.as_secs();
        if seconds < 60 {
            format!("{seconds}s ago")
        } else if seconds < 3600 {
            format!("{}m ago", seconds / 60)
        } else {
            ">1h ago".into()
        }
    }
}

enum LastActionStatus {
    Success(String),
    Failure(String),
}

impl App {
    fn new(
        _cc: &eframe::CreationContext,
        dirs: Dirs,
        args: Option<Vec<String>>,
    ) -> Result<Self, MintError> {
        Self::load(dirs, args)
    }

    fn load(dirs: Dirs, args: Option<Vec<String>>) -> Result<Self, MintError> {
        let (tx, rx) = mpsc::channel(10);
        let state = State::init(dirs)?;

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
            available_update: None,
            show_update_time: None,
            open_profiles: Default::default(),
            lint_rid: None,
            lint_report_window: None,
            lint_report: None,
            lints_toggle_window: None,
            lint_options: LintOptions::default(),
            cache: Default::default(),
            needs_restart: false,
            self_update_rid: None,
            original_exe_path: None,
            problematic_mod_id: None,
            show_error_details: false,
            delete_confirmation: None,
            groups_window: None,
            log_window: None,
            group_views: Default::default(),
        })
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
        if let Some(LastAction {
            status: LastActionStatus::Failure(details),
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
                egui::Window::new("Error details")
                    .open(&mut self.show_error_details)
                    .default_size([520.0, 280.0])
                    .show(ctx, |ui| diagnostics::details_ui(ui, details));
            }
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

    fn show_update_window(&mut self, ctx: &egui::Context) {
        if let (Some(update), Some(update_time)) =
            (self.available_update.as_ref(), self.show_update_time)
        {
            let now = SystemTime::now();
            let wait_time = Duration::from_secs(10);
            egui::Area::new("available-update-overlay".into())
                .movable(false)
                .fixed_pos(Pos2::ZERO)
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 127))
                        .show(ui, |ui| {
                            ui.allocate_space(ui.available_size());
                        })
                });
            if let Some(MessageHandle { state, .. }) = &self.self_update_rid {
                egui::Window::new("Update progress")
                    .collapsible(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(Align::Center), |ui| {
                            match state {
                                SelfUpdateProgress::Pending => {
                                    ui.add(egui::ProgressBar::new(0.0).show_percentage());
                                }
                                SelfUpdateProgress::Progress { progress, size } => {
                                    ui.add(
                                        egui::ProgressBar::new(*progress as f32 / *size as f32)
                                            .show_percentage(),
                                    );
                                }
                                SelfUpdateProgress::Complete => {
                                    ui.add(egui::ProgressBar::new(1.0).show_percentage());
                                    ui.label(
                                        egui::RichText::new("Update successful.")
                                            .color(Color32::LIGHT_GREEN),
                                    );

                                    if ui.button("Restart").clicked() {
                                        self.needs_restart = true;
                                    }
                                }
                            };
                        });
                    });
            } else {
                egui::Window::new(format!("Update available: {}", update.tag_name))
                    .collapsible(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .resizable(false)
                    .show(ctx, |ui| {
                        CommonMarkViewer::new().max_image_width(Some(512)).show(
                            ui,
                            &mut self.cache,
                            &update.body,
                        );
                        ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                            if ui
                                .add(egui::Button::new("Install update"))
                                .on_hover_text("Download and install the update.")
                                .clicked()
                            {
                                self.self_update_rid = Some(message::SelfUpdate::send(
                                    &mut self.request_counter,
                                    self.tx.clone(),
                                    ctx.clone(),
                                ));
                            }

                            let elapsed = now.duration_since(update_time).unwrap_or_default();
                            if elapsed > wait_time {
                                if ui.button("Close").clicked() {
                                    self.show_update_time = None;
                                }
                            } else {
                                ui.spinner();
                            }
                        });
                    });
            }
        }
    }

    fn show_profile_windows(&mut self, ctx: &egui::Context) {
        let mut to_remove = vec![];
        for profile in &self.open_profiles.clone() {
            let mut open = true;
            egui::Window::new(format!("Profile \"{profile}\""))
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
            && let Some(original_exe_path) = &self.original_exe_path
        {
            debug!("needs restart");
            self.needs_restart = false;

            debug!("restarting...");
            let _child = std::process::Command::new(original_exe_path)
                .spawn()
                .unwrap();
            debug!("created child process");

            std::process::exit(0);
        }

        // do some init things that depend on ctx so cannot be done earlier
        if !self.has_run_init {
            self.has_run_init = true;

            let theme = GuiTheme::into_egui_theme(self.state.config.gui_theme);
            ctx.memory_mut(|m| m.options.theme_preference = theme);

            message::CheckUpdates::send(self, ctx);
        }

        // message handling
        while let Ok(msg) = self.rx.try_recv() {
            msg.handle(self);
        }

        // begin draw

        self.show_update_window(ctx);
        self.show_provider_parameters(ctx);
        self.show_profile_windows(ctx);
        self.show_settings(ctx);
        self.show_lints_toggle(ctx);
        self.show_lint_report(ctx);
        self.show_error_window(ctx);
        self.show_groups(ctx);

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
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
                            let mut button = ui.button("Install mods");
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

                        if ui
                            .button("Update cache")
                            .on_hover_text(
                                "Checks for updates for all mods and updates local cache",
                            )
                            .clicked()
                        {
                            message::UpdateCache::send(self, ctx);
                            self.problematic_mod_id = None;
                        }
                    },
                );
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
                if self.update_rid.is_some() {
                    if ui.button("Cancel").clicked() {
                        self.update_rid.take().unwrap().handle.abort();
                    }
                    ui.spinner();
                }
                if ui
                    .button("Lint mods")
                    .on_hover_text("Lint mods in the current profile")
                    .clicked()
                {
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
                    ui.ctx()
                        .open_url(egui::OpenUrl::new_tab(&available_update.html_url));
                }
                ui.with_layout(egui::Layout::left_to_right(Align::TOP), |ui| {
                    if let Some(last_action) = &self.last_action {
                        let msg = match &last_action.status {
                            LastActionStatus::Success(msg) => {
                                ui.label(
                                    egui::RichText::new("STATUS")
                                        .color(Color32::BLACK)
                                        .background_color(Color32::LIGHT_GREEN),
                                );
                                msg
                            }
                            LastActionStatus::Failure(msg) => {
                                ui.label(
                                    egui::RichText::new("STATUS")
                                        .color(Color32::BLACK)
                                        .background_color(Color32::LIGHT_RED),
                                );
                                msg
                            }
                        };
                        ui.ctx().request_repaint(); // for continuously updating time
                        if matches!(last_action.status, LastActionStatus::Failure(_))
                            && ui.button("Details").clicked()
                        {
                            self.show_error_details = true;
                        }
                        ui.add(
                            egui::Label::new(format!(
                                "({}): {}",
                                last_action.timeago(),
                                msg.lines().next().unwrap_or_default()
                            ))
                            .truncate(),
                        );
                    }
                });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.editing_busy() || self.delete_confirmation.is_some() {
                ui.disable();
            }
            // profile selection

            let mut open_groups = false;
            let mut delete_profile = false;
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
                    let mods = Self::build_mod_string(&mods);
                    ui.ctx().copy_text(mods);
                }

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

            if named_combobox::ui(
                ui,
                "profile",
                self.state.mod_data.deref_mut().deref_mut(),
                &mut delete_profile,
                Some(buttons),
            ) {
                self.report_save(self.state.mod_data.save());
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
                ui.label("Sort by: ");

                let (mut sort_category, mut is_ascending) = self
                    .get_sorting_config()
                    .map(|c| (Some(c.sort_category), c.is_ascending))
                    .unwrap_or_default();

                let mut clicked = ui.radio_value(&mut sort_category, None, "Manual").clicked();
                for category in SortBy::iter() {
                    let mut radio_label = category.as_str().to_owned();
                    if sort_category == Some(category) {
                        radio_label.push_str(if is_ascending { " ⏶" } else { " ⏷" });
                    }
                    let resp = ui.radio_value(&mut sort_category, Some(category), radio_label);
                    if resp.clicked() {
                        clicked = true;
                        if resp.changed() {
                            is_ascending = true;
                        } else {
                            is_ascending = !is_ascending;
                        }
                    };
                }
                if clicked {
                    self.update_sorting_config(sort_category, is_ascending);
                }

                ui.add_space(16.);
                let search_string = &mut self.search_string;
                let lower = search_string.to_lowercase();
                let any_matches = self.state.mod_data.any_mod(&profile, |mc, _| {
                    self.state
                        .store
                        .get_mod_info(&mc.spec)
                        .map(|i| i.name.to_lowercase().contains(&lower))
                        .unwrap_or(false)
                });

                let mut text_edit = egui::TextEdit::singleline(search_string).hint_text("Search");
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
            ui.add_space(4.);

            self.ui_profile(ui, &profile);

            // must access memory outside of input lock to prevent deadlock
            let is_anything_focused = ctx.memory(|m| m.focused().is_some());
            ctx.input(|i| {
                if self.delete_confirmation.is_some() || self.editing_busy() {
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
        if let Some(mut window) = self.log_window.take()
            && window.show(ctx)
        {
            self.log_window = Some(window);
        }
    }
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

#[derive(Debug, PartialEq)]
pub enum SelfUpdateProgress {
    Pending,
    Progress { progress: u64, size: u64 },
    Complete,
}
