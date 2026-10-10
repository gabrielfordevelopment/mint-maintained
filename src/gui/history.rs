use std::{collections::VecDeque, sync::Arc};

use super::{App, Icon, LastAction, ModData, icons};

const LIMIT: usize = 50;

struct Entry {
    id: u64,
    label: String,
    before: Arc<ModData>,
    after: Arc<ModData>,
}

pub(super) struct History {
    current: Arc<ModData>,
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    next_id: u64,
}

fn same_content(a: &ModData, b: &ModData) -> bool {
    a.profiles == b.profiles && a.groups == b.groups
}

impl History {
    pub(super) fn new(data: &ModData) -> Self {
        Self {
            current: Arc::new(data.clone()),
            undo: VecDeque::new(),
            redo: Vec::new(),
            next_id: 0,
        }
    }

    fn record(&mut self, data: &ModData, label: &str) {
        if same_content(&self.current, data) {
            return;
        }
        self.next_id += 1;
        let after = Arc::new(data.clone());
        self.undo.push_back(Entry {
            id: self.next_id,
            label: label.into(),
            before: self.current.clone(),
            after: after.clone(),
        });
        self.current = after;
        self.redo.clear();
        if self.undo.len() > LIMIT {
            self.undo.pop_front();
        }
    }

    pub(super) fn undo_id(&self) -> Option<u64> {
        self.undo.back().map(|e| e.id)
    }
}

impl App {
    pub(super) fn save_mod_data(&mut self, label: &str) -> bool {
        self.history.record(&self.state.mod_data, label);
        self.report_save(self.state.mod_data.save())
    }

    pub(super) fn notify_edit(&mut self, text: String) {
        self.notifications
            .success_with_undo(text, self.history.undo_id());
    }

    pub(super) fn history_available(&self) -> bool {
        !self.editing_busy() && self.self_update_rid.is_none() && self.delete_confirmation.is_none()
    }

    pub(super) fn restore_history(&mut self, redo: bool, expected: Option<u64>) {
        if !self.history_available() {
            return;
        }
        let entry = if redo {
            self.history.redo.last()
        } else {
            self.history.undo.back()
        };
        let Some(entry) = entry else {
            return;
        };
        if expected.is_some_and(|id| id != entry.id) {
            return;
        }
        if !same_content(&self.state.mod_data, &self.history.current) {
            self.last_action = Some(LastAction::failure(
                "The mod list changed outside the editing history. Nothing was restored.".into(),
            ));
            return;
        }
        let mut target = if redo {
            (*entry.after).clone()
        } else {
            (*entry.before).clone()
        };
        if target
            .profiles
            .contains_key(&self.state.mod_data.active_profile)
        {
            target
                .active_profile
                .clone_from(&self.state.mod_data.active_profile);
        }
        if !target.profiles.contains_key(&target.active_profile)
            && let Some(name) = target.profiles.keys().next()
        {
            target.active_profile = name.clone();
        }
        let label = entry.label.clone();
        let action_id = entry.id;
        let previous = std::mem::replace(&mut **self.state.mod_data, target);
        if let Err(error) = self.state.mod_data.save() {
            **self.state.mod_data = previous;
            self.report_save(Err(error));
            return;
        }
        if redo {
            let entry = self.history.redo.pop().unwrap();
            self.history.current = entry.after.clone();
            self.history.undo.push_back(entry);
        } else {
            let entry = self.history.undo.pop_back().unwrap();
            self.history.current = entry.before.clone();
            self.history.redo.push(entry);
        }
        self.open_profiles
            .retain(|p| self.state.mod_data.profiles.contains_key(p));
        for (profile, data) in &self.state.mod_data.profiles {
            for item in &data.mods {
                if let super::ModOrGroup::Group { group_name, .. } = item {
                    let existed = previous.profiles.get(profile).is_some_and(|p| p.mods.iter().any(|entry|
                        matches!(entry, super::ModOrGroup::Group { group_name: name, .. } if name == group_name)));
                    if !existed
                        || previous.groups.get(group_name)
                            != self.state.mod_data.groups.get(group_name)
                    {
                        self.group_views.open(profile, group_name);
                    }
                }
            }
        }
        let message = format!("{}: {label}", if redo { "Redone" } else { "Undone" });
        tracing::info!("{message}");
        self.notifications.dismiss_edit(action_id);
    }
}

impl History {
    pub(super) fn controls(&self, ui: &mut egui::Ui, enabled: bool) -> Option<bool> {
        let mut requested = None;
        for redo in [true, false] {
            let entry = if redo {
                self.redo.last()
            } else {
                self.undo.back()
            };
            let name = if redo { "Redo" } else { "Undo" };
            let hint = entry.map_or_else(
                || {
                    format!(
                        "{name}: nothing to {name_lower}",
                        name_lower = name.to_lowercase()
                    )
                },
                |e| {
                    format!(
                        "{name}: {}\n{}",
                        e.label,
                        if redo {
                            "Ctrl+Y / Ctrl+Shift+Z"
                        } else {
                            "Ctrl+Z"
                        }
                    )
                },
            );
            let response = ui
                .add_enabled_ui(enabled && entry.is_some(), |ui| {
                    icons::button(ui, if redo { Icon::Redo } else { Icon::Undo }, name)
                })
                .inner
                .on_hover_text(hint);
            if response.clicked() {
                requested = Some(redo);
            }
        }
        requested
    }
}

impl App {
    pub(super) fn history_shortcuts(&mut self, ctx: &egui::Context) {
        let editing_text = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if !self.history_available() || editing_text || egui::DragAndDrop::has_any_payload(ctx) {
            return;
        }
        let action = ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)
            {
                Some(true)
            } else if i.consume_key(egui::Modifiers::CTRL, egui::Key::Z) {
                Some(false)
            } else {
                None
            }
        });
        if let Some(redo) = action {
            self.restore_history(redo, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_limited_to_fifty_changes_and_navigation_is_not_an_edit() {
        let mut data = ModData::default();
        data.profiles.insert("A".into(), Default::default());
        let mut history = History::new(&data);
        for i in 0..60 {
            data.profiles
                .insert(format!("Profile {i}"), Default::default());
            history.record(&data, "Create profile");
        }
        assert_eq!(history.undo.len(), 50);
        let id = history.undo_id();
        data.active_profile = "A".into();
        history.record(&data, "Select profile");
        assert_eq!(history.undo_id(), id);
        assert_eq!(history.undo.front().unwrap().id, 11);
        assert!(History::new(&data).undo.is_empty());
    }
}
