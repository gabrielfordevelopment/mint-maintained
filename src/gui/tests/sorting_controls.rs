use super::*;

fn choose(test: &mut TestApp, current: &str, next: &str) {
    let frame = test.full_frame(vec![]);
    test.full_click(text_rect(&frame, current).center(), false);
    test.full_frame(vec![]);
    let frame = test.full_frame(vec![]);
    let option = frame
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == next => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap();
    test.full_click(option.center(), false);
    test.full_frame(vec![]);
}

#[test]
fn dropdown_and_direction_preserve_manual_order_and_saved_legacy_direction() {
    for dark in [false, true] {
        let mut test = TestApp::new();
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let zulu = test.local_mod("zulu.pak");
        let alpha = test.local_mod("alpha.pak");
        test.app.state.mod_data.get_active_profile_mut().mods =
            vec![ModOrGroup::Individual(zulu), ModOrGroup::Individual(alpha)];
        let manual = test.app.state.mod_data.get_active_profile().mods.clone();
        test.full_frame(vec![]);
        test.full_button("Ascending", false);
        assert!(test.app.get_sorting_config().is_none());
        choose(&mut test, "Manual", "Name");
        let frame = test.full_frame(vec![]);
        assert!(text_rect(&frame, "alpha.pak").top() < text_rect(&frame, "zulu.pak").top());
        assert!(!test.app.get_sorting_config().unwrap().is_ascending);
        test.full_button("Ascending", false);
        let frame = test.full_frame(vec![]);
        assert!(text_rect(&frame, "zulu.pak").top() < text_rect(&frame, "alpha.pak").top());
        let loaded = State::init(test.app.state.dirs.clone()).unwrap();
        assert!(loaded.config.sorting_config.as_ref().unwrap().is_ascending);
        test.app.state.config.sorting_config = loaded.config.sorting_config.clone();
        let frame = test.full_frame(vec![]);
        assert_eq!(button_rects(&frame, "Descending").len(), 1);
        choose(&mut test, "Name", "Name");
        assert!(test.app.get_sorting_config().unwrap().is_ascending);
        choose(&mut test, "Name", "Priority");
        assert!(test.app.get_sorting_config().unwrap().is_ascending);
        choose(&mut test, "Priority", "Manual");
        assert!(test.app.get_sorting_config().is_none());
        assert_eq!(test.app.state.mod_data.get_active_profile().mods, manual);
        let frame = test.full_frame(vec![]);
        assert!(text_rect(&frame, "zulu.pak").top() < text_rect(&frame, "alpha.pak").top());
    }
}

#[test]
fn sorting_dropdown_closes_on_outside_click_without_changing_selection() {
    let mut test = TestApp::new();
    test.full_frame(vec![]);
    let frame = test.full_frame(vec![]);
    test.full_click(text_rect(&frame, "Manual").center(), false);
    test.full_frame(vec![]);
    let frame = test.full_frame(vec![]);
    text_rect(&frame, "Priority");
    test.full_click(egui::pos2(800.0, 450.0), false);
    let frame = test.full_frame(vec![]);
    assert!(!frame.shapes.iter().any(
        |shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Priority")
    ));
    assert!(test.app.get_sorting_config().is_none());
}
