//! Right-side panel with the selected mod's cached metadata, priority and actions.

use super::*;
use crate::state::edits::ListTarget;

const DEFAULT_WIDTH: f32 = 300.0;
const PANEL_ID: &str = "mod-details";
const WIDTH_RANGE: RangeInclusive<f32> = 240.0..=560.0;

/// Calendar date (UTC) of a Unix timestamp, as shown for mod versions and updates.
pub(super) fn format_date(secs: u64) -> Option<String> {
    chrono::DateTime::from_timestamp(i64::try_from(secs).ok()?, 0)
        .map(|date| date.format("%Y-%m-%d").to_string())
}

const STATUS_ENABLED: Color32 = Color32::from_rgb(34, 197, 94);
const STATUS_GROUP_OFF: Color32 = Color32::from_rgb(245, 158, 11);

/// The panel may take at most 45% of the window, so the list keeps room in narrow windows.
pub(super) fn panel_width_range(window_width: f32) -> RangeInclusive<f32> {
    let max = (window_width * 0.45).clamp(160.0, *WIDTH_RANGE.end());
    WIDTH_RANGE.start().min(max)..=max
}

/// Width of the panel's scrolling content: the panel's width minus a constant gutter for the
/// floating scroll bar, the same whether or not the bar is shown.
pub(super) fn content_width(ui: &Ui) -> f32 {
    let scroll = &ui.spacing().scroll;
    (ui.available_width() - scroll.bar_width - scroll.bar_inner_margin - scroll.bar_outer_margin)
        .max(0.0)
}

const FOCUS_ID: &str = "mod-list-focus-last-frame";

/// Records which control has keyboard focus at the end of the list, for the next frame.
pub(super) fn remember_focus(ctx: &egui::Context) {
    let focused = ctx.memory(|memory| memory.focused());
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(FOCUS_ID), focused));
}

fn focus_last_frame(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.data(|data| data.get_temp::<Option<egui::Id>>(egui::Id::new(FOCUS_ID)))
        .flatten()
}

/// Small uppercase title that starts a panel section.
fn section(ui: &mut Ui, title: &str) {
    ui.add_space(14.0);
    ui.label(
        RichText::new(title.to_uppercase())
            .small()
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(2.0);
}

/// Faint rounded box that groups related fields, outlined so it stays visible in light themes.
fn card(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

fn field(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).weak());
}

/// A label in the style of the list's colored tags.
fn badge(ui: &mut Ui, text: &str, fill: Color32) {
    mod_list::tag_chip(ui, RichText::new(text).color(Color32::BLACK), Some(fill));
}

/// The selected row. `url` identifies the entry if its stored index moves or disappears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DetailsTarget {
    pub target: ListTarget,
    pub url: String,
}

impl App {
    pub(super) fn entry(
        &self,
        profile: &str,
        group: Option<&str>,
        index: usize,
    ) -> Option<&ModConfig> {
        match group {
            Some(group) => self.state.mod_data.groups.get(group)?.mods.get(index),
            None => match self.state.mod_data.profiles.get(profile)?.mods.get(index)? {
                ModOrGroup::Individual(mc) => Some(mc),
                ModOrGroup::Group { .. } => None,
            },
        }
    }

    fn entry_mut(&mut self, target: &ListTarget) -> Option<&mut ModConfig> {
        let data = self.state.mod_data.deref_mut().deref_mut();
        match &target.group {
            Some(group) => data.groups.get_mut(group)?.mods.get_mut(target.index),
            None => match data
                .profiles
                .get_mut(&target.profile)?
                .mods
                .get_mut(target.index)?
            {
                ModOrGroup::Individual(mc) => Some(mc),
                ModOrGroup::Group { .. } => None,
            },
        }
    }

    pub(super) fn entry_url(
        &self,
        profile: &str,
        group: Option<&str>,
        index: usize,
    ) -> Option<String> {
        self.entry(profile, group, index)
            .map(|mc| mc.spec.url.clone())
    }

    /// Follows the selected entry through edits. Closes when it was removed or the profile changed.
    pub(super) fn resolve_details(&mut self) -> Option<DetailsTarget> {
        let mut selected = self.details.clone()?;
        let target = &selected.target;
        let matches = |mc: &ModConfig| mc.spec.url == selected.url;
        let current = self
            .entry(&target.profile, target.group.as_deref(), target.index)
            .is_some_and(matches);
        if !current {
            let found = match &target.group {
                Some(group) => self
                    .state
                    .mod_data
                    .groups
                    .get(group)
                    .and_then(|g| g.mods.iter().position(matches)),
                None => self
                    .state
                    .mod_data
                    .profiles
                    .get(&target.profile)
                    .and_then(|p| {
                        p.mods
                            .iter()
                            .position(|e| matches!(e, ModOrGroup::Individual(mc) if matches(mc)))
                    }),
            };
            match found {
                Some(index) => selected.target.index = index,
                None => {
                    self.details = None;
                    return None;
                }
            }
        }
        let open_in_profile = selected.target.profile == self.state.mod_data.active_profile
            || self.open_profiles.contains(&selected.target.profile);
        if !open_in_profile {
            self.details = None;
            return None;
        }
        self.details = Some(selected.clone());
        Some(selected)
    }

    /// Escape clears the selection unless something else is using it: a focused control (egui
    /// removes focus on Escape before this runs, so the previous frame's focus is checked), an
    /// open dropdown, a confirmation or other modal, or a drag (egui consumes Escape for drags).
    fn escape_closes_details(&self, ctx: &egui::Context) -> bool {
        self.delete_confirmation.is_none()
            && focus_last_frame(ctx).is_none()
            && !ctx.memory(|memory| memory.any_popup_open() || memory.top_modal_layer().is_some())
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    }

    pub(super) fn show_details_panel(&mut self, ctx: &egui::Context) {
        if self.details.is_some() && self.escape_closes_details(ctx) {
            self.details = None;
        }
        let Some(selected) = self.resolve_details() else {
            return;
        };
        let Some(mc) = self
            .entry(
                &selected.target.profile,
                selected.target.group.as_deref(),
                selected.target.index,
            )
            .cloned()
        else {
            return;
        };
        let store = self.state.store.clone();
        let info = store.get_mod_info(&mc.spec);
        let details = store.get_mod_details(&mc.spec);
        let versions = store.get_mod_versions(&mc.spec);
        let download = self.state.config.mod_list.download_thumbnails;
        let texture = details
            .as_ref()
            .and_then(|d| d.thumbnail_url.as_deref())
            .filter(|_| download)
            .and_then(|url| {
                self.thumbnails
                    .get(ctx, url, thumbnails::ThumbnailSize::Large)
            });
        let enabled = !self.editing_busy() && self.delete_confirmation.is_none();
        let group_off = selected.target.group.as_ref().is_some_and(|group| {
            self.state.mod_data.profiles[&selected.target.profile]
                .mods
                .iter()
                .any(|entry| {
                    matches!(entry, ModOrGroup::Group { group_name, enabled }
                        if group_name == group && !enabled)
                })
        });
        let (status, status_color) = match (mc.enabled, group_off) {
            (true, false) => ("Enabled", STATUS_ENABLED),
            (true, true) => ("Disabled by its group", STATUS_GROUP_OFF),
            (false, _) => ("Disabled", ctx.style().visuals.weak_text_color()),
        };
        let source = info.as_ref().map(|i| match i.provider {
            "modio" => "mod.io",
            "http" => "Web download",
            "file" => "Local file",
            other => other,
        });
        let version = info
            .as_ref()
            .map(|_| store.get_version_name(&mc.spec).unwrap_or_default());
        let pinned = info.is_some() && store.is_pinned(&mc.spec);
        let selected_version = info.as_ref().map(|i| i.resolution.url.0.clone());
        let updated = details
            .as_ref()
            .and_then(|d| d.updated)
            .and_then(format_date);
        let author = details.as_ref().and_then(|d| d.author.clone());
        let summary = details.as_ref().and_then(|d| d.summary.clone());
        let modio = info.as_ref().map_or_else(
            || mc.spec.url.starts_with("https://mod.io/"),
            |i| i.provider == "modio",
        );

        let width_range = panel_width_range(ctx.screen_rect().width());
        let width_id = egui::Id::new("mod-details-width");
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        let (previous, mut resizing) = ctx
            .data(|data| data.get_temp::<(f32, bool)>(width_id))
            .map_or((None, false), |(width, resizing)| (Some(width), resizing));
        // Outside a drag of its edge, the panel returns to the saved width whenever the window
        // has room again, instead of keeping the width a narrow window squeezed it to.
        if !resizing && !pointer_down {
            let preferred = self
                .state
                .config
                .mod_list
                .details_width
                .unwrap_or(DEFAULT_WIDTH)
                .clamp(*width_range.start(), *width_range.end());
            let panel_id = egui::Id::new(PANEL_ID);
            if let Some(mut state) = egui::containers::panel::PanelState::load(ctx, panel_id)
                && (state.rect.width() - preferred).abs() > 0.5
            {
                state.rect.min.x = state.rect.max.x - preferred;
                ctx.data_mut(|data| data.insert_persisted(panel_id, state));
            }
        }
        let mut close = false;
        let mut priority = mc.priority;
        let mut check_update = false;
        let mut copied = false;
        let panel = egui::SidePanel::right(PANEL_ID)
            .resizable(true)
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(12, 10)))
            .default_width(
                self.state
                    .config
                    .mod_list
                    .details_width
                    .unwrap_or(DEFAULT_WIDTH)
                    .clamp(*width_range.start(), *width_range.end()),
            )
            .width_range(width_range)
            .show(ctx, |ui| {
                let name = info
                    .as_ref()
                    .map_or(mc.spec.url.as_str(), |i| i.name.as_str());
                // The name wraps instead of being cut off, and the close button stays at the top.
                ui.horizontal_top(|ui| {
                    let width = (ui.available_width() - 28.0).max(0.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.add(
                                egui::Label::new(RichText::new(name).heading().strong()).wrap(),
                            );
                            let byline: Vec<String> = author
                                .iter()
                                .map(|author| format!("by {author}"))
                                .chain(source.map(str::to_owned))
                                .collect();
                            if !byline.is_empty() {
                                ui.label(RichText::new(byline.join("  ·  ")).weak());
                            }
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if icons::button(ui, Icon::Close, "Close details").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(8.0);
                // The content width is fixed before scrolling, with a constant gutter that the
                // floating scroll bar draws over. Whether the bar is shown never changes the
                // width, so wrapping and the image size cannot feed back into the content height
                // and make the panel flicker between two layouts.
                let width = content_width(ui);
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        if let Some(texture) = &texture {
                            let size = texture.size_vec2();
                            let scale = (width / size.x).min(1.5);
                            ui.add(
                                egui::Image::new(texture)
                                    .corner_radius(6)
                                    .fit_to_exact_size(size * scale),
                            );
                            ui.add_space(10.0);
                        }

                        ui.add_enabled_ui(enabled, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                if icons::primary_button(ui, "Open mod page", true).clicked() {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(&mc.spec.url));
                                }
                                if ui.button("Copy link").clicked() {
                                    ui.ctx().copy_text(mc.spec.url.clone());
                                    copied = true;
                                }
                                if modio
                                    && icons::button(ui, Icon::Update, "Check for updates").clicked()
                                {
                                    check_update = true;
                                }
                            });
                        });

                        if let Some(summary) = &summary {
                            ui.add_space(10.0);
                            ui.label(summary);
                        }

                        section(ui, "Overview");
                        card(ui, |ui| {
                            ui.add_enabled_ui(enabled, |ui| {
                                egui::Grid::new("mod-details-grid")
                                    .num_columns(2)
                                    .spacing([16.0, 8.0])
                                    .show(ui, |ui| {
                                        ui.style_mut().wrap_mode =
                                            Some(egui::TextWrapMode::Extend);
                                        field(ui, "Status");
                                        ui.horizontal(|ui| {
                                            let (dot, _) = ui.allocate_exact_size(
                                                egui::vec2(8.0, 8.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().circle_filled(
                                                dot.center(),
                                                4.0,
                                                status_color,
                                            );
                                            ui.label(status);
                                        });
                                        ui.end_row();
                                        if let Some(group) = &selected.target.group {
                                            field(ui, "Group");
                                            ui.label(group);
                                            ui.end_row();
                                        }
                                        if let Some(version) = &version {
                                            field(ui, "Version");
                                            ui.horizontal(|ui| {
                                                ui.label(version);
                                                if pinned {
                                                    badge(ui, "pinned", Color32::LIGHT_BLUE);
                                                }
                                            });
                                            ui.end_row();
                                        }
                                        if let Some(updated) = &updated {
                                            field(ui, "Updated");
                                            ui.label(updated);
                                            ui.end_row();
                                        }
                                        field(ui, "Priority");
                                        ui.add(mod_list::priority_value(&mut priority))
                                            .on_hover_text(mod_list::PRIORITY_HINT);
                                        ui.end_row();
                                    });
                            });
                            if info.is_none() {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(
                                        "Details are not cached yet. Check for updates or install to fetch them.",
                                    )
                                    .weak(),
                                );
                            }
                        });

                        if let Some(info) = &info {
                            if let Some(tags) = &info.modio_tags {
                                section(ui, "Tags");
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x =
                                        mod_list::TAG_GAP.max(ui.spacing().item_spacing.x);
                                    for tag in mod_list::modio_tags(tags) {
                                        let text = RichText::new(tag.label)
                                            .color(mod_list::tag_text(ui.visuals(), tag.fill));
                                        let response = mod_list::tag_chip(ui, text, tag.fill);
                                        if let Some(hint) = tag.hint {
                                            response.on_disabled_hover_text(hint);
                                        }
                                    }
                                });
                                if !tags.versions.is_empty() {
                                    ui.add_space(4.0);
                                    let game_versions: Vec<_> =
                                        tags.versions.iter().map(String::as_str).collect();
                                    ui.label(
                                        RichText::new(format!(
                                            "Game versions: {}",
                                            game_versions.join(", ")
                                        ))
                                        .weak(),
                                    );
                                }
                            }
                            if !info.suggested_dependencies.is_empty() {
                                section(ui, "Dependencies");
                                for dependency in &info.suggested_dependencies {
                                    let name = store
                                        .get_mod_info(dependency)
                                        .map_or_else(|| dependency.url.clone(), |i| i.name);
                                    ui.hyperlink_to(name, &dependency.url);
                                }
                            }
                        }

                        if !versions.is_empty() {
                            ui.add_space(8.0);
                            icons::collapsing_header(
                                RichText::new(format!("Versions ({})", versions.len())).strong(),
                            )
                            .id_salt("mod-details-versions")
                            .show(ui, |ui| {
                                for version in &versions {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.strong(&version.name);
                                        if let Some(date) = version.date_added.and_then(format_date)
                                        {
                                            ui.weak(date);
                                        }
                                        if version.latest {
                                            badge(ui, "latest", Color32::LIGHT_GREEN);
                                        }
                                        if selected_version.as_ref() == Some(&version.spec.url) {
                                            badge(ui, "selected", Color32::LIGHT_BLUE);
                                        }
                                    });
                                    if let Some(changelog) = &version.changelog {
                                        ui.label(RichText::new(changelog).weak());
                                    }
                                    ui.add_space(8.0);
                                }
                            });
                        }
                    });
            });

        let width = panel.response.rect.width();
        let saved = self.state.config.mod_list.details_width;
        // Only a width set by dragging the panel's edge is saved: one that changes while the
        // pointer is held, written once after release. A narrow window that squeezes the panel
        // keeps the saved width, and a changing width is never written frame after frame.
        if pointer_down && previous.is_some_and(|previous| (previous - width).abs() >= 0.5) {
            resizing = true;
        }
        if resizing && !pointer_down {
            resizing = false;
            if saved.is_none_or(|saved| (saved - width).abs() > 1.0) {
                self.state.config.mod_list.details_width = Some(width);
                self.report_save(self.state.config.save());
            }
        }
        ctx.data_mut(|data| data.insert_temp(width_id, (width, resizing)));
        if close {
            self.details = None;
        }
        if priority != mc.priority
            && let Some(entry) = self.entry_mut(&selected.target)
        {
            entry.priority = priority;
            self.save_mod_data("Edit mod settings");
        }
        if copied {
            self.notifications.success("Copied mod link.");
        }
        if check_update {
            message::UpdateCache::send_specs(self, ctx, vec![mc.spec.clone()]);
        }
    }
}
