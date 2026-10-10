use super::*;
use crate::state::edits::{Edit, ListTarget};

fn fixture() -> TestApp {
    let mut test = TestApp::new();
    let a = test.local_mod("A.pak");
    let b = test.local_mod("B.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Individual(a),
        ModOrGroup::Individual(b),
        ModOrGroup::Group {
            group_name: "Shared".into(),
            enabled: true,
        },
    ];
    test.app
        .state
        .mod_data
        .groups
        .insert("Shared".into(), ModGroup::default());
    let profile = test.app.state.mod_data.get_active_profile().clone();
    test.app
        .state
        .mod_data
        .profiles
        .insert("Other".into(), profile);
    test.app.state.mod_data.save().unwrap();
    test.app.history = super::super::history::History::new(&test.app.state.mod_data);
    test
}

fn remove_first(test: &mut TestApp) {
    test.app.request_edit(
        Edit::DeleteEntry(ListTarget {
            profile: "default".into(),
            group: None,
            index: 0,
        }),
        true,
    );
}

#[test]
fn history_undo_redo_stay_silent_and_dismiss_the_original_edit_notification() {
    let mut test = fixture();
    remove_first(&mut test);
    for _ in 0..25 {
        test.full_frame(vec![]);
    }
    assert_eq!(
        button_rects(&test.full_frame(vec![]), "Dismiss notification").len(),
        1
    );
    test.full_button("Undo", false);
    assert!(button_rects(&test.full_frame(vec![]), "Dismiss notification").is_empty());
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 3);
    test.full_button("Redo", false);
    assert!(button_rects(&test.full_frame(vec![]), "Dismiss notification").is_empty());
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 2);
}

#[test]
fn history_restores_shared_groups_references_and_persisted_profiles_together() {
    let mut test = fixture();
    let before = (**test.app.state.mod_data).clone();
    test.app
        .request_edit(Edit::DeleteGroup("Shared".into()), true);
    assert!(!test.app.state.mod_data.groups.contains_key("Shared"));
    let after = (**test.app.state.mod_data).clone();
    test.app.restore_history(false, None);
    assert_eq!(**test.app.state.mod_data, before);
    let reloaded = State::init(test.app.state.dirs.clone()).unwrap();
    assert_eq!(**reloaded.mod_data, before);
    drop(reloaded);
    test.app.restore_history(true, None);
    assert_eq!(**test.app.state.mod_data, after);
    test.app.restore_history(false, None);
    test.app
        .request_edit(Edit::DeleteProfile("Other".into()), true);
    test.app.restore_history(false, None);
    assert_eq!(**test.app.state.mod_data, before);
}

#[test]
fn history_new_edit_clears_redo_and_stale_snackbar_cannot_undo_another_edit() {
    let mut test = fixture();
    remove_first(&mut test);
    let old_action = test.app.history.undo_id();
    test.app.request_edit(
        Edit::CreateGroup {
            profile: "default".into(),
            name: "New".into(),
        },
        false,
    );
    let changed = (**test.app.state.mod_data).clone();
    test.app.restore_history(false, old_action);
    assert_eq!(**test.app.state.mod_data, changed);
    test.app.restore_history(false, None);
    test.app.state.mod_data.active_profile = "Other".into();
    test.app.save_mod_data("Select profile");
    test.app.restore_history(true, None);
    assert!(test.app.state.mod_data.groups.contains_key("New"));
    assert_eq!(test.app.state.mod_data.active_profile, "Other");
    test.app.restore_history(false, None);
    test.app.request_edit(
        Edit::RenameGroup {
            name: "Shared".into(),
            replacement: "Renamed".into(),
        },
        false,
    );
    let new_branch = (**test.app.state.mod_data).clone();
    test.app.restore_history(true, None);
    assert_eq!(**test.app.state.mod_data, new_branch);
}

#[test]
fn history_failed_undo_keeps_memory_disk_and_cursor_and_can_retry() {
    let mut test = fixture();
    remove_first(&mut test);
    let changed = (**test.app.state.mod_data).clone();
    let action = test.app.history.undo_id();
    let path = test.app.state.dirs.config_dir.join("mod_data.json");
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    test.app.restore_history(false, None);
    assert_eq!(**test.app.state.mod_data, changed);
    assert_eq!(test.app.history.undo_id(), action);
    assert!(matches!(
        test.app.last_action.as_ref().unwrap().status,
        LastActionStatus::Failure(_)
    ));
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, bytes).unwrap();
    test.app.restore_history(false, None);
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 3);
}

#[test]
fn history_tracks_mod_settings_and_toolbar_undo_redo_without_changing_files() {
    let mut test = fixture();
    let before = (**test.app.state.mod_data).clone();
    if let ModOrGroup::Individual(config) =
        &mut test.app.state.mod_data.get_active_profile_mut().mods[0]
    {
        config.enabled = false;
        config.priority = 42;
        config.spec.url = "https://example.invalid/pinned-version".into();
    }
    test.app.save_mod_data("Edit mod settings");
    let after = (**test.app.state.mod_data).clone();
    test.full_button("Undo", false);
    assert_eq!(**test.app.state.mod_data, before);
    test.full_button("Redo", false);
    assert_eq!(**test.app.state.mod_data, after);
    test.app.restore_history(false, None);
    remove_first(&mut test);
    for _ in 0..25 {
        test.full_frame(vec![]);
    }
    let frame = test.full_frame(vec![]);
    let undo_buttons = button_rects(&frame, "Undo");
    let snackbar_button = undo_buttons
        .iter()
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap();
    test.full_click(snackbar_button.center(), false);
    assert_eq!(**test.app.state.mod_data, before);
    assert_eq!(
        std::fs::read(test.directory.path().join("A.pak")).unwrap(),
        b"UI test fixture"
    );
}

#[test]
fn history_keyboard_shortcuts_leave_text_editing_and_confirmation_alone() {
    let mut test = fixture();
    remove_first(&mut test);
    test.full_frame(vec![]);
    test.context
        .memory_mut(|m| m.request_focus(egui::Id::new("focused-button")));
    let key = |key, shift| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            ctrl: true,
            command: true,
            shift,
            ..Default::default()
        },
    };
    test.full_frame(vec![key(egui::Key::Z, false)]);
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 3);
    test.full_frame(vec![key(egui::Key::Z, true)]);
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 2);
    test.full_button("Undo", false);
    test.full_frame(vec![key(egui::Key::Y, false)]);
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 2);
    test.app
        .request_edit(Edit::DeleteProfile("Other".into()), false);
    test.full_frame(vec![key(egui::Key::Z, false)]);
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 2);
    test.full_button("Cancel", false);
    let frame = test.full_frame(vec![]);
    let edit = frame
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, n)| n.role() == egui::accesskit::Role::TextInput)
        .unwrap()
        .1
        .bounds()
        .unwrap();
    test.full_click(
        egui::pos2(edit.x0 as f32 + 10.0, edit.y0 as f32 + 5.0),
        false,
    );
    test.full_frame(vec![egui::Event::Text("typing".into())]);
    test.full_frame(vec![key(egui::Key::Z, false)]);
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 2);
}
