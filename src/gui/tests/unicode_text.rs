use super::*;

const NAMES: &[(&str, &str)] = &[
    ("简体中文矿工", "中文"),
    ("繁體中文礦工", "礦工"),
    ("日本語のドワーフ", "ドワーフ"),
    ("한국어 광부", "광부"),
    ("Русский Шахтёр", "ШАХТЁР"),
    ("Український Гірник", "ГІРНИК"),
    ("Български Миньор", "МИНЬОР"),
    ("Српски Рудар", "РУДАР"),
    ("Ελληνικά", "ΕΛΛΗΝΙΚΆ"),
    ("Árvíztűrő tükörfúrógép", "TÜKÖRFÚRÓGÉP"),
    ("İstanbul 中文 РУДАР", "рудАР"),
];

#[test]
fn unicode_names_have_real_glyphs_in_both_font_families() {
    let mut test = TestApp::new();
    for scale in [1.0, 1.5, 2.0] {
        test.context.set_pixels_per_point(scale);
        test.frame(vec![]);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let font = egui::FontId::new(14.0, family);
            for (name, _) in NAMES {
                let missing: String = test.context.fonts(|fonts| {
                    name.chars()
                        .filter(|&c| !fonts.has_glyph(&font, c))
                        .collect()
                });
                assert!(
                    missing.is_empty(),
                    "Missing glyphs for {name:?} in {font:?}: {missing}"
                );
            }
        }
    }
}

#[test]
fn unicode_search_highlights_names_and_does_not_report_false_misses() {
    for (metadata, dark) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut test = TestApp::new();
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        for (name, query) in NAMES {
            let config = if metadata {
                test.local_mod(&format!("{name}.pak"))
            } else {
                mod_config(&format!("unavailable/{name}.pak"))
            };
            test.app.state.mod_data.get_active_profile_mut().mods =
                vec![ModOrGroup::Individual(config)];
            test.app.search_string = (*query).into();
            test.full_frame(vec![]);
            let output = test.full_frame(vec![]);
            let input = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == *query => Some(text),
                    _ => None,
                })
                .expect("search input must be rendered");
            let error_color = test.context.style().visuals.error_fg_color;
            assert!(
                input
                    .galley
                    .job
                    .sections
                    .iter()
                    .all(|s| s.format.color != error_color),
                "false miss for {name:?}, metadata={metadata}"
            );
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text().contains(name) && text.galley.job.sections.iter().any(|s| s.format.background == egui::Color32::YELLOW)
            )), "name must be highlighted: {name:?}");
        }
    }
}

#[test]
fn unicode_input_group_names_and_files_survive_search_sorting_and_reload() {
    use crate::state::edits::Edit;
    for (name, query) in NAMES {
        let mut test = TestApp::new();
        let config = test.local_mod(&format!("{name}.pak"));
        let group = format!("Group {name}");
        test.app.request_edit(
            Edit::CreateGroup {
                profile: "default".into(),
                name: group.clone(),
            },
            false,
        );
        test.app
            .state
            .mod_data
            .groups
            .get_mut(&group)
            .unwrap()
            .mods
            .push(config.clone());
        test.app.state.mod_data.save().unwrap();
        let original = (**test.app.state.mod_data).clone();
        test.app.state = State::init(test.app.state.dirs.clone()).unwrap();
        assert_eq!(**test.app.state.mod_data, original);
        test.app.focus_search = true;
        test.full_frame(vec![]);
        test.full_frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled)]);
        test.full_frame(vec![egui::Event::Ime(egui::ImeEvent::Preedit(
            (*query).into(),
        ))]);
        test.full_frame(vec![egui::Event::Ime(egui::ImeEvent::Commit(
            (*query).into(),
        ))]);
        test.full_frame(vec![egui::Event::Ime(egui::ImeEvent::Disabled)]);
        assert_eq!(test.app.search_string, *query);
        for direction in [false, true] {
            test.app
                .update_sorting_config(Some(SortBy::Name), direction);
            let frame = test.full_frame(vec![]);
            text_rect(&frame, &group);
            let label = format!("{name}.pak");
            text_rect(&frame, &label);
            assert!(frame.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == label && text.galley.job.sections.iter().any(|s| s.format.background == egui::Color32::YELLOW)
            )));
            assert_eq!(**test.app.state.mod_data, original);
        }
        assert!(std::path::Path::new(&config.spec.url).is_file());
        test.app.search_string.clear();
        test.full_frame(vec![egui::Event::Paste((*query).into())]);
        assert_eq!(test.app.search_string, *query);
    }
}
