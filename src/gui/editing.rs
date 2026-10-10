use super::*;
use crate::state::edits::{Edit, ListTarget, PreparedEdit};

pub(super) struct DeleteConfirmation {
    edit: PreparedEdit,
    title: &'static str,
    action: &'static str,
    details: String,
    focus_cancel: bool,
}

pub(super) fn skip_delete_confirmation(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input
            .events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                    ..
                }
                | egui::Event::Key {
                    key: egui::Key::Enter | egui::Key::Space,
                    pressed: true,
                    modifiers,
                    ..
                } => Some(modifiers.shift),
                _ => None,
            })
            .unwrap_or(input.modifiers.shift)
    })
}

impl App {
    pub(super) fn editing_busy(&self) -> bool {
        self.integrate_rid.is_some()
            || self.update_rid.is_some()
            || self.lint_rid.is_some()
            || self.resolve_mod_rid.is_some()
    }

    pub(super) fn mod_name(&self, config: &ModConfig) -> String {
        self.state
            .store
            .get_mod_info(&config.spec)
            .map(|info| info.name)
            .unwrap_or_else(|| {
                if !config.spec.url.contains("://") {
                    std::path::Path::new(&config.spec.url)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(&config.spec.url)
                        .to_owned()
                } else {
                    config.spec.url.clone()
                }
            })
    }

    pub(super) fn request_edit(&mut self, edit: Edit, skip_confirmation: bool) {
        if self.editing_busy() || self.delete_confirmation.is_some() {
            return;
        }
        let description = match &edit {
            Edit::DeleteEntry(target) => self.delete_description(target),
            Edit::DeleteProfile(name) => Some((
                "Delete profile?",
                format!(
                    "Delete profile \"{name}\" and its mod list? Shared groups and mod files will be kept."
                ),
            )),
            Edit::DeleteGroup(name) => Some((
                "Delete shared group?",
                format!(
                    "Delete shared group \"{name}\" and its contents from all profiles?\nUsed by: {}\nMod files will not be deleted.",
                    self.state.mod_data.group_users(name).join(", ")
                ),
            )),
            _ => None,
        };
        let prepared = PreparedEdit::new(&self.state.mod_data, edit);
        if !skip_confirmation && let Some((title, details)) = description {
            self.delete_confirmation = Some(DeleteConfirmation {
                edit: prepared,
                title,
                action: if title.starts_with("Remove") {
                    "Remove"
                } else {
                    "Delete"
                },
                details,
                focus_cancel: true,
            });
        } else {
            self.finish_edit(prepared);
        }
    }

    fn delete_description(&self, target: &ListTarget) -> Option<(&'static str, String)> {
        if let Some(group) = &target.group {
            let config = self
                .state
                .mod_data
                .groups
                .get(group)?
                .mods
                .get(target.index)?;
            Some((
                "Remove mod?",
                format!(
                    "Remove \"{}\" from shared group \"{group}\"?\nThis affects profiles: {}\nThe mod file will not be deleted.",
                    self.mod_name(config),
                    self.state.mod_data.group_users(group).join(", ")
                ),
            ))
        } else {
            match self
                .state
                .mod_data
                .profiles
                .get(&target.profile)?
                .mods
                .get(target.index)?
            {
                ModOrGroup::Individual(config) => Some((
                    "Remove mod?",
                    format!(
                        "Remove \"{}\" from profile \"{}\"?\nThe mod file will not be deleted.",
                        self.mod_name(config),
                        target.profile
                    ),
                )),
                ModOrGroup::Group { group_name, .. } => Some((
                    "Remove group from profile?",
                    format!(
                        "Remove group \"{group_name}\" from profile \"{}\"?\nThe shared group, its mods and other profiles will be kept.",
                        target.profile
                    ),
                )),
            }
        }
    }

    pub(super) fn finish_edit(&mut self, edit: PreparedEdit) -> bool {
        let action = edit.action().clone();
        let before = (**self.state.mod_data).clone();
        let item = match &action {
            Edit::DeleteEntry(target)
            | Edit::MoveMod { source: target, .. }
            | Edit::MoveEntry { source: target, .. } => {
                let config = if let Some(group) = &target.group {
                    before
                        .groups
                        .get(group)
                        .and_then(|group| group.mods.get(target.index))
                } else {
                    before
                        .profiles
                        .get(&target.profile)
                        .and_then(|p| p.mods.get(target.index))
                        .and_then(|entry| match entry {
                            ModOrGroup::Individual(config) => Some(config),
                            _ => None,
                        })
                };
                config
                    .map(|config| self.mod_name(config))
                    .unwrap_or_else(|| {
                        before
                            .profiles
                            .get(&target.profile)
                            .and_then(|p| p.mods.get(target.index))
                            .and_then(|entry| match entry {
                                ModOrGroup::Group { group_name, .. } => Some(group_name.clone()),
                                _ => None,
                            })
                            .unwrap_or_default()
                    })
            }
            _ => String::new(),
        };
        let description = diagnostics::redact(
            &edit_message(&action, &item, &before),
            self.state
                .config
                .provider_parameters
                .values()
                .flat_map(|parameters| parameters.values().cloned()),
        );
        match edit.apply(&mut self.state.mod_data) {
            Ok(changed) => {
                match &action {
                    Edit::CreateGroup { profile, name } | Edit::AttachGroup { profile, name } => {
                        self.group_views.open(profile, name)
                    }
                    Edit::MoveEntry {
                        destination:
                            ListTarget {
                                profile,
                                group: Some(name),
                                ..
                            },
                        ..
                    } => self.group_views.open(profile, name),
                    Edit::MoveMod {
                        source,
                        destination: Some(name),
                    } => self.group_views.open(&source.profile, name),
                    _ => {}
                }
                self.open_profiles
                    .retain(|profile| self.state.mod_data.profiles.contains_key(profile));
                if changed {
                    if self.report_save(self.state.mod_data.save()) {
                        tracing::info!("{description}");
                    } else {
                        tracing::warn!(
                            "Profile edit applied in memory but could not be saved. {description}"
                        );
                    }
                }
                changed
            }
            Err(error) => {
                let error_text = diagnostics::redact(
                    &error.to_string(),
                    self.state
                        .config
                        .provider_parameters
                        .values()
                        .flat_map(|p| p.values().cloned()),
                );
                tracing::warn!(
                    "Profile edit rejected; nothing changed. Attempted change: {description} Reason: {error_text}"
                );
                self.last_action = Some(LastAction::failure(error.to_string()));
                false
            }
        }
    }

    pub(super) fn show_delete_confirmation(&mut self, ctx: &egui::Context) {
        let busy = self.editing_busy();
        let Some(dialog) = &mut self.delete_confirmation else {
            return;
        };
        let response = egui::Modal::new(egui::Id::new("delete-confirmation")).show(ctx, |ui| {
            ui.set_width(420.0_f32.min(ui.ctx().screen_rect().width() - 48.0));
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.heading(dialog.title);
                ui.add_space(10.0);
                ui.add(egui::Label::new(&dialog.details).wrap());
            });
            ui.add_space(16.0);
            let (cancel, delete) = ui
                .horizontal(|ui| {
                    let button_size = egui::vec2(88.0, 28.0);
                    ui.add_space(
                        (ui.available_width() - 2.0 * button_size.x - ui.spacing().item_spacing.x)
                            .max(0.0)
                            / 2.0,
                    );
                    let cancel = ui.add_sized(button_size, egui::Button::new("Cancel"));
                    if dialog.focus_cancel {
                        cancel.request_focus();
                        dialog.focus_cancel = false;
                    }
                    (
                        cancel.clicked(),
                        ui.add_enabled_ui(!busy, |ui| {
                            super::icons::delete_button(ui, dialog.action, button_size)
                        })
                        .inner
                        .clicked(),
                    )
                })
                .inner;
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.small("Hold Shift while clicking the trash icon to skip confirmation.");
            });
            ui.add_space(6.0);
            (cancel, delete)
        });
        if response.inner.0 || response.should_close() {
            self.delete_confirmation = None;
        } else if response.inner.1 {
            let dialog = self.delete_confirmation.take().unwrap();
            self.finish_edit(dialog.edit);
        }
    }
}

fn edit_message(action: &Edit, item: &str, data: &crate::state::ModData_v0_1_0) -> String {
    let location = |target: &ListTarget| match &target.group {
        Some(group) => format!("shared group {group:?}"),
        None => format!("profile {:?}", target.profile),
    };
    match action {
        Edit::DeleteEntry(target) => format!(
            "Removed {item:?} from {}. Mod files were kept.",
            location(target)
        ),
        Edit::DeleteProfile(name) => {
            format!("Deleted profile {name:?}. Shared groups and mod files were kept.")
        }
        Edit::CreateGroup { profile, name } => {
            format!("Created group {name:?} in profile {profile:?}.")
        }
        Edit::AttachGroup { profile, name } => {
            format!("Added shared group {name:?} to profile {profile:?}.")
        }
        Edit::RenameGroup { name, replacement } => {
            format!("Renamed shared group {name:?} to {replacement:?}.")
        }
        Edit::DeleteGroup(name) => {
            format!("Deleted shared group {name:?} from all profiles. Mod files were kept.")
        }
        Edit::SetGroupColor { name, color } => format!(
            "Changed color of shared group {name:?} to {}.",
            super::group_colors::palette(*color, false).0
        ),
        Edit::MoveMod {
            source,
            destination,
        } => {
            let destination = ListTarget {
                profile: source.profile.clone(),
                group: destination.clone(),
                index: 0,
            };
            format!(
                "Moved {item:?} from {} to {}.",
                location(source),
                location(&destination)
            )
        }
        Edit::MoveEntry {
            source,
            destination,
        } => {
            if source.profile == destination.profile && source.group == destination.group {
                format!("Reordered {item:?} within {}.", location(source))
            } else {
                format!(
                    "Moved {item:?} from {} to {}.",
                    location(source),
                    location(destination)
                )
            }
        }
        Edit::Ungroup { profile, index } => {
            let name = data
                .profiles
                .get(profile)
                .and_then(|p| p.mods.get(*index))
                .and_then(|entry| match entry {
                    ModOrGroup::Group { group_name, .. } => Some(group_name.as_str()),
                    _ => None,
                })
                .unwrap_or("unavailable group");
            format!("Ungrouped {name:?} in profile {profile:?}. The shared group was kept.")
        }
    }
}

pub(super) fn mod_context_menu(ui: &mut Ui, groups: &[String], target: ListTarget) -> Option<Edit> {
    let mut edit = None;
    ui.label("Groups are shared across profiles.");
    ui.menu_button("Move to group…", |ui| {
        for name in groups {
            if target.group.as_ref() != Some(name) && ui.button(name).clicked() {
                edit = Some(Edit::MoveMod {
                    source: target.clone(),
                    destination: Some(name.clone()),
                });
                ui.close_menu();
            }
        }
        if groups.is_empty() {
            ui.label("Create a group with the Groups button first.");
        }
    });
    if target.group.is_some() && ui.button("Move out of group").clicked() {
        edit = Some(Edit::MoveMod {
            source: target,
            destination: None,
        });
        ui.close_menu();
    }
    edit
}
