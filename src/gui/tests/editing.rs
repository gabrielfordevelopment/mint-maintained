use super::*;

impl TestApp {
    fn full_frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.app.has_run_init = true;
        self.app.state.config.drg_pak_path = None;
        let modifiers = events
            .iter()
            .find_map(|event| match event {
                egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        self.full_frame_with_modifiers(events, modifiers)
    }

    fn full_frame_with_modifiers(
        &mut self,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        self.context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(&mut self.app, ctx, &mut eframe::Frame::_new_kittest()),
        )
    }

    fn full_click(&mut self, pos: egui::Pos2, shift: bool) {
        for pressed in [true, false] {
            self.full_frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers {
                        shift,
                        ..Default::default()
                    },
                },
            ]);
        }
    }

    fn full_button(&mut self, label: &str, shift: bool) {
        self.full_frame(vec![]);
        let frame = self.full_frame(vec![]);
        self.full_click(button_rects(&frame, label)[0].center(), shift);
    }
}

#[test]
fn group_creation_and_context_menu_move_are_wired_to_saved_profile_data() {
    let mut test = TestApp::new();
    let config = test.local_mod("move me.pak");
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .push(ModOrGroup::Individual(config.clone()));
    test.full_button("Groups", false);
    test.full_frame(vec![]);
    let frame = test.full_frame(vec![]);
    test.full_click(text_rect(&frame, "New group name").center(), false);
    test.full_frame(vec![egui::Event::Text("Shared edits".into())]);
    test.full_button("Create group", false);
    assert!(test.app.state.mod_data.groups.contains_key("Shared edits"));
    assert_eq!(
        test.app.state.mod_data.group_users("Shared edits"),
        ["default"]
    );
    test.full_button("Close window", false);
    let frame = test.full_frame(vec![]);
    let pos = text_rect(&frame, "move me.pak").center();
    for pressed in [true, false] {
        test.full_frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    test.full_button("Move to group…", false);
    let frame = test.full_frame(vec![]);
    let destination = button_rects(&frame, "Shared edits")
        .into_iter()
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap();
    test.full_click(destination.center(), false);
    assert_eq!(
        test.app.state.mod_data.groups["Shared edits"].mods,
        [config]
    );
    assert_eq!(test.app.state.mod_data.get_active_profile().mods.len(), 1);
    let loaded = State::init(test.app.state.dirs.clone()).unwrap();
    assert_eq!(**loaded.mod_data, **test.app.state.mod_data);
}

#[test]
fn releasing_shift_after_the_click_in_the_same_frame_still_skips_confirmation() {
    let mut test = TestApp::new();
    let config = test.local_mod("quick shift click.pak");
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .push(ModOrGroup::Individual(config));
    test.full_frame(vec![]);
    let frame = test.full_frame(vec![]);
    let pos = button_rects(&frame, "Delete mod")[0].center();
    for pressed in [true, false] {
        test.full_frame_with_modifiers(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::SHIFT,
                },
            ],
            if pressed {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            },
        );
    }
    assert!(test.app.delete_confirmation.is_none());
    assert!(test.app.state.mod_data.get_active_profile().mods.is_empty());
}

#[test]
fn deletion_confirmation_cancel_escape_enter_and_shift_preserve_files_in_both_themes() {
    for theme in [egui::ThemePreference::Light, egui::ThemePreference::Dark] {
        let mut test = TestApp::new();
        test.context.set_theme(theme);
        let config = test.local_mod("Unicode İ fixture.pak");
        let path = config.spec.url.clone();
        test.app
            .state
            .mod_data
            .get_active_profile_mut()
            .mods
            .push(ModOrGroup::Individual(config));
        for cancel in ["Cancel", "Escape", "Enter"] {
            test.full_button("Delete mod", false);
            assert!(test.app.delete_confirmation.is_some());
            assert_eq!(test.app.state.mod_data.get_active_profile().mods.len(), 1);
            test.full_frame(vec![]);
            let frame = test.full_frame(vec![]);
            assert!(
                text_rect(
                    &frame,
                    "Hold Shift while clicking Delete to skip confirmation."
                )
                .is_positive()
            );
            if cancel == "Cancel" {
                test.full_button("Cancel", false);
            } else {
                test.full_frame(vec![egui::Event::Key {
                    key: if cancel == "Escape" {
                        egui::Key::Escape
                    } else {
                        egui::Key::Enter
                    },
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            assert!(test.app.delete_confirmation.is_none(), "{cancel}");
            assert_eq!(test.app.state.mod_data.get_active_profile().mods.len(), 1);
        }
        test.full_button("Delete mod", true);
        assert!(test.app.delete_confirmation.is_none());
        assert!(test.app.state.mod_data.get_active_profile().mods.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"UI test fixture");
        let loaded = State::init(Dirs::from_path(test.directory.path()).unwrap()).unwrap();
        assert!(loaded.mod_data.get_active_profile().mods.is_empty());
    }
}

#[test]
fn confirmation_refuses_stale_targets_and_profile_switches() {
    let mut test = TestApp::new();
    for name in ["a.pak", "b.pak"] {
        let config = test.local_mod(name);
        test.app
            .state
            .mod_data
            .get_active_profile_mut()
            .mods
            .push(ModOrGroup::Individual(config));
    }
    test.full_button("Delete mod", false);
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .swap(0, 1);
    test.full_button("Delete", false);
    assert_eq!(test.app.state.mod_data.get_active_profile().mods.len(), 2);
    assert!(matches!(
        test.app.last_action.as_ref().unwrap().status,
        LastActionStatus::Failure(_)
    ));
    test.app
        .state
        .mod_data
        .profiles
        .insert("other".into(), Default::default());
    test.full_button("Delete mod", false);
    test.app.state.mod_data.active_profile = "other".into();
    test.full_button("Delete", false);
    assert_eq!(test.app.state.mod_data.profiles["default"].mods.len(), 2);
}

#[test]
fn group_entry_and_profile_deletion_share_confirmation_and_shift_behavior() {
    let mut test = TestApp::new();
    test.app
        .state
        .mod_data
        .profiles
        .insert("other".into(), Default::default());
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .push(ModOrGroup::Group {
            group_name: "default".into(),
            enabled: true,
        });
    test.full_button("Remove group from profile", false);
    assert!(test.app.delete_confirmation.is_some());
    test.full_button("Delete", false);
    assert!(test.app.state.mod_data.get_active_profile().mods.is_empty());
    assert!(test.app.state.mod_data.groups.contains_key("default"));
    test.full_button("Delete profile", false);
    assert!(test.app.delete_confirmation.is_some());
    test.full_button("Cancel", false);
    assert_eq!(test.app.state.mod_data.profiles.len(), 2);
    test.full_button("Delete profile", true);
    assert_eq!(test.app.state.mod_data.profiles.len(), 1);
    assert!(test.app.delete_confirmation.is_none());
    test.full_button("Delete profile", true);
    assert_eq!(test.app.state.mod_data.profiles.len(), 1);
}

#[test]
fn groups_window_is_reachable_and_shared_group_delete_lists_affected_profiles() {
    let mut test = TestApp::new();
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .push(ModOrGroup::Group {
            group_name: "default".into(),
            enabled: true,
        });
    let shared_profile = test.app.state.mod_data.get_active_profile().clone();
    test.app
        .state
        .mod_data
        .profiles
        .insert("second profile".into(), shared_profile);
    test.full_button("Groups", false);
    assert!(test.app.groups_window.is_some());
    test.full_button("Delete group", false);
    let frame = test.full_frame(vec![]);
    assert!(frame.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("second profile"))));
    test.full_button("Cancel", false);
    assert!(test.app.state.mod_data.groups.contains_key("default"));
    test.full_button("Delete group", true);
    assert!(!test.app.state.mod_data.groups.contains_key("default"));
    test.app.state.mod_data.validate().unwrap();
}
