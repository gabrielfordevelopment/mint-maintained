use super::*;
use crate::state::GroupColor;

fn color_at(output: &egui::FullOutput, pos: egui::Pos2) -> Color32 {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if shape.clip_rect.contains(pos)
                    && rect.rect.contains(pos)
                    && rect.fill != Color32::TRANSPARENT =>
            {
                Some(rect.fill)
            }
            _ => None,
        })
        .next_back()
        .unwrap()
}

#[test]
fn group_header_and_gutter_use_the_palette_while_members_keep_alternating_rows() {
    for dark in [false, true] {
        let mut test = TestApp::new();
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        test.context.style_mut(|style| style.animation_time = 0.0);
        let first = test.local_mod("first.pak");
        let second = test.local_mod("second.pak");
        test.app.state.mod_data.groups.insert(
            "Colored".into(),
            ModGroup {
                mods: vec![first, second],
                ..Default::default()
            },
        );
        test.app
            .state
            .mod_data
            .get_active_profile_mut()
            .mods
            .push(ModOrGroup::Group {
                group_name: "Colored".into(),
                enabled: true,
            });
        test.open_group("Colored");
        assert_eq!(GroupColor::iter().count(), 15);
        for color in GroupColor::iter() {
            test.app
                .state
                .mod_data
                .groups
                .get_mut("Colored")
                .unwrap()
                .color = color;
            let frame = test.frame(vec![]);
            let header = text_rect(&frame, "Colored");
            let a = text_rect(&frame, "first.pak");
            let b = text_rect(&frame, "second.pak");
            let fill = super::super::group_colors::palette(color, dark).1;
            assert_eq!(color_at(&frame, egui::pos2(950.0, header.center().y)), fill);
            assert_eq!(color_at(&frame, egui::pos2(70.0, a.center().y)), fill);
            assert_ne!(color_at(&frame, egui::pos2(950.0, a.center().y)), fill);
            assert_ne!(
                color_at(&frame, egui::pos2(950.0, a.center().y)),
                color_at(&frame, egui::pos2(950.0, b.center().y))
            );
        }
    }
}

#[test]
fn group_context_palette_changes_and_saves_the_selected_color() {
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
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    let pos = text_rect(&frame, "default").center();
    for pressed in [true, false] {
        test.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    test.frame(vec![]);
    let frame = test.frame(vec![]);
    for color in GroupColor::iter() {
        let (name, _) = super::super::group_colors::palette(color, false);
        assert_eq!(button_rects(&frame, name).len(), 1);
    }
    test.click(button_rects(&frame, "Blue")[0].center());
    assert_eq!(
        test.app.state.mod_data.groups["default"].color,
        GroupColor::Blue
    );
    let loaded = State::init(test.app.state.dirs.clone()).unwrap();
    assert_eq!(loaded.mod_data.groups["default"].color, GroupColor::Blue);
}

#[test]
fn group_header_tail_toggles_and_opens_the_menu_without_changing_members() {
    for dark in [false, true] {
        let mut test = TestApp::new();
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        test.context.style_mut(|style| style.animation_time = 0.0);
        let mut member = test.local_mod("member.pak");
        member.enabled = false;
        test.app
            .state
            .mod_data
            .groups
            .get_mut("default")
            .unwrap()
            .mods = vec![member];
        test.app
            .state
            .mod_data
            .get_active_profile_mut()
            .mods
            .push(ModOrGroup::Group {
                group_name: "default".into(),
                enabled: true,
            });
        test.frame(vec![]);
        let frame = test.frame(vec![]);
        text_rect(&frame, "(1 mod)");
        let pos = egui::pos2(950.0, text_rect(&frame, "default").center().y);
        test.click(pos);
        let frame = test.frame(vec![]);
        text_rect(&frame, "member.pak");
        test.click(pos);
        let frame = test.frame(vec![]);
        assert!(button_rects(&frame, "Copy URL").is_empty());
        for pressed in [true, false] {
            test.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        test.frame(vec![]);
        let frame = test.frame(vec![]);
        assert_eq!(button_rects(&frame, "Blue").len(), 1);
        assert!(!test.app.group_views.get("default", "default").open);
        assert!(!test.app.state.mod_data.groups["default"].mods[0].enabled);
        test.frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        test.app
            .state
            .mod_data
            .groups
            .get_mut("default")
            .unwrap()
            .mods
            .clear();
        let frame = test.frame(vec![]);
        text_rect(&frame, "(0 mods)");
    }
}
