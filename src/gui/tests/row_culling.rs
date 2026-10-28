//! Rows outside the visible part of the list are skipped. These tests drive two windows with the
//! same input, one drawing every row, and require identical visible output and edits.
use super::*;

const SIZE: egui::Vec2 = egui::vec2(900.0, 600.0);

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

fn run(test: &mut TestApp, events: Vec<egui::Event>) -> egui::FullOutput {
    test.app.has_run_init = true;
    test.app.state.config.drg_pak_path = None;
    test.context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            events,
            ..Default::default()
        },
        |ctx| eframe::App::update(&mut test.app, ctx, &mut eframe::Frame::_new_kittest()),
    )
}

/// Texts that can be seen, with their positions.
fn visible_texts(output: &egui::FullOutput) -> Vec<(String, [i32; 4])> {
    let mut texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                let shown = rect.intersect(clipped.clip_rect);
                (shown.is_positive() && !text.galley.text().is_empty()).then(|| {
                    let r = [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
                        .map(|v| (v * 10.0).round() as i32);
                    (text.galley.text().to_owned(), r)
                })
            }
            _ => None,
        })
        .collect();
    texts.sort();
    texts
}

fn drag_handles(output: &egui::FullOutput) -> Vec<egui::Rect> {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| node.label().is_some_and(|l| l.starts_with("Drag ")))
        .filter_map(|(_, node)| node.bounds())
        .map(|r| {
            egui::Rect::from_min_max(
                egui::pos2(r.x0 as f32, r.y0 as f32),
                egui::pos2(r.x1 as f32, r.y1 as f32),
            )
        })
        .filter(|r| r.min.y > 60.0 && r.max.y < SIZE.y - 80.0)
        .collect()
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// A long profile: groups (one large) before many standalone mods, some disabled or without
/// metadata. Both windows use the same files.
fn long_profile(a: &mut TestApp, b: &mut TestApp, standalone: usize) {
    let mods: Vec<_> = (0..standalone + 40)
        .map(|i| {
            let mut mc = a.local_mod(&format!("mod_{i:03}.pak"));
            mc.enabled = i % 7 != 3;
            mc.priority = (i % 5) as i32 - 2;
            mc
        })
        .collect();
    let missing = mod_config(a.directory.path().join("missing.pak").to_str().unwrap());
    for test in [&mut *a, &mut *b] {
        test.app.state.mod_data.groups.insert(
            "Large group".into(),
            ModGroup {
                mods: mods[..30].to_vec(),
                ..Default::default()
            },
        );
        test.app.state.mod_data.groups.insert(
            "Small group".into(),
            ModGroup {
                mods: mods[30..40].to_vec(),
                ..Default::default()
            },
        );
        let mut entries = vec![
            ModOrGroup::Group {
                group_name: "Large group".into(),
                enabled: true,
            },
            ModOrGroup::Group {
                group_name: "Small group".into(),
                enabled: true,
            },
        ];
        entries.extend(mods[40..].iter().cloned().map(ModOrGroup::Individual));
        entries.push(ModOrGroup::Individual(missing.clone()));
        test.app.state.mod_data.get_active_profile_mut().mods = entries;
        test.app.state.mod_data.save().unwrap();
    }
    b.app.cull_rows = false;
}

fn same(
    a: &mut TestApp,
    b: &mut TestApp,
    events: Vec<egui::Event>,
    step: &str,
) -> egui::FullOutput {
    let out_a = run(a, events.clone());
    let out_b = run(b, events);
    let (texts_a, texts_b) = (visible_texts(&out_a), visible_texts(&out_b));
    if texts_a != texts_b {
        let only = |x: &[(String, [i32; 4])], y: &[(String, [i32; 4])]| {
            x.iter()
                .filter(|t| !y.contains(t))
                .cloned()
                .collect::<Vec<_>>()
        };
        panic!(
            "{step}: only with skipping {:?}, only fully drawn {:?}",
            only(&texts_a, &texts_b),
            only(&texts_b, &texts_a)
        );
    }
    assert_eq!(
        serde_json::to_value(&*a.app.state.mod_data).unwrap(),
        serde_json::to_value(&*b.app.state.mod_data).unwrap(),
        "{step}"
    );
    assert_eq!(
        a.app.details.as_ref().map(|d| d.target.clone()),
        b.app.details.as_ref().map(|d| d.target.clone()),
        "{step}"
    );
    assert_eq!(
        a.context.memory(|m| m.focused()),
        b.context.memory(|m| m.focused()),
        "{step}"
    );
    assert_eq!(
        a.app.state.config.sorting_config.is_some(),
        b.app.state.config.sorting_config.is_some(),
        "{step}"
    );
    out_a
}

fn differential(seed: u64, steps: usize, standalone: usize) {
    let mut a = TestApp::new();
    let mut b = TestApp::new();
    long_profile(&mut a, &mut b, standalone);
    let mut rng = Rng(seed);
    let mut output = same(&mut a, &mut b, vec![], "first frame");
    // Open both groups.
    for name in ["Large group", "Small group"] {
        if let Some(rect) = output.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == name => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            _ => None,
        }) {
            let pos = rect.center();
            same(
                &mut a,
                &mut b,
                vec![egui::Event::PointerMoved(pos), button(pos, true)],
                name,
            );
            output = same(&mut a, &mut b, vec![button(pos, false)], name);
        }
    }
    let list = egui::Rect::from_min_max(egui::pos2(20.0, 90.0), egui::pos2(500.0, SIZE.y - 90.0));
    for step in 0..steps {
        let at = |rng: &mut Rng| {
            egui::pos2(
                list.left() + rng.below(list.width() as u64) as f32,
                list.top() + rng.below(list.height() as u64) as f32,
            )
        };
        let action = rng.below(15);
        let label = format!("seed {seed} step {step} action {action}");
        match action {
            0..=2 => {
                let pos = at(&mut rng);
                let delta = (rng.below(1600) as f32 - 800.0).round();
                output = same(
                    &mut a,
                    &mut b,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, delta),
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    &label,
                );
            }
            3 => {
                // Click free space of a row, which selects it.
                let pos = egui::pos2(
                    560.0 + rng.below(100) as f32,
                    list.top() + rng.below(list.height() as u64) as f32,
                );
                same(
                    &mut a,
                    &mut b,
                    vec![egui::Event::PointerMoved(pos), button(pos, true)],
                    &label,
                );
                same(&mut a, &mut b, vec![button(pos, false)], &label);
                output = same(&mut a, &mut b, vec![], &label);
            }
            4 | 5 => {
                let count = 1 + rng.below(25);
                let k = if rng.below(2) == 0 {
                    egui::Key::ArrowDown
                } else {
                    egui::Key::ArrowUp
                };
                for _ in 0..count {
                    output = same(&mut a, &mut b, vec![key(k)], &label);
                }
            }
            6 | 7 => {
                let handles = drag_handles(&output);
                if handles.is_empty() {
                    continue;
                }
                let start = handles[rng.below(handles.len() as u64) as usize].center();
                same(
                    &mut a,
                    &mut b,
                    vec![egui::Event::PointerMoved(start), button(start, true)],
                    &label,
                );
                let mut pos = start;
                let moves = 2 + rng.below(12);
                let target_y = list.top() - 20.0 + rng.below(list.height() as u64 + 40) as f32;
                for i in 1..=moves {
                    pos = egui::pos2(
                        start.x + rng.below(120) as f32,
                        start.y + (target_y - start.y) * i as f32 / moves as f32,
                    );
                    same(&mut a, &mut b, vec![egui::Event::PointerMoved(pos)], &label);
                }
                // Releasing applies the move, or a release outside the list cancels it.
                same(&mut a, &mut b, vec![button(pos, false)], &label);
                output = same(&mut a, &mut b, vec![], &label);
            }
            8 => {
                let view = if rng.below(2) == 0 {
                    ModListView::Compact
                } else {
                    ModListView::Normal
                };
                a.app.state.config.mod_list.view = view;
                b.app.state.config.mod_list.view = view;
                output = same(&mut a, &mut b, vec![], &label);
            }
            9 => {
                output = same(&mut a, &mut b, vec![key(egui::Key::Escape)], &label);
            }
            10 => {
                // Typing in the search box scrolls to the first match.
                let search =
                    ["", "", "mod_1", "05", "group", "zz", "missing"][rng.below(7) as usize];
                for test in [&mut a, &mut b] {
                    test.app.search_string = search.into();
                    test.app.scroll_to_match = !search.is_empty();
                }
                output = same(&mut a, &mut b, vec![], &label);
            }
            11 => {
                let choice = rng.below(5);
                for test in [&mut a, &mut b] {
                    let filters = &mut test.app.mod_filters;
                    match choice {
                        0 => filters.enabled ^= true,
                        1 => filters.disabled ^= true,
                        2 => filters.missing_metadata ^= true,
                        3 => filters.local ^= true,
                        _ => *filters = Default::default(),
                    }
                }
                output = same(&mut a, &mut b, vec![], &label);
            }
            12 => {
                let sort = SortBy::iter().nth(rng.below(SortBy::iter().count() as u64) as usize);
                let sort = sort.filter(|_| rng.below(3) != 0);
                let ascending = rng.below(2) == 0;
                for test in [&mut a, &mut b] {
                    test.app.update_sorting_config(sort, ascending);
                }
                output = same(&mut a, &mut b, vec![], &label);
            }
            13 => {
                // Collapse or expand a group from its header.
                let name = ["Large group", "Small group"][rng.below(2) as usize];
                let header = output.shapes.iter().find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == name => {
                        Some(egui::Rect::from_min_size(t.pos, t.galley.size()).center())
                    }
                    _ => None,
                });
                if let Some(pos) = header.filter(|pos| list.contains(*pos)) {
                    same(
                        &mut a,
                        &mut b,
                        vec![egui::Event::PointerMoved(pos), button(pos, true)],
                        &label,
                    );
                    output = same(&mut a, &mut b, vec![button(pos, false)], &label);
                }
            }
            _ => {
                // Open a row's menu, scroll the list under it, then close it.
                let pos = egui::pos2(
                    560.0 + rng.below(100) as f32,
                    list.top() + rng.below(list.height() as u64) as f32,
                );
                let secondary = |pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                same(
                    &mut a,
                    &mut b,
                    vec![egui::Event::PointerMoved(pos), secondary(true)],
                    &label,
                );
                same(&mut a, &mut b, vec![secondary(false)], &label);
                same(&mut a, &mut b, vec![], &label);
                output = same(&mut a, &mut b, vec![key(egui::Key::Escape)], &label);
            }
        }
        // Let animations settle the same way in both windows.
        for _ in 0..2 {
            output = same(&mut a, &mut b, vec![], &label);
        }
    }
}

#[test]
fn skipped_rows_match_a_fully_drawn_list() {
    for seed in [1, 2] {
        differential(seed, 70, 160);
    }
}

#[test]
#[ignore = "long stress run, run explicitly"]
fn skipped_rows_match_a_fully_drawn_list_long() {
    // For example `ROW_CULLING_SEEDS=3,4`.
    let seeds = std::env::var("ROW_CULLING_SEEDS").unwrap_or_else(|_| "3,4,5,6,7,8".into());
    for seed in seeds.split(',').map(|seed| seed.trim().parse().unwrap()) {
        differential(seed, 400, 360);
    }
}

#[test]
fn only_rows_near_the_view_are_built() {
    let mut a = TestApp::new();
    let mut b = TestApp::new();
    long_profile(&mut a, &mut b, 360);
    let nodes = |output: &egui::FullOutput| {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, node)| node.label().is_some_and(|l| l.starts_with("Drag ")))
            .count()
    };
    // The first frame measures every row, later frames skip the distant ones.
    run(&mut a, vec![]);
    let culled = nodes(&run(&mut a, vec![]));
    let full = nodes(&run(&mut b, vec![]));
    assert!(full > 360, "{full}");
    assert!(culled < 60, "{culled}");
}

fn arrow_keys_reach_the_last_mod(cull: bool) {
    let mut a = TestApp::new();
    let mut b = TestApp::new();
    long_profile(&mut a, &mut b, 200);
    a.app.cull_rows = cull;
    let output = run(&mut a, vec![]);
    run(&mut a, vec![]);
    // Select the first standalone mod by clicking its row.
    let first = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "mod_040.pak" => Some(t.pos.y + 6.0),
            _ => None,
        })
        .unwrap();
    let pos = egui::pos2(600.0, first);
    run(
        &mut a,
        vec![egui::Event::PointerMoved(pos), button(pos, true)],
    );
    run(&mut a, vec![button(pos, false)]);
    assert_eq!(a.app.details.as_ref().unwrap().target.index, 2);
    let last = a.app.state.mod_data.profiles["default"].mods.len() - 1;
    for _ in 0..last + 5 {
        let mut output = run(&mut a, vec![key(egui::Key::ArrowDown)]);
        let index = a.app.details.as_ref().unwrap().target.index;
        let ModOrGroup::Individual(mc) = &a.app.state.mod_data.profiles["default"].mods[index]
        else {
            panic!("a group was selected");
        };
        let name = mc.spec.url.rsplit(['/', '\\']).next().unwrap().to_owned();
        // Scrolling to the selected row is animated.
        let mut frames = 0;
        // Mods without metadata show their whole path.
        while !visible_texts(&output)
            .iter()
            .any(|(t, _)| t.ends_with(&name))
        {
            frames += 1;
            assert!(frames < 30, "{name} is not shown");
            output = run(&mut a, vec![]);
        }
    }
    assert_eq!(a.app.details.as_ref().unwrap().target.index, last);
}

#[test]
fn arrow_keys_reach_the_last_mod_through_skipped_rows() {
    arrow_keys_reach_the_last_mod(true);
}

#[test]
fn arrow_keys_reach_the_last_mod_in_a_fully_drawn_list() {
    arrow_keys_reach_the_last_mod(false);
}
