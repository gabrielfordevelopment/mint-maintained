use super::*;
use crate::state::edits::{Edit, ListTarget};

fn notification_frame(test: &mut TestApp, time: f64) -> egui::FullOutput {
    test.context.run(
        egui::RawInput {
            time: Some(time),
            ..Default::default()
        },
        |ctx| {
            test.app.show_notifications(ctx);
        },
    )
}

#[test]
fn notification_removal_requires_saved_change_and_cancel_stays_silent() {
    let mut test = TestApp::new();
    let config = test.local_mod("Example.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![ModOrGroup::Individual(config)];
    let edit = Edit::DeleteEntry(ListTarget {
        profile: "default".into(),
        group: None,
        index: 0,
    });
    test.app.request_edit(edit.clone(), false);
    test.full_button("Cancel", false);
    let frame = notification_frame(&mut test, 1.0);
    assert!(button_rects(&frame, "Dismiss notification").is_empty());
    test.app.request_edit(edit, true);
    notification_frame(&mut test, 2.0);
    let frame = notification_frame(&mut test, 2.5);
    assert!(
        text_rect(
            &frame,
            "Removed \"Example.pak\" from profile \"default\". Mod files were kept."
        )
        .is_positive()
    );
    assert_eq!(button_rects(&frame, "Dismiss notification").len(), 1);
}

#[test]
fn notification_failed_save_has_details_without_success_and_is_not_repeated() {
    let mut test = TestApp::new();
    let path = test.app.state.dirs.config_dir.join("mod_data.json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    test.app.request_edit(
        Edit::CreateGroup {
            profile: "default".into(),
            name: "New group".into(),
        },
        false,
    );
    notification_frame(&mut test, 1.0);
    let frame = notification_frame(&mut test, 1.5);
    assert_eq!(button_rects(&frame, "Details").len(), 1);
    assert!(text_rect(&frame, "Changes could not be saved. Keep the app open and correct the file permissions before trying again.").is_positive());
    assert!(test.app.last_action.as_ref().unwrap().notified);
    let frame = notification_frame(&mut test, 30.0);
    assert_eq!(button_rects(&frame, "Dismiss notification").len(), 1);
}

#[test]
fn notification_copy_filters_disabled_entries_and_can_be_dismissed() {
    for dark in [false, true] {
        let mut test = TestApp::new();
        test.context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let active = test.local_mod("Active.pak");
        let mut disabled = test.local_mod("Disabled.pak");
        disabled.enabled = false;
        let excluded = test.local_mod("Excluded group member.pak");
        test.app.state.mod_data.groups.insert(
            "Disabled group".into(),
            ModGroup {
                mods: vec![excluded],
                ..Default::default()
            },
        );
        test.app.state.mod_data.get_active_profile_mut().mods = vec![
            ModOrGroup::Individual(active),
            ModOrGroup::Individual(disabled),
            ModOrGroup::Group {
                group_name: "Disabled group".into(),
                enabled: false,
            },
        ];
        test.full_button("Copy profile mods", false);
        for _ in 0..30 {
            test.full_frame(vec![]);
        }
        let frame = test.full_frame(vec![]);
        assert!(
            text_rect(
                &frame,
                "Copied 1 active mod link.\nDisabled mods and groups were excluded."
            )
            .is_positive()
        );
        test.full_button("Dismiss notification", false);
        for _ in 0..30 {
            test.full_frame(vec![]);
        }
        assert!(button_rects(&test.full_frame(vec![]), "Dismiss notification").is_empty());
        test.app
            .state
            .mod_data
            .get_active_profile_mut()
            .mods
            .clear();
        test.full_button("Copy profile mods", false);
        for _ in 0..30 {
            test.full_frame(vec![]);
        }
        assert!(text_rect(&test.full_frame(vec![]), "No active mod links to copy.").is_positive());
    }
}

#[test]
fn notification_moves_stay_silent() {
    let mut test = TestApp::new();
    let first = test.local_mod("First.pak");
    let second = test.local_mod("Second.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![
        ModOrGroup::Individual(first),
        ModOrGroup::Individual(second),
    ];
    test.app.request_edit(
        Edit::MoveEntry {
            source: ListTarget {
                profile: "default".into(),
                group: None,
                index: 0,
            },
            destination: ListTarget {
                profile: "default".into(),
                group: None,
                index: 2,
            },
        },
        false,
    );
    let frame = notification_frame(&mut test, 1.0);
    assert!(button_rects(&frame, "Dismiss notification").is_empty());
    assert!(
        matches!(&test.app.state.mod_data.get_active_profile().mods[0],
        ModOrGroup::Individual(config) if config.spec.url.ends_with("Second.pak"))
    );
}
