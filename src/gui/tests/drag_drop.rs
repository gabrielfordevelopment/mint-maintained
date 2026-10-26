use super::*;

impl TestApp {
    /// Runs frames until preview animations finish.
    fn settle(&mut self) -> egui::FullOutput {
        for _ in 0..20 {
            self.frame(vec![]);
        }
        self.frame(vec![])
    }

    /// Drags the row whose handle is at `source` so that its top edge lands on `destination`, a
    /// point in the list as laid out before the drag.
    fn drag_row(&mut self, source: egui::Pos2, destination: egui::Pos2, release: bool) {
        let output = self.frame(vec![]);
        let destination = aim(&output, source, destination);
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

fn drag_handles(output: &egui::FullOutput) -> Vec<egui::Rect> {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == egui::accesskit::Role::Button
                && node.label().is_some_and(|label| label.starts_with("Drag "))
        })
        .filter_map(|(_, node)| node.bounds())
        .map(|rect| {
            egui::Rect::from_min_max(
                egui::pos2(rect.x0 as f32, rect.y0 as f32),
                egui::pos2(rect.x1 as f32, rect.y1 as f32),
            )
        })
        .collect()
}

/// Pointer position, for a drag grabbed at `grab`, that puts the dragged row's top edge on
/// `target`. Once the row is lifted out, rows below it are shown one slot higher, and rows have
/// one pixel of padding above their handles.
fn aim(output: &egui::FullOutput, grab: egui::Pos2, target: egui::Pos2) -> egui::Pos2 {
    let handles = drag_handles(output);
    let Some(source) = handles.iter().find(|rect| rect.contains(grab)) else {
        return target;
    };
    let below = || handles.iter().filter(|rect| rect.top() > source.bottom());
    // The next row, skipping the members of a dragged group.
    let next = below()
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .and_then(|next| {
            if next.left() > source.left() + 1.0 {
                below()
                    .filter(|rect| rect.left() <= source.left() + 1.0)
                    .map(|rect| rect.top())
                    .reduce(f32::min)
            } else {
                Some(next.top())
            }
        });
    let mut target = target;
    if let Some(next) = next
        && target.y >= next - 1.0
    {
        target.y -= next - source.top();
    }
    target + egui::vec2(0.0, grab.y - source.top() + 1.0)
}

/// The space the drop preview opens where the dragged entry would land.
fn drop_gap(output: &egui::FullOutput) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) if rect.stroke.width == crate::gui::drag_drop::GAP_STROKE => {
            Some(rect.rect)
        }
        _ => None,
    })
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
        ModOrGroup::Group {
            group_name: "Destination".into(),
            enabled: true,
        },
        ModOrGroup::Individual(config.clone()),
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
    let member = button_rects(&frame, &format!("Drag {}", config.spec.url))[0];
    let destination = egui::pos2(
        40.0,
        button_rects(&frame, "Drag Destination")[0].top() + 1.0,
    );
    test.drag_row(member.center(), destination, false);
    let frame = test.settle();
    // The member is lifted out of the group, so the opened space starts where it was.
    let gap = drop_gap(&frame).expect("dragging shows where the mod lands");
    assert!(
        (gap.top() - member.top()).abs() < 4.0,
        "without standalone mods, the root insertion space must follow the group: {gap:?} {member:?}"
    );
    test.frame(vec![egui::Event::PointerButton {
        pos: destination,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    test.frame(vec![]);
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
        ModOrGroup::Group {
            group_name: "Hover group".into(),
            enabled: true,
        },
        ModOrGroup::Individual(dragged.clone()),
    ];
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    let source = button_rects(&frame, &format!("Drag {}", dragged.spec.url))[0].center();
    let target = text_rect(&frame, "Hover group").center();
    test.drag_row(source, target, false);
    // Later moves aim the dragged row the same way.
    let over_header = aim(&frame, source, target);
    assert!(!test.app.group_views.get("default", "Hover group").open);
    for _ in 0..45 {
        test.frame(vec![]);
    }
    assert!(test.app.group_views.get("default", "Hover group").open);
    let frame = test.frame(vec![]);
    let inside = button_rects(&frame, &format!("Drag {}", member.spec.url))[0].center();
    test.frame(vec![egui::Event::PointerMoved(inside)]);
    test.frame(vec![]);
    assert!(test.app.group_views.get("default", "Hover group").open);
    test.frame(vec![egui::Event::PointerMoved(egui::pos2(-20.0, -20.0))]);
    test.frame(vec![]);
    assert!(!test.app.group_views.get("default", "Hover group").open);
    test.frame(vec![egui::Event::PointerMoved(over_header)]);
    for _ in 0..45 {
        test.frame(vec![]);
    }
    test.frame(vec![egui::Event::PointerButton {
        pos: over_header,
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
    let source = button_rects(&frame, &format!("Drag {}", first.spec.url))[0];
    test.drag_row(source.center(), text_rect(&frame, "B").center(), true);
    assert!(test.app.state.mod_data.groups["A"].mods.is_empty());
    assert_eq!(
        test.app.state.mod_data.groups["B"].mods,
        [second.clone(), first.clone()]
    );
    let frame = test.frame(vec![]);
    let target = button_rects(&frame, &format!("Drag {}", first.spec.url))[0];
    let source = button_rects(&frame, &format!("Drag {}", second.spec.url))[0];
    test.drag_row(
        source.center(),
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
fn outside_drops_and_no_op_sorted_drops_preserve_manual_order_and_sorting() {
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
    assert!(test.app.get_sorting_config().is_some());
}

#[test]
fn root_drops_clamp_to_the_correct_section_and_show_the_actual_boundary() {
    for dark in [false, true] {
        let mut test = TestApp::new();
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let a = test.local_mod("a.pak");
        let b = test.local_mod("b.pak");
        test.app.state.mod_data.get_active_profile_mut().mods = vec![
            ModOrGroup::Individual(a.clone()),
            ModOrGroup::Individual(b.clone()),
        ];
        for name in ["A", "B"] {
            test.app.request_edit(
                crate::state::edits::Edit::CreateGroup {
                    profile: "default".into(),
                    name: name.into(),
                },
                false,
            );
        }
        test.frame(vec![]);
        let frame = test.frame(vec![]);
        let boundary = button_rects(&frame, &format!("Drag {}", a.spec.url))[0].top();
        let outside_groups = egui::pos2(40.0, button_rects(&frame, "Drag A")[0].center().y);
        test.drag_row(
            button_rects(&frame, &format!("Drag {}", b.spec.url))[0].center(),
            outside_groups,
            false,
        );
        let frame = test.settle();
        let gap = drop_gap(&frame).expect("dragging shows where the mod lands");
        assert!(
            (gap.top() - boundary).abs() < 4.0,
            "insertion space must stay before the first standalone mod: {gap:?} {boundary}"
        );
        test.frame(vec![egui::Event::PointerButton {
            pos: outside_groups,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert_eq!(
            test.app.state.mod_data.get_active_profile().mods[2],
            ModOrGroup::Individual(b)
        );
        assert_eq!(
            test.app.state.mod_data.get_active_profile().mods[3],
            ModOrGroup::Individual(a)
        );
        let frame = test.frame(vec![]);
        test.drag_row(
            button_rects(&frame, "Drag A")[0].center(),
            egui::pos2(40.0, 350.0),
            true,
        );
        assert!(
            !test
                .app
                .state
                .mod_data
                .get_active_profile()
                .has_mixed_order()
        );
        assert!(
            matches!(&test.app.state.mod_data.get_active_profile().mods[0], ModOrGroup::Group { group_name, .. } if group_name == "B")
        );
    }
}

#[test]
fn mixed_profiles_require_confirmation_and_cancel_or_stale_accept_keeps_data() {
    use crate::state::edits::Edit;
    for cancel in [true, false] {
        let mut test = TestApp::new();
        let config = test.local_mod("legacy.pak");
        test.app.state.mod_data.get_active_profile_mut().mods = vec![
            ModOrGroup::Individual(config.clone()),
            ModOrGroup::Group {
                group_name: "default".into(),
                enabled: false,
            },
        ];
        test.app.state.mod_data.save().unwrap();
        test.app.history = super::super::history::History::new(&test.app.state.mod_data);
        let original = (**test.app.state.mod_data).clone();
        test.frame(vec![]);
        let frame = test.frame(vec![]);
        test.drag_row(
            button_rects(&frame, &format!("Drag {}", config.spec.url))[0].center(),
            egui::pos2(40.0, 350.0),
            true,
        );
        assert_eq!(**test.app.state.mod_data, original);
        assert!(test.app.delete_confirmation.is_none());
        let frame = test.frame(vec![]);
        test.click(button_rects(&frame, "Arrange groups…")[0].center());
        let frame = test.frame(vec![]);
        text_rect(&frame, "Arrange groups?");
        test.click(button_rects(&frame, if cancel { "Cancel" } else { "Arrange" })[0].center());
        assert_eq!(
            test.app
                .state
                .mod_data
                .get_active_profile()
                .has_mixed_order(),
            cancel
        );
        if cancel {
            assert_eq!(**test.app.state.mod_data, original);
            // Shift cannot bypass a load-order change, and stale confirmation cannot overwrite edits.
            test.app
                .request_edit(Edit::ArrangeProfile("default".into()), true);
            assert!(test.app.delete_confirmation.is_some());
            test.app
                .state
                .mod_data
                .get_active_profile_mut()
                .mods
                .push(ModOrGroup::Individual(config));
            let before = (**test.app.state.mod_data).clone();
            let frame = test.frame(vec![]);
            test.click(button_rects(&frame, "Arrange")[0].center());
            assert_eq!(**test.app.state.mod_data, before);
            assert!(test.app.last_action.is_some());
        } else {
            let loaded = State::init(test.app.state.dirs.clone()).unwrap();
            assert_eq!(**loaded.mod_data, **test.app.state.mod_data);
            assert!(matches!(
                &loaded.mod_data.get_active_profile().mods[0],
                ModOrGroup::Group { enabled: false, .. }
            ));
            let arranged = (**test.app.state.mod_data).clone();
            test.app.restore_history(false, None);
            assert_eq!(**test.app.state.mod_data, original);
            test.app.restore_history(true, None);
            assert_eq!(**test.app.state.mod_data, arranged);
        }
    }
}

#[test]
fn dragging_in_every_sort_mode_saves_visible_order_and_switches_to_manual() {
    for category in SortBy::iter() {
        for reverse in [false, true] {
            let mut test = TestApp::new();
            let mut zulu = test.local_mod("zulu.pak");
            zulu.priority = 20;
            zulu.enabled = false;
            let alpha = test.local_mod("alpha.pak");
            let missing = mod_config("missing-metadata");
            let configs = [zulu, alpha, missing];
            test.app.state.mod_data.get_active_profile_mut().mods = configs
                .iter()
                .cloned()
                .map(ModOrGroup::Individual)
                .collect();
            test.app.update_sorting_config(Some(category), reverse);
            test.frame(vec![]);
            let frame = test.frame(vec![]);
            let mut visible: Vec<_> = configs
                .into_iter()
                .map(|config| {
                    let rect = button_rects(&frame, &format!("Drag {}", config.spec.url))[0];
                    (rect, config)
                })
                .collect();
            visible.sort_by(|a, b| a.0.top().total_cmp(&b.0.top()));
            let source = visible[0].0.center();
            visible.rotate_left(1);
            let expected: Vec<_> = visible
                .into_iter()
                .map(|(_, config)| ModOrGroup::Individual(config))
                .collect();
            test.drag_row(source, egui::pos2(40.0, 350.0), true);
            assert_eq!(
                test.app.state.mod_data.get_active_profile().mods,
                expected,
                "{category:?}, reverse={reverse}"
            );
            assert!(test.app.get_sorting_config().is_none());
            let loaded = State::init(test.app.state.dirs.clone()).unwrap();
            assert!(loaded.config.sorting_config.is_none());
            assert_eq!(loaded.mod_data.get_active_profile().mods, expected);
        }
    }
}

#[test]
fn sorted_group_moves_preserve_visible_members_and_unrelated_groups() {
    let mut test = TestApp::new();
    let a = test.local_mod("a.pak");
    let b = test.local_mod("b.pak");
    let c = test.local_mod("c.pak");
    let d = test.local_mod("d.pak");
    for (name, mods) in [
        ("A", vec![b.clone(), a.clone()]),
        ("B", vec![d.clone(), c.clone()]),
        ("Unrelated", vec![d.clone(), c.clone()]),
    ] {
        test.app.state.mod_data.groups.insert(
            name.into(),
            ModGroup {
                mods,
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
    test.app.update_sorting_config(Some(SortBy::Name), false);
    test.open_group("A");
    test.open_group("B");
    let frame = test.frame(vec![]);
    let source = button_rects(&frame, &format!("Drag {}", a.spec.url))[0];
    let destination = button_rects(&frame, &format!("Drag {}", d.spec.url))[0].center_top()
        + egui::vec2(0.0, 1.0);
    test.drag_row(source.center(), destination, true);
    assert!(test.app.get_sorting_config().is_none());
    assert_eq!(test.app.state.mod_data.groups["A"].mods, [b]);
    assert_eq!(
        test.app.state.mod_data.groups["B"].mods,
        [c.clone(), a.clone(), d.clone()]
    );
    assert_eq!(test.app.state.mod_data.groups["Unrelated"].mods, [d, c]);
    test.app.update_sorting_config(Some(SortBy::Name), true);
    let frame = test.frame(vec![]);
    test.drag_row(
        button_rects(&frame, &format!("Drag {}", a.spec.url))[0].center(),
        egui::pos2(40.0, 350.0),
        true,
    );
    assert!(test.app.get_sorting_config().is_none());
    assert!(
        matches!(test.app.state.mod_data.get_active_profile().mods.last(), Some(ModOrGroup::Individual(config)) if *config == a)
    );
    test.app.update_sorting_config(Some(SortBy::Name), false);
    let frame = test.frame(vec![]);
    test.drag_row(
        button_rects(&frame, &format!("Drag {}", a.spec.url))[0].center(),
        text_rect(&frame, "B").center(),
        true,
    );
    assert!(test.app.get_sorting_config().is_none());
    assert_eq!(test.app.state.mod_data.groups["B"].mods.last(), Some(&a));
    test.app.update_sorting_config(Some(SortBy::Name), true);
    let frame = test.frame(vec![]);
    test.drag_row(
        button_rects(&frame, "Drag B")[0].center(),
        button_rects(&frame, "Drag A")[0].center_top() + egui::vec2(0.0, 1.0),
        true,
    );
    assert!(test.app.get_sorting_config().is_none());
    assert!(
        matches!(&test.app.state.mod_data.get_active_profile().mods[0], ModOrGroup::Group { group_name, .. } if group_name == "B")
    );
}

#[test]
fn cancelled_and_stale_sorted_drags_keep_the_sort_selection() {
    for cancel in [0, 1, 2, 3] {
        let mut test = TestApp::new();
        let a = test.local_mod("a.pak");
        let b = test.local_mod("b.pak");
        test.app.state.mod_data.get_active_profile_mut().mods =
            vec![ModOrGroup::Individual(b), ModOrGroup::Individual(a.clone())];
        test.app.update_sorting_config(Some(SortBy::Name), false);
        test.frame(vec![]);
        let frame = test.frame(vec![]);
        let target = egui::pos2(40.0, 350.0);
        test.drag_row(
            button_rects(&frame, &format!("Drag {}", a.spec.url))[0].center(),
            target,
            false,
        );
        match cancel {
            0 => {
                test.frame(vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            1 => {
                let new = test.local_mod("new.pak");
                test.app
                    .state
                    .mod_data
                    .get_active_profile_mut()
                    .mods
                    .push(ModOrGroup::Individual(new));
            }
            2 => test.app.update_sorting_config(Some(SortBy::Priority), true),
            _ => {}
        }
        let before = (**test.app.state.mod_data).clone();
        let pos = if cancel == 3 {
            egui::pos2(-20.0, -20.0)
        } else {
            target
        };
        test.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        test.frame(vec![]);
        assert_eq!(**test.app.state.mod_data, before);
        assert!(test.app.get_sorting_config().is_some());
    }
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

#[test]
fn preview_shift_closes_the_source_slot_and_opens_the_insertion_slot() {
    use crate::gui::drag_drop::preview_shift;
    // Rows centered at 10, 30, 50 and 70, each 20 tall. Dragging the row at 10 below the row at 50.
    assert_eq!(preview_shift(30.0, 10.0, Some(60.0), 20.0), -20.0);
    assert_eq!(preview_shift(50.0, 10.0, Some(60.0), 20.0), -20.0);
    assert_eq!(preview_shift(70.0, 10.0, Some(60.0), 20.0), 0.0);
    // Dragging the row at 70 above the row at 30.
    assert_eq!(preview_shift(10.0, 70.0, Some(20.0), 20.0), 0.0);
    assert_eq!(preview_shift(30.0, 70.0, Some(20.0), 20.0), 20.0);
    assert_eq!(preview_shift(50.0, 70.0, Some(20.0), 20.0), 20.0);
    // Dropping a row next to itself, or outside the list, moves nothing.
    for insert in [Some(0.0), Some(20.0), None] {
        assert_eq!(
            preview_shift(30.0, 10.0, insert, 20.0),
            if insert.is_none() { -20.0 } else { 0.0 }
        );
        assert_eq!(
            preview_shift(50.0, 10.0, insert, 20.0),
            if insert.is_none() { -20.0 } else { 0.0 }
        );
    }
}

#[test]
fn dragging_previews_the_new_order_and_the_drop_lands_without_a_jump() {
    for dark in [false, true] {
        let mut test = TestApp::new();
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let mods = ["a.pak", "b.pak", "c.pak", "d.pak"].map(|name| test.local_mod(name));
        test.app.state.mod_data.get_active_profile_mut().mods =
            mods.iter().cloned().map(ModOrGroup::Individual).collect();
        test.frame(vec![]);
        let frame = test.frame(vec![]);
        let tops = ["a.pak", "b.pak", "c.pak", "d.pak"].map(|name| text_rect(&frame, name).top());
        let step = tops[1] - tops[0];
        assert!(step > 10.0);
        let source = button_rects(&frame, &format!("Drag {}", mods[0].spec.url))[0].center();
        // The lower half of c: a lands after it.
        let destination = egui::pos2(source.x, text_rect(&frame, "c.pak").bottom() - 1.0);
        test.drag_row(source, destination, false);
        let frame = test.settle();
        let text_tops = |frame: &egui::FullOutput, name: &str| -> Vec<f32> {
            frame
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == name => Some(text.pos.y),
                    _ => None,
                })
                .collect()
        };
        // b and c slide up into the lifted row's place, d stays below the opened space.
        assert_eq!(text_tops(&frame, "b.pak"), vec![tops[0]]);
        assert_eq!(text_tops(&frame, "c.pak"), vec![tops[1]]);
        assert_eq!(text_tops(&frame, "d.pak"), vec![tops[3]]);
        // The dragged row follows the pointer instead of staying in its slot.
        let lifted = text_tops(&frame, "a.pak");
        assert_eq!(lifted.len(), 1);
        let gap = drop_gap(&frame).expect("dragging shows where the mod lands");
        // The space opens under the lifted row, in the third slot between c and d.
        assert!(
            gap.y_range().contains(lifted[0] + step / 2.0),
            "{gap:?} {lifted:?}"
        );
        assert!(
            gap.top() > tops[1] && gap.top() <= tops[2],
            "{gap:?} {tops:?}"
        );
        assert!(gap.bottom() <= tops[3], "{gap:?} {tops:?}");
        test.frame(vec![egui::Event::PointerButton {
            pos: destination,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let frame = test.frame(vec![]);
        assert_eq!(
            test.app.state.mod_data.get_active_profile().mods,
            [&mods[1], &mods[2], &mods[0], &mods[3]].map(|mc| ModOrGroup::Individual(mc.clone()))
        );
        // The reordered list is painted where the preview showed it.
        for (name, top) in ["b.pak", "c.pak", "a.pak", "d.pak"].into_iter().zip(tops) {
            assert_eq!(text_tops(&frame, name), vec![top], "{name}");
        }
        assert!(drop_gap(&frame).is_none());
    }
}

#[test]
fn escape_returns_previewed_rows_to_their_places() {
    let mut test = TestApp::new();
    let mods = ["a.pak", "b.pak", "c.pak"].map(|name| test.local_mod(name));
    test.app.state.mod_data.get_active_profile_mut().mods =
        mods.iter().cloned().map(ModOrGroup::Individual).collect();
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    let before = text_rect(&frame, "b.pak");
    let source = button_rects(&frame, &format!("Drag {}", mods[0].spec.url))[0].center();
    let destination = egui::pos2(source.x, text_rect(&frame, "c.pak").center().y + 3.0);
    test.drag_row(source, destination, false);
    let frame = test.settle();
    assert_ne!(text_rect(&frame, "b.pak"), before);
    test.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let frame = test.frame(vec![]);
    assert_eq!(text_rect(&frame, "b.pak"), before);
    assert!(drop_gap(&frame).is_none());
}

#[test]
fn dropping_into_an_open_group_grows_its_background_to_fit_the_preview() {
    let mut test = TestApp::new();
    let member = test.local_mod("member.pak");
    let dragged = test.local_mod("dragged.pak");
    test.app.state.mod_data.groups.insert(
        "Target".into(),
        ModGroup {
            mods: vec![member.clone()],
            ..Default::default()
        },
    );
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Group {
            group_name: "Target".into(),
            enabled: true,
        },
        ModOrGroup::Individual(dragged.clone()),
    ];
    test.app.group_views.get("default", "Target").open = true;
    for _ in 0..10 {
        test.frame(vec![]);
    }
    let frame = test.frame(vec![]);
    let fill = crate::gui::group_colors::palette(
        test.app.state.mod_data.groups["Target"].color,
        test.context.style().visuals.dark_mode,
    )
    .1;
    let group_height = |frame: &egui::FullOutput| {
        frame
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect.height()),
                _ => None,
            })
            .expect("group background")
    };
    let height = group_height(&frame);
    let member_rect = text_rect(&frame, "member.pak");
    let member_handle = button_rects(&frame, &format!("Drag {}", member.spec.url))[0];
    let source = button_rects(&frame, &format!("Drag {}", dragged.spec.url))[0].center();
    // Between the header and the existing member.
    let destination = egui::pos2(member_handle.center().x, member_handle.top() + 1.0);
    test.drag_row(source, destination, false);
    let frame = test.settle();
    let step = text_rect(&frame, "member.pak").top() - member_rect.top();
    assert!(step > 10.0, "the member makes room for the dropped mod");
    assert!((group_height(&frame) - height - step).abs() < 1.0);
    let gap = drop_gap(&frame).expect("dragging shows where the mod lands");
    assert!(
        gap.top() <= member_rect.top() && gap.top() > member_rect.top() - 6.0,
        "the space opens before the member: {gap:?} {member_rect:?}"
    );
    test.frame(vec![egui::Event::PointerButton {
        pos: destination,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert_eq!(
        test.app.state.mod_data.groups["Target"].mods,
        vec![dragged, member]
    );
}

#[test]
fn the_opened_space_stays_under_the_dragged_row_in_both_directions() {
    use crate::gui::drag_drop::lifted_to_layout;
    assert_eq!(lifted_to_layout(5.0, 10.0, 20.0), 5.0);
    assert_eq!(lifted_to_layout(10.0, 10.0, 20.0), 30.0);
    assert_eq!(lifted_to_layout(25.0, 10.0, 20.0), 45.0);

    let names = ["a.pak", "b.pak", "c.pak", "d.pak", "e.pak"];
    for (dragged, direction) in [(0, 1.0), (4, -1.0)] {
        let mut test = TestApp::new();
        let mods = names.map(|name| test.local_mod(name));
        test.app.state.mod_data.get_active_profile_mut().mods =
            mods.iter().cloned().map(ModOrGroup::Individual).collect();
        test.frame(vec![]);
        let frame = test.frame(vec![]);
        let tops = names.map(|name| text_rect(&frame, name).top());
        let step = tops[1] - tops[0];
        let source = button_rects(&frame, &format!("Drag {}", mods[dragged].spec.url))[0].center();
        test.frame(vec![
            egui::Event::PointerMoved(source),
            egui::Event::PointerButton {
                pos: source,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let mut gaps = Vec::new();
        for offset in (0..=(4.0 * step) as usize).step_by(3) {
            let pointer = source + egui::vec2(0.0, direction * offset as f32);
            test.frame(vec![egui::Event::PointerMoved(pointer)]);
            let frame = test.settle();
            let lifted = text_rect(&frame, names[dragged]);
            assert_eq!(
                lifted.top() - tops[dragged],
                direction * offset as f32,
                "the dragged row follows the pointer"
            );
            let gap = drop_gap(&frame).expect("dragging shows where the mod lands");
            assert!(
                (lifted.center().y - gap.center().y).abs() <= step / 2.0 + 1.0,
                "the dragged row must cover at least half of the opened space: {lifted:?} {gap:?}"
            );
            gaps.push(gap.top());
        }
        // The space visits every slot on the way.
        gaps.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        assert_eq!(gaps.len(), names.len(), "{gaps:?}");
        test.frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
}
