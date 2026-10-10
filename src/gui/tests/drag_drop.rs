use super::*;

impl TestApp {
    fn drag_row(&mut self, source: egui::Pos2, destination: egui::Pos2, release: bool) {
        self.frame(vec![
            egui::Event::PointerMoved(source),
            egui::Event::PointerButton {
                pos: source,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        for fraction in [0.2, 0.5, 1.0] {
            self.frame(vec![egui::Event::PointerMoved(
                source.lerp(destination, fraction),
            )]);
        }
        self.frame(vec![]);
        if release {
            self.frame(vec![egui::Event::PointerButton {
                pos: destination,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        self.frame(vec![]);
    }
}

#[test]
fn dragging_moves_mods_into_and_out_of_groups_without_deleting_files() {
    let mut test = TestApp::new();
    let config = test.local_mod("dragged.pak");
    test.app
        .state
        .mod_data
        .groups
        .insert("Destination".into(), ModGroup::default());
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Individual(config.clone()),
        ModOrGroup::Group {
            group_name: "Destination".into(),
            enabled: true,
        },
    ];
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    let handle = button_rects(&frame, &format!("Drag {}", config.spec.url))[0].center();
    let target = text_rect(&frame, "Destination").center();
    test.drag_row(handle, target, true);
    assert_eq!(
        test.app.state.mod_data.groups["Destination"]
            .mods
            .as_slice(),
        std::slice::from_ref(&config)
    );
    assert_eq!(test.app.state.mod_data.get_active_profile().mods.len(), 1);
    // A successful drop leaves the destination open.
    let frame = test.frame(vec![]);
    let handle = button_rects(&frame, &format!("Drag {}", config.spec.url))[0].center();
    test.drag_row(handle, egui::pos2(45.0, 230.0), true);
    assert!(
        test.app.state.mod_data.groups["Destination"]
            .mods
            .is_empty()
    );
    assert!(
        matches!(&test.app.state.mod_data.get_active_profile().mods[1], ModOrGroup::Individual(mc) if mc == &config)
    );
    assert_eq!(std::fs::read(&config.spec.url).unwrap(), b"UI test fixture");
    let loaded = State::init(test.app.state.dirs.clone()).unwrap();
    assert_eq!(**loaded.mod_data, **test.app.state.mod_data);
}

#[test]
fn hover_expands_temporarily_and_successful_drop_keeps_the_group_open() {
    let mut test = TestApp::new();
    let dragged = test.local_mod("dragged.pak");
    let member = test.local_mod("member.pak");
    test.app.state.mod_data.groups.insert(
        "Hover group".into(),
        ModGroup {
            mods: vec![member.clone()],
            ..Default::default()
        },
    );
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Individual(dragged.clone()),
        ModOrGroup::Group {
            group_name: "Hover group".into(),
            enabled: true,
        },
    ];
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    let source = button_rects(&frame, &format!("Drag {}", dragged.spec.url))[0].center();
    let target = text_rect(&frame, "Hover group").center();
    test.drag_row(source, target, false);
    assert!(!test.app.group_views.get("default", "Hover group").open);
    for _ in 0..45 {
        test.frame(vec![]);
    }
    assert!(test.app.group_views.get("default", "Hover group").open);
    let frame = test.frame(vec![]);
    let inside = button_rects(&frame, &format!("Drag {}", member.spec.url))[0].center();
    test.frame(vec![egui::Event::PointerMoved(inside)]);
    assert!(test.app.group_views.get("default", "Hover group").open);
    test.frame(vec![egui::Event::PointerMoved(egui::pos2(-20.0, -20.0))]);
    assert!(!test.app.group_views.get("default", "Hover group").open);
    test.frame(vec![egui::Event::PointerMoved(target)]);
    for _ in 0..45 {
        test.frame(vec![]);
    }
    test.frame(vec![egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    test.frame(vec![]);
    assert!(test.app.group_views.get("default", "Hover group").open);
    assert_eq!(
        test.app.state.mod_data.groups["Hover group"].mods,
        [member, dragged]
    );
}

#[test]
fn new_groups_open_and_cancelled_hover_preserves_previously_open_groups() {
    let mut test = TestApp::new();
    test.app.request_edit(
        crate::state::edits::Edit::CreateGroup {
            profile: "default".into(),
            name: "New".into(),
        },
        false,
    );
    assert!(test.app.group_views.get("default", "New").open);
    let state = test.app.group_views.get("default", "New");
    state.hover(true, 1.0);
    state.hover(true, 2.0);
    state.hover(false, 3.0);
    assert!(state.open);
    state.toggle();
    state.hover(true, 4.0);
    state.hover(true, 4.6);
    assert!(state.open);
    state.hover(false, 4.7);
    assert!(!state.open);
}

#[test]
fn dragging_between_groups_and_reordering_members_and_groups_preserves_contents() {
    let mut test = TestApp::new();
    let first = test.local_mod("first.pak");
    let second = test.local_mod("second.pak");
    for (name, config) in [("A", first.clone()), ("B", second.clone())] {
        test.app.state.mod_data.groups.insert(
            name.into(),
            ModGroup {
                mods: vec![config],
                ..Default::default()
            },
        );
    }
    test.app.state.mod_data.get_active_profile_mut().mods = ["A", "B"]
        .into_iter()
        .map(|name| ModOrGroup::Group {
            group_name: name.into(),
            enabled: true,
        })
        .collect();
    test.open_group("A");
    test.open_group("B");
    let frame = test.frame(vec![]);
    test.drag_row(
        button_rects(&frame, &format!("Drag {}", first.spec.url))[0].center(),
        text_rect(&frame, "B").center(),
        true,
    );
    assert!(test.app.state.mod_data.groups["A"].mods.is_empty());
    assert_eq!(
        test.app.state.mod_data.groups["B"].mods,
        [second.clone(), first.clone()]
    );
    let frame = test.frame(vec![]);
    let target = button_rects(&frame, &format!("Drag {}", first.spec.url))[0];
    test.drag_row(
        button_rects(&frame, &format!("Drag {}", second.spec.url))[0].center(),
        target.center_bottom() - egui::vec2(0.0, 1.0),
        true,
    );
    assert_eq!(test.app.state.mod_data.groups["B"].mods, [first, second]);
    let frame = test.frame(vec![]);
    let target = button_rects(&frame, "Drag A")[0];
    test.drag_row(
        button_rects(&frame, "Drag B")[0].center(),
        target.center_top() + egui::vec2(0.0, 1.0),
        true,
    );
    assert!(
        matches!(&test.app.state.mod_data.get_active_profile().mods[0], ModOrGroup::Group { group_name, .. } if group_name == "B")
    );
    test.app.state.mod_data.validate().unwrap();
}

#[test]
fn outside_drops_and_sorted_views_do_not_mutate_manual_order() {
    let mut test = TestApp::new();
    let config = test.local_mod("stay.pak");
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .push(ModOrGroup::Individual(config.clone()));
    let before = (**test.app.state.mod_data).clone();
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    let source = button_rects(&frame, &format!("Drag {}", config.spec.url))[0].center();
    test.drag_row(source, egui::pos2(-20.0, -20.0), true);
    assert_eq!(**test.app.state.mod_data, before);
    test.app.state.config.sorting_config = Some(sorting(SortBy::Name, false));
    let frame = test.frame(vec![]);
    let source = button_rects(&frame, &format!("Drag {}", config.spec.url))[0].center();
    test.drag_row(source, egui::pos2(40.0, 200.0), true);
    assert_eq!(**test.app.state.mod_data, before);
}

#[test]
fn reorder_keeps_switch_ids_with_mods_and_escape_cancels_drag() {
    let mut test = TestApp::new();
    let a = test.local_mod("a.pak");
    let mut b = test.local_mod("b.pak");
    b.enabled = false;
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Individual(a.clone()),
        ModOrGroup::Individual(b.clone()),
    ];
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    let switches = |output: &egui::FullOutput| -> Vec<_> {
        let mut switches: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::CheckBox)
            .map(|(id, node)| (node.bounds().unwrap().y0, *id))
            .collect();
        switches.sort_by(|a, b| a.0.total_cmp(&b.0));
        switches.into_iter().map(|(_, id)| id).collect()
    };
    let ids = switches(&frame);
    assert_eq!(ids.len(), 2);
    let handle = button_rects(&frame, &format!("Drag {}", a.spec.url))[0].center();
    test.drag_row(handle, egui::pos2(40.0, 150.0), true);
    let frame = test.frame(vec![]);
    assert_eq!(
        test.app.state.mod_data.get_active_profile().mods,
        [ModOrGroup::Individual(b), ModOrGroup::Individual(a.clone())]
    );
    assert_eq!(switches(&frame), [ids[1], ids[0]]);
    let before = (**test.app.state.mod_data).clone();
    let handle = button_rects(&frame, &format!("Drag {}", a.spec.url))[0].center();
    test.drag_row(handle, egui::pos2(40.0, 10.0), false);
    test.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    test.frame(vec![egui::Event::PointerButton {
        pos: egui::pos2(40.0, 10.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert_eq!(**test.app.state.mod_data, before);
}
