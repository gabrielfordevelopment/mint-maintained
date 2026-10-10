use super::*;
use crate::state::edits::{Edit, ListTarget};

#[derive(Default)]
pub(super) struct GroupsWindow {
    pub selected: String,
    name: String,
    new_name: String,
}

impl GroupsWindow {
    pub fn selected(name: String) -> Self {
        Self {
            selected: name.clone(),
            name,
            new_name: String::new(),
        }
    }
}

impl App {
    pub(super) fn show_groups(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.groups_window.take() else {
            return;
        };
        let names: Vec<_> = self.state.mod_data.groups.keys().cloned().collect();
        if !names.contains(&window.selected) {
            window.selected = names.first().cloned().unwrap_or_default();
            window.name = window.selected.clone();
        }
        let profile = self.state.mod_data.active_profile.clone();
        let busy = self.editing_busy() || self.delete_confirmation.is_some();
        let mut open = true;
        let mut action = None;
        egui::Window::new("Groups").open(&mut open).default_width(430.0).show(ctx, |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("Groups are shared between profiles. Changes affect every profile listed below.");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut window.new_name).hint_text("New group name").desired_width(180.0));
                    let name = window.new_name.trim();
                    if ui.add_enabled(!name.is_empty() && !names.iter().any(|n| n == name), egui::Button::new("Create group")).clicked() {
                        action = Some(Edit::CreateGroup { profile: profile.clone(), name: name.into() });
                        window.selected = name.into();
                        window.name = name.into();
                        window.new_name.clear();
                    }
                });
                ui.separator();
                let old_selection = window.selected.clone();
                egui::ComboBox::from_id_salt("shared-group").selected_text(if window.selected.is_empty() { "Select a group" } else { &window.selected }).show_ui(ui, |ui| {
                    for name in &names { ui.selectable_value(&mut window.selected, name.clone(), name); }
                });
                if old_selection != window.selected { window.name = window.selected.clone(); }
                if let Some(group) = self.state.mod_data.groups.get(&window.selected) {
                    let users = self.state.mod_data.group_users(&window.selected);
                    ui.add(egui::Label::new(format!("Used by: {}", if users.is_empty() { "No profiles".into() } else { users.join(", ") })).wrap());
                    let attached = users.contains(&profile);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!attached, egui::Button::new("Add to current profile")).clicked() {
                            action = Some(Edit::AttachGroup { profile: profile.clone(), name: window.selected.clone() });
                        }
                        if ui.button("Delete group").clicked() { action = Some(Edit::DeleteGroup(window.selected.clone())); }
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut window.name).hint_text("Group name").desired_width(180.0));
                        let name = window.name.trim();
                        if ui.add_enabled(!name.is_empty() && name != window.selected && !names.iter().any(|n| n == name), egui::Button::new("Rename group")).clicked() {
                            action = Some(Edit::RenameGroup { name: window.selected.clone(), replacement: name.into() });
                            window.selected = name.into();
                        }
                    });
                    ui.separator();
                    ui.label("Right-click a mod name in the profile to move it into a group.");
                    if !attached { ui.label("Add this group to the current profile to edit its mod list."); }
                    if group.mods.is_empty() { ui.label("This group is empty."); }
                    egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                        for (index, config) in group.mods.iter().enumerate() {
                            ui.push_id(index, |ui| {
                                ui.horizontal(|ui| {
                                    // An unused group can be attached before its entries are edited.
                                    ui.add_enabled_ui(attached, |ui| {
                                        if icons::button(ui, Icon::Delete, "Delete mod").clicked() {
                                            action = Some(Edit::DeleteEntry(ListTarget { profile: profile.clone(), group: Some(window.selected.clone()), index }));
                                        }
                                    });
                                    ui.add(egui::Label::new(self.mod_name(config)).truncate());
                                });
                            });
                        }
                    });
                }
            });
        });
        if open {
            self.groups_window = Some(window);
        }
        if let Some(action) = action {
            self.request_edit(action, editing::skip_delete_confirmation(ctx));
        }
    }
}
