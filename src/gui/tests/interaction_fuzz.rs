//! Reproducible random interaction with the whole window, looking for panics and invalid data.
//! Pointer input stays inside the mod list and details panel, away from the update, install
//! and settings controls, so no network request or game installation change can start.

use super::*;
use crate::state::{ModListView, RowStriping};

/// Small deterministic generator, so a failing seed can be replayed.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn unit(&mut self) -> f32 {
        self.below(10_000) as f32 / 10_000.0
    }
}

fn run(test: &mut TestApp, size: egui::Vec2, events: Vec<egui::Event>) -> egui::FullOutput {
    test.app.has_run_init = true;
    test.app.state.config.drg_pak_path = None;
    test.context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| eframe::App::update(&mut test.app, ctx, &mut eframe::Frame::_new_kittest()),
    )
}

fn button(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn drag_handles(output: &egui::FullOutput) -> Vec<egui::Rect> {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .map(|update| {
            update
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
                .collect()
        })
        .unwrap_or_default()
}

fn setup(test: &mut TestApp) {
    let mut mods: Vec<_> = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]
        .iter()
        .map(|name| test.local_mod(&format!("{name}.pak")))
        .collect();
    mods[2].enabled = false;
    mods[3].priority = -3;
    // A file that does not exist and so never gets metadata.
    let missing = mod_config(test.directory.path().join("missing.pak").to_str().unwrap());
    test.app.state.mod_data.groups.insert(
        "First group".into(),
        ModGroup {
            mods: vec![mods[0].clone(), mods[1].clone()],
            ..Default::default()
        },
    );
    test.app.state.mod_data.groups.insert(
        "Second group".into(),
        ModGroup {
            mods: vec![mods[2].clone()],
            ..Default::default()
        },
    );
    test.app
        .state
        .mod_data
        .groups
        .insert("Empty".into(), ModGroup::default());
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Group {
            group_name: "First group".into(),
            enabled: true,
        },
        ModOrGroup::Group {
            group_name: "Second group".into(),
            enabled: false,
        },
        ModOrGroup::Group {
            group_name: "Empty".into(),
            enabled: true,
        },
        ModOrGroup::Individual(mods[3].clone()),
        ModOrGroup::Individual(mods[4].clone()),
        ModOrGroup::Individual(mods[5].clone()),
        ModOrGroup::Individual(missing),
    ];
    test.app.state.mod_data.save().unwrap();
}

fn fuzz(seed: u64, steps: usize) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let mut test = TestApp::new();
    setup(&mut test);
    let mut rng = Lcg(seed);
    let sizes = [
        egui::vec2(1100.0, 700.0),
        egui::vec2(900.0, 600.0),
        egui::vec2(480.0, 320.0),
        egui::vec2(640.0, 420.0),
        egui::vec2(1400.0, 900.0),
    ];
    let mut size = sizes[0];
    let mut output = run(&mut test, size, vec![]);
    for step in 0..steps {
        // The list starts below the toolbar, search and filter rows and ends above the footer.
        let area = egui::Rect::from_min_max(
            egui::pos2(0.0, 150.0_f32.min(size.y * 0.4)),
            egui::pos2(size.x, size.y - 60.0),
        );
        let point = |rng: &mut Lcg| {
            egui::pos2(
                area.left() + rng.unit() * area.width(),
                area.top() + rng.unit() * area.height(),
            )
        };
        match rng.below(16) {
            0..=4 => {
                let pos = point(&mut rng);
                let kind = if rng.below(4) == 0 {
                    egui::PointerButton::Secondary
                } else {
                    egui::PointerButton::Primary
                };
                run(
                    &mut test,
                    size,
                    vec![egui::Event::PointerMoved(pos), button(pos, kind, true)],
                );
                run(&mut test, size, vec![button(pos, kind, false)]);
            }
            5..=6 => {
                let handles = drag_handles(&output);
                if handles.is_empty() {
                    continue;
                }
                let from = handles[rng.below(handles.len() as u64) as usize].center();
                let to = point(&mut rng);
                run(
                    &mut test,
                    size,
                    vec![
                        egui::Event::PointerMoved(from),
                        button(from, egui::PointerButton::Primary, true),
                    ],
                );
                for fraction in [0.25, 0.5, 0.75, 1.0] {
                    run(
                        &mut test,
                        size,
                        vec![egui::Event::PointerMoved(from.lerp(to, fraction))],
                    );
                }
                let events = if rng.below(5) == 0 {
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    vec![button(to, egui::PointerButton::Primary, false)]
                };
                run(&mut test, size, events);
                run(
                    &mut test,
                    size,
                    vec![button(to, egui::PointerButton::Primary, false)],
                );
            }
            7 => {
                let key = [
                    egui::Key::Escape,
                    egui::Key::ArrowUp,
                    egui::Key::ArrowDown,
                    egui::Key::Tab,
                    egui::Key::Enter,
                ][rng.below(5) as usize];
                run(
                    &mut test,
                    size,
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            8 => {
                size = sizes[rng.below(sizes.len() as u64) as usize];
                run(&mut test, size, vec![]);
            }
            9 => {
                let filters = &mut test.app.mod_filters;
                match rng.below(8) {
                    0 => filters.enabled ^= true,
                    1 => filters.disabled ^= true,
                    2 => filters.update_available ^= true,
                    3 => filters.missing_metadata ^= true,
                    4 => filters.modio ^= true,
                    5 => filters.web ^= true,
                    6 => filters.local ^= true,
                    _ => *filters = Default::default(),
                }
            }
            10 => {
                test.app.search_string = ["", "", "a", "ALPHA", "group", "zz", "missing", "."]
                    [rng.below(8) as usize]
                    .into();
            }
            11 => {
                let sort = SortBy::iter().nth(rng.below(SortBy::iter().count() as u64) as usize);
                let sort = sort.filter(|_| rng.below(4) != 0);
                test.app.update_sorting_config(sort, rng.below(2) == 0);
            }
            12 => {
                let list = &mut test.app.state.config.mod_list;
                match rng.below(5) {
                    0 => {
                        list.view = if list.view == ModListView::Compact {
                            ModListView::Normal
                        } else {
                            ModListView::Compact
                        }
                    }
                    1 => list.row_lines ^= true,
                    2 => list.compact_thumbnails ^= true,
                    3 => {
                        list.striping = [
                            RowStriping::Off,
                            RowStriping::Subtle,
                            RowStriping::Medium,
                            RowStriping::Strong,
                        ][rng.below(4) as usize]
                    }
                    _ => list.show_tags ^= true,
                }
            }
            13 => {
                let key = if rng.below(2) == 0 {
                    egui::Key::Z
                } else {
                    egui::Key::Y
                };
                run(
                    &mut test,
                    size,
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::COMMAND,
                    }],
                );
            }
            _ => {
                // Scroll the list or the details panel.
                let pos = point(&mut rng);
                run(&mut test, size, vec![egui::Event::PointerMoved(pos)]);
                run(
                    &mut test,
                    size,
                    vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, (rng.unit() - 0.5) * 400.0),
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
        }
        output = run(&mut test, size, vec![]);
        if let Err(error) = test.app.state.mod_data.validate() {
            panic!("seed {seed}, step {step}: invalid mod data: {error}");
        }
    }
}

#[test]
fn random_interaction_never_panics_or_corrupts_mod_data() {
    for seed in [1, 7, 42, 2026] {
        fuzz(seed, 400);
    }
}
