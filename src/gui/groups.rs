use super::*;
use crate::state::edits::Edit;

#[derive(Default)]
pub(super) struct GroupsWindow {
    pub selected: String,
    name: String,
    new_name: String,
    show_management: bool,
}

impl GroupsWindow {
    pub fn selected(name: String) -> Self {
        Self {
            selected: name.clone(),
            name,
            new_name: String::new(),
            show_management: true,
        }
    }
}

impl App {
    pub(super) fn show_groups(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.groups_window.take() else {
            return;
        };
        let profile = self.state.mod_data.active_profile.clone();
        let busy = self.editing_busy() || self.delete_confirmation.is_some();
        let mut open = true;
        let mut action = None;
        egui::Window::new("Groups").open(&mut open).default_width(400.0).show(ctx, |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    let input = ui.add(inputs::bordered(egui::TextEdit::singleline(&mut window.new_name).hint_text("New group name").desired_width(220.0)));
                    let name = window.new_name.trim();
                    let valid = !name.is_empty() && !self.state.mod_data.groups.contains_key(name);
                    if ui.add_enabled(valid, egui::Button::new("Create group")).clicked() || (valid && is_committed(&input)) {
                        action = Some(Edit::CreateGroup { profile: profile.clone(), name: name.into() });
                        window.new_name.clear();
                    }
                });
                ui.add_space(6.0);
                ui.label("Drag mods onto a group in the list. Drag them back to the main list to move them out.");
                ui.weak("Groups are shared across profiles.");
                ui.add_space(6.0);
                icons::collapsing_header("Manage shared groups").open(window.show_management.then_some(true)).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                        for (name, group) in &self.state.mod_data.groups {
                            ui.push_id(name, |ui| {
                                let users = self.state.mod_data.group_users(name);
                                ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(format!("{name} ({} mods)", group.mods.len())).truncate());
                                    if users.contains(&profile) { ui.weak("In this profile"); }
                                    else if ui.button("Add to profile").clicked() { action = Some(Edit::AttachGroup { profile: profile.clone(), name: name.clone() }); }
                                    ui.menu_button("Manage", |ui| {
                                        ui.label(format!("Used by: {}", if users.is_empty() { "No profiles".into() } else { users.join(", ") }));
                                        if ui.button("Rename group").clicked() { window.selected = name.clone(); window.name = name.clone(); ui.close_menu(); }
                                        if ui.button("Delete group").clicked() { action = Some(Edit::DeleteGroup(name.clone())); ui.close_menu(); }
                                    });
                                });
                                if window.selected == *name {
                                    ui.horizontal(|ui| {
                                        ui.add(inputs::bordered(egui::TextEdit::singleline(&mut window.name).desired_width(180.0)));
                                        let replacement = window.name.trim();
                                        let valid = !replacement.is_empty() && replacement != name && !self.state.mod_data.groups.contains_key(replacement);
                                        if ui.add_enabled(valid, egui::Button::new("Save name")).clicked() {
                                            action = Some(Edit::RenameGroup { name: name.clone(), replacement: replacement.into() });
                                            window.selected.clear();
                                        }
                                        if ui.button("Cancel").clicked() { window.selected.clear(); }
                                    });
                                }
                                ui.separator();
                            });
                        }
                    });
                });
            });
        });
        window.show_management = false;
        if open {
            self.groups_window = Some(window);
        }
        if let Some(action) = action {
            self.request_edit(action, editing::skip_delete_confirmation(ctx));
        }
    }
}
