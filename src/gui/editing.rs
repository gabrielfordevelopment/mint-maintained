use super::*;
use crate::state::edits::{Edit, ListTarget, PreparedEdit};

pub(super) struct DeleteConfirmation {
    edit: PreparedEdit,
    title: &'static str,
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
                "Delete mod?",
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
                    "Delete mod?",
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

    fn finish_edit(&mut self, edit: PreparedEdit) {
        match edit.apply(&mut self.state.mod_data) {
            Ok(()) => {
                self.open_profiles
                    .retain(|profile| self.state.mod_data.profiles.contains_key(profile));
                self.report_save(self.state.mod_data.save());
            }
            Err(error) => self.last_action = Some(LastAction::failure(error.to_string())),
        }
    }

    pub(super) fn show_delete_confirmation(&mut self, ctx: &egui::Context) {
        let busy = self.editing_busy();
        let Some(dialog) = &mut self.delete_confirmation else {
            return;
        };
        let response = egui::Modal::new(egui::Id::new("delete-confirmation")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.heading(dialog.title);
            ui.add(egui::Label::new(&dialog.details).wrap());
            let (cancel, delete) = ui
                .horizontal(|ui| {
                    let cancel = ui.button("Cancel");
                    if dialog.focus_cancel {
                        cancel.request_focus();
                        dialog.focus_cancel = false;
                    }
                    (
                        cancel.clicked(),
                        ui.add_enabled(!busy, egui::Button::new("Delete")).clicked(),
                    )
                })
                .inner;
            ui.small("Hold Shift while clicking Delete to skip confirmation.");
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
