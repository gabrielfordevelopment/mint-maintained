use super::*;
use crate::state::ModListSettings;

fn nodes_with_role(output: &egui::FullOutput, role: egui::accesskit::Role) -> usize {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role)
        .count()
}

fn rendered(output: &egui::FullOutput, text: &str) -> bool {
    output.shapes.iter().any(
        |shape| matches!(&shape.shape, egui::Shape::Text(shape) if shape.galley.text() == text),
    )
}

fn three_mods(test: &mut TestApp) -> [ModConfig; 3] {
    let alpha = test.local_mod("alpha.pak");
    let mut bravo = test.local_mod("bravo.pak");
    bravo.priority = 5;
    let mut charlie = test.local_mod("charlie.pak");
    charlie.enabled = false;
    test.app.state.mod_data.get_active_profile_mut().mods = [&alpha, &bravo, &charlie]
        .map(|mc| ModOrGroup::Individual(mc.clone()))
        .to_vec();
    [alpha, bravo, charlie]
}

fn row_spacing(test: &mut TestApp) -> f32 {
    let output = test.frame(vec![]);
    text_rect(&output, "bravo.pak").top() - text_rect(&output, "alpha.pak").top()
}

#[test]
fn compact_rows_keep_their_height_and_normal_rows_are_taller() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    let compact = row_spacing(&mut test);
    test.app.state.config.mod_list.compact_thumbnails = true;
    assert_eq!(row_spacing(&mut test), compact);
    test.app.state.config.mod_list.view = ModListView::Normal;
    let normal = row_spacing(&mut test);
    assert!(normal >= mod_list::NORMAL_ROW_HEIGHT, "{normal}");
    assert!(normal > compact);
    let output = test.frame(vec![]);
    assert!(rendered(&output, "Local file"));
}

#[test]
fn priority_display_modes_control_the_row_editor() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    let spin = egui::accesskit::Role::SpinButton;
    test.app.state.config.mod_list.priority = PriorityDisplay::Always;
    assert_eq!(nodes_with_role(&test.frame(vec![]), spin), 3);
    test.app.state.config.mod_list.priority = PriorityDisplay::Hidden;
    assert_eq!(nodes_with_role(&test.frame(vec![]), spin), 0);
    // The default shows every priority, including zero, regardless of hover.
    test.app.state.config.mod_list = Default::default();
    let output = test.frame(vec![egui::Event::PointerGone]);
    assert_eq!(nodes_with_role(&output, spin), 3);
}

#[test]
fn filters_hide_rows_count_group_members_and_disable_dragging_without_editing() {
    let mut test = TestApp::new();
    let [alpha, bravo, charlie] = three_mods(&mut test);
    let mut grouped = test.local_mod("delta.pak");
    grouped.enabled = false;
    let echo = test.local_mod("echo.pak");
    test.app.state.mod_data.groups.insert(
        "Group".into(),
        ModGroup {
            mods: vec![grouped, echo],
            ..Default::default()
        },
    );
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .insert(
            0,
            ModOrGroup::Group {
                group_name: "Group".into(),
                enabled: true,
            },
        );
    let before = serde_json::to_value(&*test.app.state.mod_data).unwrap();
    test.open_group("Group");
    test.app.mod_filters.disabled = true;
    let output = test.frame(vec![]);
    assert!(rendered(&output, "charlie.pak") && rendered(&output, "delta.pak"));
    for hidden in ["alpha.pak", "bravo.pak", "echo.pak"] {
        assert!(!rendered(&output, hidden), "{hidden}");
    }
    assert!(rendered(
        &output,
        "Clear filters to reorder mods by dragging."
    ));
    let header = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some("Group (1/2 mods)"));
    assert!(header);

    test.app.mod_filters = filters::ModFilters {
        local: true,
        missing_metadata: true,
        ..Default::default()
    };
    let output = test.frame(vec![]);
    assert!(rendered(&output, "No mods match the selected filters."));
    test.app.mod_filters = Default::default();
    let output = test.frame(vec![]);
    for name in [&alpha, &bravo, &charlie] {
        assert!(rendered(&output, mod_list_name(name)));
    }
    assert!(!rendered(
        &output,
        "Clear filters to reorder mods by dragging."
    ));
    assert_eq!(
        serde_json::to_value(&*test.app.state.mod_data).unwrap(),
        before
    );
}

fn mod_list_name(mc: &ModConfig) -> &str {
    std::path::Path::new(&mc.spec.url)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
}

#[test]
fn clicking_a_sorted_row_selects_the_stored_entry_and_its_name_opens_the_page() {
    let mut test = TestApp::new();
    let zulu = test.local_mod("zulu.pak");
    let alpha = test.local_mod("alpha.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Individual(zulu),
        ModOrGroup::Individual(alpha.clone()),
    ];
    test.app.update_sorting_config(Some(SortBy::Name), false);
    let output = test.frame(vec![]);
    let alpha_rect = text_rect(&output, "alpha.pak");
    let zulu_rect = text_rect(&output, "zulu.pak");
    assert!(alpha_rect.top() < zulu_rect.top());
    test.click(egui::pos2(700.0, alpha_rect.center().y));
    let selected = test.app.details.clone().unwrap();
    assert_eq!(selected.target.index, 1);
    assert_eq!(selected.url, alpha.spec.url);

    test.app.details = None;
    let pos = zulu_rect.center();
    let mut opened = Vec::new();
    for pressed in [true, false] {
        let output = test.context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| test.app.ui_profile(ui, "default"));
            },
        );
        opened.extend(
            output
                .platform_output
                .commands
                .into_iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::OpenUrl(url) => Some(url.url),
                    _ => None,
                }),
        );
    }
    assert!(
        test.app.details.is_none(),
        "the name opens the page, not the details"
    );
    assert_eq!(opened.len(), 1);
    assert!(opened[0].ends_with("zulu.pak"));
}

#[test]
fn details_follow_moved_entries_and_close_for_removed_ones() {
    let mut test = TestApp::new();
    let [_, bravo, _] = three_mods(&mut test);
    test.app.details = Some(details::DetailsTarget {
        target: crate::state::edits::ListTarget {
            profile: "default".into(),
            group: None,
            index: 1,
        },
        url: bravo.spec.url.clone(),
    });
    let output = test.full_frame(vec![]);
    assert!(rendered(&output, "bravo.pak") && rendered(&output, "Priority"));
    assert!(rendered(&output, "Local file"));

    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .remove(0);
    test.full_frame(vec![]);
    assert_eq!(test.app.details.as_ref().unwrap().target.index, 0);

    test.full_frame(vec![]);
    let output = test.full_frame(vec![]);
    test.full_click(button_rects(&output, "Close details")[0].center(), false);
    assert!(test.app.details.is_none());

    test.app.details = Some(details::DetailsTarget {
        target: crate::state::edits::ListTarget {
            profile: "default".into(),
            group: None,
            index: 0,
        },
        url: bravo.spec.url.clone(),
    });
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .remove(0);
    test.full_frame(vec![]);
    assert!(test.app.details.is_none());
}

#[test]
fn view_controls_and_settings_persist_without_touching_mod_data() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    test.app.state.mod_data.save().unwrap();
    let before = std::fs::read(test.app.state.dirs.config_dir.join("mod_data.json")).unwrap();
    test.full_button("Normal", false);
    assert_eq!(test.app.state.config.mod_list.view, ModListView::Normal);
    let loaded = State::init(test.app.state.dirs.clone()).unwrap();
    assert_eq!(
        loaded.config.mod_list,
        ModListSettings {
            view: ModListView::Normal,
            ..Default::default()
        }
    );
    assert_eq!(
        std::fs::read(test.app.state.dirs.config_dir.join("mod_data.json")).unwrap(),
        before
    );
    for dark in [false, true] {
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        for view in [ModListView::Compact, ModListView::Normal] {
            test.app.state.config.mod_list.view = view;
            test.app.state.config.mod_list.compact_thumbnails = true;
            let output = test.full_frame(vec![]);
            assert!(rendered(&output, "alpha.pak"));
        }
    }
}

#[test]
fn long_names_truncate_inside_the_row() {
    let mut test = TestApp::new();
    let name = format!("{}.pak", "very_long_mod_name_".repeat(12));
    let long = test.local_mod(&name);
    test.app.state.mod_data.get_active_profile_mut().mods = vec![ModOrGroup::Individual(long)];
    for view in [ModListView::Compact, ModListView::Normal] {
        test.app.state.config.mod_list.view = view;
        let output = test.frame(vec![]);
        let galley = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == name => Some(text.clone()),
                _ => None,
            })
            .expect("name is rendered");
        assert!(galley.galley.elided, "{view:?}");
        let right = galley.pos.x + galley.galley.size().x;
        assert!(right <= 1000.0, "{view:?} {right}");
    }
}

#[test]
fn dialogs_stay_inside_short_main_windows_and_never_collapse() {
    let titles = ["Settings", "Groups", "Check mod files"];
    for height in [800.0, 300.0] {
        let mut test = TestApp::new();
        test.app.settings_window = Some(WindowSettings::new(&test.app.state));
        test.app.groups_window = Some(groups::GroupsWindow::default());
        test.app.lints_toggle_window = Some(WindowLintsToggle);
        let context = test.context.clone();
        let mut frame = |events: Vec<egui::Event>| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, height),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    test.app.show_settings(ctx);
                    test.app.show_groups(ctx);
                    test.app.show_lints_toggle(ctx);
                },
            )
        };
        for _ in 0..3 {
            frame(vec![]);
        }
        let rect = |title: &str| {
            context
                .memory(|memory| memory.area_rect(egui::Id::new(title)))
                .unwrap_or_else(|| panic!("{title} is shown"))
        };
        for title in titles {
            let rect = rect(title);
            assert!(rect.bottom() <= height + 0.5, "{title} {height} {rect:?}");
            assert!(rect.height() > 60.0, "{title} {height} {rect:?}");
        }
        // Double-clicking a title bar collapses collapsible egui windows. Dialogs must not.
        let output = frame(vec![]);
        let title = text_rect(&output, "Check mod files").center();
        let before = rect("Check mod files").height();
        for _ in 0..2 {
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(title),
                    egui::Event::PointerButton {
                        pos: title,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
        }
        frame(vec![]);
        assert_eq!(rect("Check mod files").height(), before);
    }
}

#[test]
fn row_striping_levels_increase_contrast_in_both_themes() {
    for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
        let distance = |striping: RowStriping| {
            striping.fill(&visuals).map(|fill| {
                // Fills are premultiplied. Compare the color actually shown over the panel.
                let base = visuals.panel_fill;
                let alpha = fill.a() as i32;
                let shown = |fill: u8, base: u8| fill as i32 + base as i32 * (255 - alpha) / 255;
                [
                    (fill.r(), base.r()),
                    (fill.g(), base.g()),
                    (fill.b(), base.b()),
                ]
                .iter()
                .map(|(a, b)| (shown(*a, *b) - *b as i32).abs())
                .sum::<i32>()
            })
        };
        assert_eq!(RowStriping::Off.fill(&visuals), None);
        assert_eq!(
            RowStriping::Subtle.fill(&visuals),
            Some(visuals.faint_bg_color)
        );
        let medium = distance(RowStriping::Medium).unwrap();
        let strong = distance(RowStriping::Strong).unwrap();
        assert!(medium > distance(RowStriping::Subtle).unwrap(), "{medium}");
        assert!(strong > medium, "{strong} {medium}");
    }
}

#[test]
fn striping_off_paints_no_row_backgrounds() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    let filled = |output: &egui::FullOutput, color: Color32| {
        output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == color))
    };
    let stripe = RowStriping::Strong
        .fill(&test.context.style().visuals)
        .unwrap();
    test.app.state.config.mod_list.striping = RowStriping::Strong;
    assert!(filled(&test.frame(vec![]), stripe));
    test.app.state.config.mod_list.striping = RowStriping::Off;
    assert!(!filled(&test.frame(vec![]), stripe));
}

#[test]
fn window_state_is_kept_with_the_selected_data_directory() {
    let directory = tempfile::tempdir().unwrap();
    let dirs = Dirs::from_path(directory.path()).unwrap();
    assert_eq!(
        super::super::window_state_path(&dirs),
        dirs.data_dir.join("window.ron")
    );
}

#[test]
fn hovered_row_controls_stay_inside_their_row() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    test.app.state.config.mod_list.striping = RowStriping::Strong;
    let output = test.frame(vec![]);
    let row = text_rect(&output, "bravo.pak");
    let switch = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.center().y - row.center().y).abs() < 3.0
                        && rect.rect.width() > rect.rect.height() * 1.5
                        && rect.corner_radius.nw as f32 * 2.0 >= rect.rect.height() - 1.0 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("enabled switch is rendered")
    };
    let idle = switch(&output);
    test.frame(vec![egui::Event::PointerMoved(idle.center())]);
    let hovered = switch(&test.frame(vec![egui::Event::PointerMoved(idle.center())]));
    assert_eq!(hovered, idle);
    assert!(hovered.height() <= test.context.style().spacing.interact_size.y + 0.01);
}

fn has_fill(output: &egui::FullOutput, color: Color32) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) if rect.fill == color => Some(rect.rect),
        _ => None,
    })
}

#[test]
fn hovered_mod_rows_are_highlighted_in_both_themes() {
    for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
        let mut test = TestApp::new();
        test.context.set_visuals(visuals.clone());
        let [alpha, ..] = three_mods(&mut test);
        let member = test.local_mod("member.pak");
        test.app.state.mod_data.groups.insert(
            "Group".into(),
            ModGroup {
                mods: vec![member],
                ..Default::default()
            },
        );
        test.app
            .state
            .mod_data
            .get_active_profile_mut()
            .mods
            .insert(
                0,
                ModOrGroup::Group {
                    group_name: "Group".into(),
                    enabled: true,
                },
            );
        test.open_group("Group");
        let hover = mod_list::row_hover_fill(&visuals);
        for stripe in RowStriping::ALL {
            assert_ne!(stripe.fill(&visuals), Some(hover), "{stripe:?}");
        }
        test.frame(vec![egui::Event::PointerGone]);
        let output = test.frame(vec![]);
        assert_eq!(has_fill(&output, hover), None);
        for name in ["alpha.pak", "member.pak"] {
            let row = text_rect(&test.frame(vec![]), name);
            test.frame(vec![egui::Event::PointerMoved(row.center())]);
            let output = test.frame(vec![egui::Event::PointerMoved(row.center())]);
            let highlighted = has_fill(&output, hover).expect("hovered row is highlighted");
            assert!(highlighted.contains(row.center()), "{name}");
            assert!(highlighted.width() > 600.0, "{name} {highlighted:?}");
        }
        test.frame(vec![egui::Event::PointerGone]);
        let output = test.frame(vec![]);
        assert_eq!(has_fill(&output, hover), None);
        assert!(alpha.enabled);
    }
}

#[test]
fn primary_button_is_blue_only_when_highlighted() {
    let context = egui::Context::default();
    for highlighted in [false, true] {
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                icons::primary_button(ui, "Install mods", highlighted);
            });
        });
        let blue = has_fill(&output, Color32::from_rgb(37, 99, 235)).is_some();
        assert_eq!(blue, highlighted);
    }
}

#[test]
fn update_dates_are_formatted_as_utc_calendar_days() {
    assert_eq!(details::format_date(0).as_deref(), Some("1970-01-01"));
    assert_eq!(
        details::format_date(1727000000).as_deref(),
        Some("2024-09-22")
    );
    assert_eq!(details::format_date(u64::MAX), None);
}

/// WCAG contrast ratio between two opaque colors.
fn contrast(a: Color32, b: Color32) -> f32 {
    let luminance = |color: Color32| {
        let channel = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn neutral_tags_stay_readable_on_the_strongest_stripes() {
    for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
        // The stripe as shown over the panel (fills are premultiplied).
        let fill = RowStriping::Strong.fill(&visuals).unwrap();
        let base = visuals.panel_fill;
        let alpha = fill.a() as u32;
        let shown =
            |fill: u8, base: u8| (fill as u32 + base as u32 * (255 - alpha) / 255).min(255) as u8;
        let stripe = Color32::from_rgb(
            shown(fill.r(), base.r()),
            shown(fill.g(), base.g()),
            shown(fill.b(), base.b()),
        );
        // Tags are disabled buttons, which egui fades halfway towards this color.
        let faded = |color| egui::ecolor::tint_color_towards(color, visuals.fade_out_to_color());
        let text = faded(crate::gui::mod_list::neutral_tag_text(&visuals));
        assert!(
            contrast(text, stripe) >= 3.0,
            "{:?} {text:?} on {stripe:?}: {}",
            visuals.dark_mode,
            contrast(text, stripe)
        );
        if !visuals.dark_mode {
            // The previous gray was the unreadable case.
            assert!(contrast(faded(Color32::GRAY), stripe) < 2.0);
        }
    }
}

#[test]
fn row_lines_separate_rows_only_when_enabled() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    let color = test
        .context
        .style()
        .visuals
        .widgets
        .noninteractive
        .bg_stroke
        .color;
    let lines = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .filter(|shape| matches!(&shape.shape,
                egui::Shape::LineSegment { points, stroke }
                    if points[0].y == points[1].y && stroke.color == color && points[1].x - points[0].x > 500.0))
            .count()
    };
    assert_eq!(lines(&test.frame(vec![])), 0);
    test.app.state.config.mod_list.row_lines = true;
    let output = test.frame(vec![]);
    assert_eq!(lines(&output), 2, "one line between each pair of rows");
    // Lines sit between rows, not across their text.
    let alpha = text_rect(&output, "alpha.pak");
    let bravo = text_rect(&output, "bravo.pak");
    assert!(
        output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::LineSegment { points, stroke }
            if stroke.color == color && points[0].y > alpha.bottom() && points[0].y < bravo.top()))
    );
    for striping in [RowStriping::Off, RowStriping::Strong] {
        test.app.state.config.mod_list.striping = striping;
        assert_eq!(lines(&test.frame(vec![])), 2);
    }
    // The selection outline replaces the lines on its edges, so they do not look thicker.
    let output = test.frame(vec![]);
    let alpha_row = text_rect(&output, "alpha.pak");
    test.click(egui::pos2(700.0, alpha_row.center().y));
    assert!(test.app.details.is_some());
    assert_eq!(
        lines(&test.frame(vec![])),
        1,
        "only the line between bravo and charlie"
    );
    let bravo_row = text_rect(&output, "bravo.pak");
    test.click(egui::pos2(700.0, bravo_row.center().y));
    assert_eq!(
        lines(&test.frame(vec![])),
        0,
        "both lines touch the selected bravo"
    );
}

#[test]
fn clicking_a_rows_free_space_shows_details_while_controls_keep_their_clicks() {
    let mut test = TestApp::new();
    let [alpha, bravo, _] = three_mods(&mut test);
    let member = test.local_mod("member.pak");
    test.app.state.mod_data.groups.insert(
        "Group".into(),
        ModGroup {
            mods: vec![member.clone()],
            ..Default::default()
        },
    );
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .insert(
            0,
            ModOrGroup::Group {
                group_name: "Group".into(),
                enabled: true,
            },
        );
    test.open_group("Group");
    let output = test.frame(vec![]);
    let bravo_row = text_rect(&output, "bravo.pak");
    test.click(egui::pos2(700.0, bravo_row.center().y));
    let selected = test.app.details.clone().expect("a row click shows details");
    assert_eq!(selected.url, bravo.spec.url);
    assert_eq!(selected.target.group, None);

    // A group member opens its own details.
    let member_row = text_rect(&output, "member.pak");
    test.click(egui::pos2(700.0, member_row.center().y));
    let selected = test.app.details.clone().unwrap();
    assert_eq!(selected.url, member.spec.url);
    assert_eq!(selected.target.group.as_deref(), Some("Group"));

    // Controls inside a row keep their own action.
    let output = test.frame(vec![]);
    let alpha_row = text_rect(&output, "alpha.pak");
    let toggle = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) if (circle.center.y - alpha_row.center().y).abs() < 2.0 => {
                Some(circle.center)
            }
            _ => None,
        })
        .expect("toggle is rendered");
    test.click(toggle);
    assert!(alpha.enabled);
    assert!(
        test.app
            .state
            .mod_data
            .get_active_profile()
            .mods
            .iter()
            .any(|entry| matches!(entry, ModOrGroup::Individual(mc) if mc.spec == alpha.spec && !mc.enabled)),
        "the toggle switches the mod off"
    );
    assert_eq!(test.app.details.clone().unwrap().url, member.spec.url);

    // The group header still toggles the group instead of showing details.
    let output = test.frame(vec![]);
    let header = text_rect(&output, "Group");
    test.click(egui::pos2(700.0, header.center().y));
    assert!(!test.app.group_views.get("default", "Group").open);
    assert_eq!(test.app.details.clone().unwrap().url, member.spec.url);
}

#[test]
fn row_stripes_cover_the_spacing_between_rows() {
    for (view, striping) in [
        (ModListView::Compact, RowStriping::Strong),
        (ModListView::Normal, RowStriping::Medium),
    ] {
        let mut test = TestApp::new();
        three_mods(&mut test);
        test.app.state.config.mod_list.view = view;
        test.app.state.config.mod_list.striping = striping;
        let step = row_spacing(&mut test);
        let fill = striping.fill(&test.context.style().visuals).unwrap();
        let output = test.frame(vec![]);
        let stripe = has_fill(&output, fill).expect("the second row is striped");
        let bravo = text_rect(&output, "bravo.pak");
        assert!(stripe.contains(bravo.center()));
        assert!(
            (stripe.height() - step).abs() < 0.5,
            "{view:?}: the stripe fills the whole row slot: {} vs {step}",
            stripe.height()
        );
    }
}

fn escape() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// A frame with the details panel, shown before the list as in the application.
fn frame_with_details(test: &mut TestApp, events: Vec<egui::Event>) -> egui::FullOutput {
    test.context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            test.app.show_details_panel(ctx);
            egui::CentralPanel::default().show(ctx, |ui| test.app.ui_profile(ui, "default"));
            test.app.show_delete_confirmation(ctx);
        },
    )
}

#[test]
fn rows_select_from_the_whole_slot_and_the_outline_matches_the_stripe() {
    let mut test = TestApp::new();
    let [alpha, bravo, _] = three_mods(&mut test);
    test.app.state.config.mod_list.striping = RowStriping::Strong;
    let step = row_spacing(&mut test);
    let fill = RowStriping::Strong
        .fill(&test.context.style().visuals)
        .unwrap();
    let output = test.frame(vec![]);
    let slot = has_fill(&output, fill).expect("bravo's row is striped");
    let spacing = test.context.style().spacing.item_spacing.y;
    assert!(spacing >= 2.0, "rows are separated by spacing");

    // Just inside the spacing above and below bravo's controls, the click still selects a row.
    test.click(egui::pos2(700.0, slot.top() + 0.5));
    assert_eq!(test.app.details.clone().unwrap().url, bravo.spec.url);
    test.click(egui::pos2(700.0, slot.top() - 0.5));
    assert_eq!(test.app.details.clone().unwrap().url, alpha.spec.url);
    test.click(egui::pos2(700.0, slot.bottom() - 0.5));
    assert_eq!(test.app.details.clone().unwrap().url, bravo.spec.url);

    // The selected row is blue, framed by an outline over the same slot as the stripe.
    let output = test.frame(vec![]);
    let visuals = test.context.style().visuals.clone();
    let selected_fill = crate::gui::mod_list::row_selected_fill(&visuals);
    assert_ne!(
        selected_fill,
        crate::gui::mod_list::row_hover_fill(&visuals)
    );
    for striping in [
        RowStriping::Subtle,
        RowStriping::Medium,
        RowStriping::Strong,
    ] {
        assert_ne!(Some(selected_fill), striping.fill(&visuals));
    }
    assert_eq!(
        has_fill(&output, selected_fill).unwrap().y_range(),
        slot.y_range()
    );
    let (index, outline) = output
        .shapes
        .iter()
        .enumerate()
        .find_map(|(index, shape)| match &shape.shape {
            egui::Shape::Rect(rect) if rect.stroke == visuals.selection.stroke => {
                Some((index, rect.rect))
            }
            _ => None,
        })
        .expect("the selected row is outlined");
    assert_eq!(outline.y_range(), slot.y_range());
    assert!((outline.height() - step).abs() < 0.5);
    // Nothing painted later covers the outline's edges.
    assert!(
        !output.shapes[index + 1..]
            .iter()
            .any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill.a() > 0
                && rect.rect.intersects(outline)
                && !outline.contains_rect(rect.rect.shrink(1.0)))),
        "a later fill overlaps the outline"
    );
}

#[test]
fn escape_clears_the_selection_unless_another_action_uses_it() {
    let mut test = TestApp::new();
    let [alpha, bravo, _] = three_mods(&mut test);
    frame_with_details(&mut test, vec![]);
    let output = frame_with_details(&mut test, vec![]);
    let bravo_row = text_rect(&output, "bravo.pak");
    let free_space = egui::pos2(bravo_row.right() + 40.0, bravo_row.center().y);
    for pressed in [true, false] {
        frame_with_details(
            &mut test,
            vec![
                egui::Event::PointerMoved(free_space),
                egui::Event::PointerButton {
                    pos: free_space,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert_eq!(test.app.details.clone().unwrap().url, bravo.spec.url);
    frame_with_details(&mut test, vec![escape()]);
    assert!(test.app.details.is_none(), "Escape clears the selection");

    // Escape cancels a pending confirmation first.
    test.app.details = Some(crate::gui::details::DetailsTarget {
        target: crate::state::edits::ListTarget {
            profile: "default".into(),
            group: None,
            index: 0,
        },
        url: alpha.spec.url.clone(),
    });
    test.app.request_edit(
        crate::state::edits::Edit::DeleteEntry(crate::state::edits::ListTarget {
            profile: "default".into(),
            group: None,
            index: 2,
        }),
        false,
    );
    assert!(test.app.delete_confirmation.is_some());
    frame_with_details(&mut test, vec![escape()]);
    frame_with_details(&mut test, vec![]);
    assert!(test.app.details.is_some());
    assert_eq!(test.app.state.mod_data.get_active_profile().mods.len(), 3);

    // Escape cancels a drag without clearing the selection.
    test.app.delete_confirmation = None;
    let output = frame_with_details(&mut test, vec![]);
    let handle = button_rects(&output, &format!("Drag {}", alpha.spec.url))[0].center();
    frame_with_details(
        &mut test,
        vec![
            egui::Event::PointerMoved(handle),
            egui::Event::PointerButton {
                pos: handle,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    for dy in [10.0, 20.0, 30.0] {
        frame_with_details(
            &mut test,
            vec![egui::Event::PointerMoved(handle + egui::vec2(0.0, dy))],
        );
    }
    assert!(egui::DragAndDrop::has_any_payload(&test.context));
    frame_with_details(&mut test, vec![escape()]);
    assert!(!egui::DragAndDrop::has_any_payload(&test.context));
    assert!(
        test.app.details.is_some(),
        "cancelling a drag keeps the selection"
    );
}

#[test]
fn details_panel_layout_is_stable_whether_or_not_it_scrolls() {
    let mut test = TestApp::new();
    let [alpha, ..] = three_mods(&mut test);
    test.app.details = Some(crate::gui::details::DetailsTarget {
        target: crate::state::edits::ListTarget {
            profile: "default".into(),
            group: None,
            index: 0,
        },
        url: alpha.spec.url.clone(),
    });
    let visuals = test.context.style().visuals.clone();
    let mut run = |height: f32| {
        test.context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, height),
                )),
                ..Default::default()
            },
            |ctx| {
                test.app.show_details_panel(ctx);
                egui::CentralPanel::default().show(ctx, |ui| test.app.ui_profile(ui, "default"));
            },
        )
    };
    let texts = |output: &egui::FullOutput| -> Vec<(String, egui::Pos2)> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos)),
                _ => None,
            })
            .collect()
    };
    let card = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == visuals.faint_bg_color
                        && rect.stroke == visuals.widgets.noninteractive.bg_stroke =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("the overview card is shown")
    };
    let mut widths = Vec::new();
    // Short windows scroll the panel, tall ones do not. The crossing must not flicker.
    for height in (140..=460).step_by(4) {
        let height = height as f32;
        for _ in 0..3 {
            run(height);
        }
        let first = run(height);
        let second = run(height);
        assert_eq!(
            texts(&first),
            texts(&second),
            "layout changes between frames at {height}"
        );
        assert_eq!(
            card(&first),
            card(&second),
            "card changes between frames at {height}"
        );
        widths.push(card(&second).width());
    }
    assert!(
        widths.windows(2).all(|pair| pair[0] == pair[1]),
        "the content width does not depend on the scroll bar: {widths:?}"
    );
    assert_eq!(
        test.app.state.config.mod_list.details_width, None,
        "an unchanged panel width is never saved"
    );
}

#[test]
fn right_clicking_anywhere_on_a_mod_row_opens_its_menu() {
    let mut test = TestApp::new();
    let [_, bravo, _] = three_mods(&mut test);
    let member = test.local_mod("member.pak");
    test.app.state.mod_data.groups.insert(
        "Group".into(),
        ModGroup {
            mods: vec![member.clone()],
            ..Default::default()
        },
    );
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .insert(
            0,
            ModOrGroup::Group {
                group_name: "Group".into(),
                enabled: true,
            },
        );
    test.open_group("Group");
    let right_click = |test: &mut TestApp, pos: egui::Pos2| {
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
        test.frame(vec![])
    };
    for (name, url, group) in [
        ("bravo.pak", &bravo.spec.url, None),
        ("member.pak", &member.spec.url, Some("Group")),
    ] {
        test.app.details = None;
        let output = test.frame(vec![]);
        let row = text_rect(&output, name);
        // Far from the name, on the row's free space.
        let menu = right_click(&mut test, egui::pos2(700.0, row.center().y));
        assert!(
            rendered(&menu, "Open in browser"),
            "{name}: the row opens the mod menu"
        );
        assert!(
            test.app.details.is_none(),
            "a right click does not select the row"
        );
        assert!(rendered(&menu, "Move to shared group…"));
        assert!(!rendered(&menu, "Groups are shared across profiles."));
        assert!(!rendered(&menu, "Hide details"));
        test.click(text_rect(&menu, "Show details").center());
        let selected = test.app.details.clone().expect("the menu shows details");
        assert_eq!(&selected.url, url);
        assert_eq!(selected.target.group.as_deref(), group);
        assert!(
            !rendered(&test.frame(vec![]), "Open in browser"),
            "the menu closes"
        );

        // On the selected row, the menu hides the details instead.
        let menu = right_click(&mut test, egui::pos2(700.0, row.center().y));
        assert!(!rendered(&menu, "Show details"));
        test.click(text_rect(&menu, "Hide details").center());
        assert!(
            test.app.details.is_none(),
            "{name}: Hide details closes the panel"
        );
        test.frame(vec![]);
    }
    // The name keeps the same menu.
    let output = test.frame(vec![]);
    let menu = right_click(&mut test, text_rect(&output, "bravo.pak").center());
    assert!(rendered(&menu, "Open in browser"));
}

#[test]
fn search_shows_only_matching_rows_without_editing() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    let delta = test.local_mod("delta.pak");
    let echo = test.local_mod("echo.pak");
    test.app.state.mod_data.groups.insert(
        "Sounds".into(),
        ModGroup {
            mods: vec![delta, echo],
            ..Default::default()
        },
    );
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .insert(
            0,
            ModOrGroup::Group {
                group_name: "Sounds".into(),
                enabled: true,
            },
        );
    let before = serde_json::to_value(&*test.app.state.mod_data).unwrap();
    let header = |output: &egui::FullOutput, label: &str| {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some(label))
    };
    let all = [
        "alpha.pak",
        "bravo.pak",
        "charlie.pak",
        "delta.pak",
        "echo.pak",
    ];
    let shown = |output: &egui::FullOutput| -> Vec<&str> {
        all.into_iter()
            .filter(|name| rendered(output, name))
            .collect()
    };

    // A standalone match, case-insensitive. The collapsed group has no match and is hidden.
    test.app.search_string = "BRAV".into();
    let output = test.frame(vec![]);
    assert_eq!(shown(&output), ["bravo.pak"]);
    assert!(!header(&output, "Sounds (0/2 mods)") && !rendered(&output, "Sounds"));
    assert!(rendered(
        &output,
        "Clear the search to reorder mods by dragging."
    ));

    // A member match shows the collapsed group expanded with only that member.
    assert!(!test.app.group_views.get("default", "Sounds").open);
    test.app.search_string = "ech".into();
    for _ in 0..10 {
        test.frame(vec![]);
    }
    let output = test.frame(vec![]);
    assert_eq!(shown(&output), ["echo.pak"]);
    assert!(header(&output, "Sounds (1/2 mods)"));
    assert!(
        !test.app.group_views.get("default", "Sounds").open,
        "searching does not change the saved expansion"
    );

    // A group name match shows all of its members.
    test.app.search_string = "sound".into();
    for _ in 0..10 {
        test.frame(vec![]);
    }
    let output = test.frame(vec![]);
    assert_eq!(shown(&output), ["delta.pak", "echo.pak"]);

    // No match offers to clear the search.
    test.app.search_string = "nothing like this".into();
    let output = test.frame(vec![]);
    assert!(shown(&output).is_empty());
    assert!(rendered(&output, "No mods match the search."));
    test.click(text_rect(&output, "Clear search").center());
    assert!(test.app.search_string.is_empty());
    // The group closes again once its collapse animation ends.
    for _ in 0..10 {
        test.frame(vec![]);
    }
    let output = test.frame(vec![]);
    assert_eq!(shown(&output), ["alpha.pak", "bravo.pak", "charlie.pak"]);
    assert!(!rendered(
        &output,
        "Clear the search to reorder mods by dragging."
    ));

    // Search and chips combine.
    test.app.search_string = "a".into();
    test.app.mod_filters.disabled = true;
    let output = test.frame(vec![]);
    assert_eq!(shown(&output), ["charlie.pak"]);
    assert!(rendered(
        &output,
        "Clear the search and filters to reorder mods by dragging."
    ));
    assert_eq!(
        serde_json::to_value(&*test.app.state.mod_data).unwrap(),
        before
    );
}

#[test]
fn collapsing_a_group_while_searching_keeps_its_saved_expansion() {
    let mut test = TestApp::new();
    three_mods(&mut test);
    let delta = test.local_mod("delta.pak");
    test.app.state.mod_data.groups.insert(
        "Sounds".into(),
        ModGroup {
            mods: vec![delta],
            ..Default::default()
        },
    );
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .insert(
            0,
            ModOrGroup::Group {
                group_name: "Sounds".into(),
                enabled: true,
            },
        );
    let settle = |test: &mut TestApp| {
        for _ in 0..10 {
            test.frame(vec![]);
        }
        test.frame(vec![])
    };
    test.app.search_string = "delta".into();
    let output = settle(&mut test);
    assert!(
        rendered(&output, "delta.pak"),
        "searching shows the group expanded"
    );
    test.click(text_rect(&output, "Sounds").center());
    let output = settle(&mut test);
    assert!(
        !rendered(&output, "delta.pak"),
        "the header collapses it during the search"
    );
    assert!(!test.app.group_views.get("default", "Sounds").open);
    test.click(text_rect(&output, "Sounds").center());
    assert!(rendered(&settle(&mut test), "delta.pak"));
    let header = text_rect(&settle(&mut test), "Sounds").center();
    test.click(header);

    test.app.search_string.clear();
    settle(&mut test);
    assert!(
        !test.app.group_views.get("default", "Sounds").open,
        "the saved state is unchanged"
    );
    test.app.search_string = "delta".into();
    assert!(
        rendered(&settle(&mut test), "delta.pak"),
        "a new search starts expanded"
    );
}

#[test]
fn the_details_panel_leaves_room_in_narrow_windows_and_saves_only_dragged_widths() {
    let mut test = TestApp::new();
    let [alpha, ..] = three_mods(&mut test);
    test.app.details = Some(crate::gui::details::DetailsTarget {
        target: crate::state::edits::ListTarget {
            profile: "default".into(),
            group: None,
            index: 0,
        },
        url: alpha.spec.url.clone(),
    });
    test.app.state.config.mod_list.details_width = Some(400.0);
    let run = |test: &mut TestApp, width: f32, events: Vec<egui::Event>| {
        test.context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                test.app.show_details_panel(ctx);
                egui::CentralPanel::default().show(ctx, |ui| test.app.ui_profile(ui, "default"));
            },
        )
    };
    let panel_left = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "OVERVIEW" => Some(text.pos.x),
                _ => None,
            })
            .expect("the panel is shown")
    };
    // The saved width is used while the window has room.
    for _ in 0..3 {
        run(&mut test, 1000.0, vec![]);
    }
    let wide = 1000.0 - panel_left(&run(&mut test, 1000.0, vec![]));
    // A narrow window squeezes the panel to 45% of its width without saving that width.
    for _ in 0..3 {
        run(&mut test, 480.0, vec![]);
    }
    let output = run(&mut test, 480.0, vec![]);
    assert!(
        480.0 - panel_left(&output) <= 480.0 * 0.45,
        "{}",
        panel_left(&output)
    );
    assert_eq!(test.app.state.config.mod_list.details_width, Some(400.0));
    assert_eq!(
        crate::gui::details::panel_width_range(480.0),
        *crate::gui::details::panel_width_range(480.0).start()..=216.0
    );
    assert_eq!(
        crate::gui::details::panel_width_range(2000.0),
        240.0..=560.0
    );

    // Widening the window again restores the saved width.
    for _ in 0..3 {
        run(&mut test, 1000.0, vec![]);
    }
    let before = panel_left(&run(&mut test, 1000.0, vec![]));
    assert_eq!(
        1000.0 - before,
        wide,
        "the panel returns to its saved width"
    );

    // Dragging the panel's edge saves the new width once the pointer is released.
    let edge = egui::pos2(before - 12.0, 300.0);
    let mut x = edge.x;
    let mut found = false;
    // The resize handle sits on the panel's outer edge, found from the cursor feedback.
    for dx in 0..30 {
        let pos = egui::pos2(edge.x - dx as f32 + 10.0, 300.0);
        let output = run(&mut test, 1000.0, vec![egui::Event::PointerMoved(pos)]);
        if output.platform_output.cursor_icon == egui::CursorIcon::ResizeHorizontal
            || output.platform_output.cursor_icon == egui::CursorIcon::ResizeEast
            || output.platform_output.cursor_icon == egui::CursorIcon::ResizeWest
        {
            x = pos.x;
            found = true;
            break;
        }
    }
    assert!(found, "the panel can be resized");
    let start = egui::pos2(x, 300.0);
    run(
        &mut test,
        1000.0,
        vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    for step in 1..=5 {
        run(
            &mut test,
            1000.0,
            vec![egui::Event::PointerMoved(
                start - egui::vec2(10.0 * step as f32, 0.0),
            )],
        );
    }
    assert_eq!(
        test.app.state.config.mod_list.details_width,
        Some(400.0),
        "not while dragging"
    );
    run(
        &mut test,
        1000.0,
        vec![egui::Event::PointerButton {
            pos: start - egui::vec2(50.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    run(&mut test, 1000.0, vec![]);
    let saved = test.app.state.config.mod_list.details_width.unwrap();
    // The width follows the pointer, so where the edge was grabbed shifts it slightly.
    assert!(
        (saved - 450.0).abs() < 15.0,
        "the dragged width is saved: {saved}"
    );
}

#[test]
fn keyboard_focus_on_a_mod_row_is_visible_and_enter_shows_its_details() {
    let mut test = TestApp::new();
    let [alpha, ..] = three_mods(&mut test);
    let key = |key: egui::Key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let selection = test.context.style().visuals.selection.stroke;
    let outlined = |output: &egui::FullOutput| {
        output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke == selection),
        )
    };
    let output = test.frame(vec![]);
    assert!(!outlined(&output));
    let labelled = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some("alpha.pak details"));
    assert!(labelled, "screen readers can name the row");

    // Tab reaches the first row's target before its controls.
    test.frame(vec![key(egui::Key::Tab)]);
    let output = test.frame(vec![]);
    assert!(outlined(&output), "the focused row is outlined");
    assert!(test.app.details.is_none());
    test.frame(vec![key(egui::Key::Enter)]);
    test.frame(vec![]);
    assert_eq!(test.app.details.clone().unwrap().url, alpha.spec.url);
}

#[test]
fn arrow_keys_step_the_selection_through_shown_mods() {
    let mut test = TestApp::new();
    let [alpha, bravo, charlie] = three_mods(&mut test);
    let member = test.local_mod("member.pak");
    let hidden = test.local_mod("hidden.pak");
    for (name, mods) in [("Open", vec![member.clone()]), ("Closed", vec![hidden])] {
        test.app.state.mod_data.groups.insert(
            name.into(),
            ModGroup {
                mods,
                ..Default::default()
            },
        );
    }
    let profile = &mut test.app.state.mod_data.get_active_profile_mut().mods;
    for name in ["Closed", "Open"] {
        profile.insert(
            0,
            ModOrGroup::Group {
                group_name: name.into(),
                enabled: true,
            },
        );
    }
    test.open_group("Open");
    let key = |key: egui::Key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let selected = |test: &TestApp| test.app.details.as_ref().map(|d| d.url.clone());

    // Without a selection the keys do nothing.
    test.frame(vec![key(egui::Key::ArrowDown)]);
    assert_eq!(selected(&test), None);

    let output = test.frame(vec![]);
    test.click(egui::pos2(
        700.0,
        text_rect(&output, "member.pak").center().y,
    ));
    assert_eq!(selected(&test), Some(member.spec.url.clone()));
    // Members of the open group, then the standalone mods. The closed group is skipped.
    for expected in [&alpha, &bravo, &charlie, &charlie] {
        test.frame(vec![key(egui::Key::ArrowDown)]);
        assert_eq!(selected(&test), Some(expected.spec.url.clone()));
    }
    for expected in [&bravo, &alpha, &member, &member] {
        test.frame(vec![key(egui::Key::ArrowUp)]);
        assert_eq!(selected(&test), Some(expected.spec.url.clone()));
    }

    // Sorting and search change the order and the shown mods.
    test.app.update_sorting_config(Some(SortBy::Name), false);
    test.app.search_string = ".pak".into();
    let output = test.frame(vec![]);
    test.click(egui::pos2(
        700.0,
        text_rect(&output, "charlie.pak").center().y,
    ));
    test.frame(vec![key(egui::Key::ArrowUp)]);
    assert_eq!(selected(&test), Some(bravo.spec.url.clone()));
    test.app.search_string = "a".into();
    test.frame(vec![]);
    test.frame(vec![key(egui::Key::ArrowDown)]);
    assert_eq!(
        selected(&test),
        Some(charlie.spec.url.clone()),
        "bravo is hidden, so Down goes on to charlie"
    );
}

#[test]
fn keyboard_focus_follows_the_selection_when_stepping_rows() {
    let mut test = TestApp::new();
    let [alpha, bravo, _] = three_mods(&mut test);
    let key = |key: egui::Key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    test.frame(vec![]);
    test.frame(vec![key(egui::Key::Tab)]);
    test.frame(vec![key(egui::Key::Enter)]);
    assert_eq!(test.app.details.clone().unwrap().url, alpha.spec.url);
    test.frame(vec![key(egui::Key::ArrowDown)]);
    test.frame(vec![]);
    assert_eq!(test.app.details.clone().unwrap().url, bravo.spec.url);
    // Enter now activates bravo's row, so the focus moved with the selection.
    test.app.details = None;
    test.frame(vec![key(egui::Key::Enter)]);
    test.frame(vec![]);
    assert_eq!(test.app.details.clone().unwrap().url, bravo.spec.url);
}

#[test]
fn escape_in_the_search_box_clears_the_search_and_keeps_the_details_open() {
    let mut test = TestApp::new();
    let [alpha, ..] = three_mods(&mut test);
    test.app.details = Some(crate::gui::details::DetailsTarget {
        target: crate::state::edits::ListTarget {
            profile: "default".into(),
            group: None,
            index: 0,
        },
        url: alpha.spec.url.clone(),
    });
    let escape = || egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    test.full_frame(vec![]);
    test.app.focus_search = true;
    test.full_frame(vec![]);
    test.full_frame(vec![egui::Event::Text("bra".into())]);
    test.full_frame(vec![]);
    assert_eq!(test.app.search_string, "bra");
    test.full_frame(vec![escape()]);
    test.full_frame(vec![]);
    assert!(
        test.app.search_string.is_empty(),
        "Escape clears the search"
    );
    assert!(test.app.details.is_some(), "the details panel stays open");
    // With nothing focused, Escape closes the panel.
    test.full_frame(vec![escape()]);
    assert!(test.app.details.is_none());
}

#[test]
fn emptying_the_list_while_its_scroll_bar_is_dragged_does_not_break_scrolling() {
    let mut test = TestApp::new();
    let mods: Vec<_> = (0..40)
        .map(|index| test.local_mod(&format!("mod{index:02}.pak")))
        .collect();
    test.app.state.mod_data.get_active_profile_mut().mods =
        mods.into_iter().map(ModOrGroup::Individual).collect();
    let run = |test: &mut TestApp, events: Vec<egui::Event>| {
        test.context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 300.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| test.app.ui_profile(ui, "default"));
            },
        )
    };
    run(&mut test, vec![]);
    // The floating scroll bar sits at the list's right edge.
    let bar = egui::pos2(800.0 - 12.0, 60.0);
    run(&mut test, vec![egui::Event::PointerMoved(bar)]);
    for _ in 0..5 {
        run(&mut test, vec![]);
    }
    run(
        &mut test,
        vec![egui::Event::PointerButton {
            pos: bar,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    // A search without matches empties the list while the bar is still held and fading out.
    test.app.search_string = "no such mod".into();
    for step in 1..=8 {
        run(
            &mut test,
            vec![egui::Event::PointerMoved(
                bar + egui::vec2(0.0, 10.0 * step as f32),
            )],
        );
    }
    run(
        &mut test,
        vec![egui::Event::PointerButton {
            pos: bar,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    test.app.search_string.clear();
    let output = run(&mut test, vec![]);
    assert!(
        rendered(&output, "mod00.pak"),
        "the list scrolls and renders normally again"
    );
}

#[test]
fn filter_facts_read_enabled_state_live_and_refresh_after_background_work() {
    let mut test = TestApp::new();
    let [alpha, _, charlie] = three_mods(&mut test);
    test.app.mod_filters = filters::ModFilters {
        disabled: true,
        ..Default::default()
    };
    let output = test.frame(vec![]);
    assert!(rendered(&output, mod_list_name(&charlie)));
    assert!(!rendered(&output, mod_list_name(&alpha)));
    // Enabled state is not cached, so toggling applies on the next frame.
    for entry in &mut test.app.state.mod_data.get_active_profile_mut().mods {
        if let ModOrGroup::Individual(mc) = entry {
            mc.enabled = !mc.enabled;
        }
    }
    let output = test.frame(vec![]);
    assert!(rendered(&output, mod_list_name(&alpha)));
    assert!(!rendered(&output, mod_list_name(&charlie)));
    assert!(test.app.facts.contains(&alpha.spec.url));

    // A finished background operation can change metadata, so cached facts are dropped. Only
    // mods still in the profile are looked up again.
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .retain(|entry| !matches!(entry, ModOrGroup::Individual(mc) if mc.spec == alpha.spec));
    let rid = test.app.request_counter.next();
    test.app
        .tx
        .try_send(message::Message::OperationFailed(rid))
        .unwrap();
    test.app.has_run_init = true;
    test.app.state.config.drg_pak_path = None;
    let _ = test.context.run(egui::RawInput::default(), |ctx| {
        eframe::App::update(&mut test.app, ctx, &mut eframe::Frame::_new_kittest())
    });
    assert!(!test.app.facts.contains(&alpha.spec.url));
    assert!(test.app.facts.contains(&charlie.spec.url));
}
