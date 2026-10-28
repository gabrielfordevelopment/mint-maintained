use super::*;
use crate::state::edits::{Edit, ListTarget};

fn fixture() -> TestApp {
    let mut test = TestApp::new();
    let a = test.local_mod("A.pak");
    let b = test.local_mod("B.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Group {
            group_name: "Shared".into(),
            enabled: true,
        },
        ModOrGroup::Individual(a),
        ModOrGroup::Individual(b),
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
            index: 1,
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
        &mut test.app.state.mod_data.get_active_profile_mut().mods[1]
    {
        config.enabled = false;
        config.priority = 42;
        config.spec.url = "https://example.invalid/pinned-version".into();
    }
    test.app.save_mod_data("Edit mod settings");
    let after = (**test.app.state.mod_data).clone();
    assert_ne!(after, before);
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

#[test]
fn dragging_a_priority_saves_once_and_undoes_in_one_step() {
    let mut test = TestApp::new();
    let alpha = test.local_mod("alpha.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![ModOrGroup::Individual(alpha)];
    test.app.save_mod_data("Add mods");
    let base = test.app.history.undo_id();
    let output = test.frame(vec![]);
    let spin = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.role() == egui::accesskit::Role::SpinButton)
        .and_then(|(_, node)| node.bounds())
        .expect("priority editor is shown");
    let start = egui::pos2(
        ((spin.x0 + spin.x1) / 2.0) as f32,
        ((spin.y0 + spin.y1) / 2.0) as f32,
    );
    let saved_priority = |test: &TestApp| {
        let state = State::init(Dirs::from_path(test.directory.path()).unwrap()).unwrap();
        match &state.mod_data.profiles["default"].mods[0] {
            ModOrGroup::Individual(mc) => mc.priority,
            ModOrGroup::Group { .. } => unreachable!(),
        }
    };
    let priority = |test: &TestApp| match &test.app.state.mod_data.profiles["default"].mods[0] {
        ModOrGroup::Individual(mc) => mc.priority,
        ModOrGroup::Group { .. } => unreachable!(),
    };
    test.frame(vec![
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    let mut seen = Vec::new();
    for step in 1..=8 {
        test.frame(vec![egui::Event::PointerMoved(
            start + egui::vec2(30.0 * step as f32, 0.0),
        )]);
        seen.push(priority(&test));
    }
    seen.dedup();
    assert!(seen.len() > 2, "the drag passes several values: {seen:?}");
    assert_eq!(
        test.app.history.undo_id(),
        base,
        "nothing is recorded mid-drag"
    );
    assert_eq!(saved_priority(&test), 0, "nothing is saved mid-drag");

    let end = start + egui::vec2(240.0, 0.0);
    test.frame(vec![egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    test.frame(vec![]);
    let dragged = priority(&test);
    assert!(dragged > 0);
    assert_eq!(saved_priority(&test), dragged);
    assert_ne!(test.app.history.undo_id(), base);

    test.app.restore_history(false, None);
    assert_eq!(priority(&test), 0);
    assert_eq!(
        test.app.history.undo_id(),
        base,
        "the drag was one undo step"
    );
    assert_eq!(saved_priority(&test), 0);
}

#[test]
fn undo_during_a_priority_drag_saves_the_change_first() {
    let mut test = TestApp::new();
    let alpha = test.local_mod("alpha.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![ModOrGroup::Individual(alpha)];
    test.app.save_mod_data("Add mods");
    let base = test.app.history.undo_id();
    test.frame(vec![]);
    if let ModOrGroup::Individual(mc) =
        &mut test.app.state.mod_data.get_active_profile_mut().mods[0]
    {
        mc.priority = 7;
    }
    test.app.priority_changed(egui::Id::new("priority"), true);
    test.app.restore_history(false, None);
    assert!(
        test.app.last_action.is_none(),
        "undo must not report an outside change"
    );
    match &test.app.state.mod_data.profiles["default"].mods[0] {
        ModOrGroup::Individual(mc) => assert_eq!(mc.priority, 0),
        ModOrGroup::Group { .. } => unreachable!(),
    }
    assert!(test.app.priority_edit.is_none());
    assert_eq!(test.app.history.undo_id(), base);
}
