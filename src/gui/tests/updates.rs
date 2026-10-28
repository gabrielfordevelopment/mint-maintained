use super::*;
use crate::providers::refresh::{RefreshFailure, RefreshReport};

#[test]
fn file_check_results_distinguish_unselected_clean_and_flagged_checks() {
    for (selected, flagged, expected) in [
        (
            false,
            false,
            "No checks were selected. Enable a check, then generate a new report.",
        ),
        (true, false, "No problems found. Checks completed: 1."),
        (true, true, "Checks completed: 1. Checks with findings: 1."),
    ] {
        let mut test = TestApp::new();
        test.app.has_run_init = true;
        test.app.lint_report_window = Some(WindowLintReport);
        test.app.lint_report = Some(crate::mod_lints::LintReport {
            empty_archive_mods: selected.then(|| {
                if flagged {
                    [ModSpecification::new("fixture.pak".into())].into()
                } else {
                    Default::default()
                }
            }),
            ..Default::default()
        });
        test.full_frame(vec![]);
        assert!(text_rect(&test.full_frame(vec![]), expected).is_positive());
    }
}

fn report() -> RefreshReport {
    RefreshReport {
        checked: 3,
        total: 3,
        failures: vec![RefreshFailure {
            spec: ModSpecification::new("https://mod.io/g/drg/m/example#1".into()),
            name: "Example mod".into(),
            reason: "mod.io temporarily limited requests. Try again after the wait.",
            details: "HTTP 429 Too Many Requests\nRetry-After: 57".into(),
        }],
        ..Default::default()
    }
}

#[test]
fn copied_refresh_reports_redact_all_display_fields_without_corrupting_retry_targets() {
    let mut report = report();
    let secret = "fixture-private-token";
    report.failures[0].spec.url = format!("https://mod.io/g/drg/m/example?token={secret}");
    report.failures[0].name = secret.into();
    report.failures[0].details = report.failures[0].spec.url.clone();
    report.new_versions = vec![secret.into()];
    report.save_error = Some(secret.into());
    let targets = report.retry_specs();
    crate::gui::updates::redact_report(&mut report, &[secret.into()]);
    assert!(!report.details().contains(secret));
    assert!(report.details().contains("[redacted]"));
    assert_eq!(report.retry_specs(), targets);
}

#[test]
fn refresh_report_uses_mod_names_hides_technical_details_and_returns_failed_specs() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let report = report();
    let mut retry = None;
    let mut frame = |events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 400.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    retry = crate::gui::updates::report_ui(ui, &report, true);
                });
            },
        )
    };
    frame(vec![]);
    let output = frame(vec![]);
    assert!(text_rect(&output, "Example mod").is_positive());
    assert!(
        !output.shapes.iter().any(
            |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().contains("HTTP 429"))
        )
    );
    assert!(
        text_rect(
            &output,
            "mod.io temporarily limited requests. Try again after the wait."
        )
        .is_positive()
    );
    let pos = button_rects(&output, "Retry failed")[0].center();
    for pressed in [true, false] {
        frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert_eq!(retry, Some(report.retry_specs()));
}

#[test]
fn footer_status_stays_above_buttons_in_both_themes_and_narrow_windows() {
    for dark in [false, true] {
        for width in [540.0, 900.0] {
            let mut test = TestApp::new();
            test.app.has_run_init = true;
            test.app.state.config.drg_pak_path = None;
            test.context.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            test.app.last_action = Some(LastAction::failure("A very long status message that must be truncated before it can overlap any action buttons. ".repeat(12)));
            let mut frame = || {
                test.context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        eframe::App::update(&mut test.app, ctx, &mut eframe::Frame::_new_kittest());
                    },
                )
            };
            frame();
            let output = frame();
            let details = *button_rects(&output, "Details").last().unwrap();
            let buttons: Vec<_> = [
                "Check mod files",
                "Check mod updates",
                "Uninstall mods",
                "Install mods",
                "Open settings",
            ]
            .into_iter()
            .map(|n| button_rects(&output, n)[0])
            .collect();
            for (index, button) in buttons.iter().enumerate() {
                assert!(details.bottom() < button.top());
                assert!((8.0..=10.0).contains(&(500.0 - button.bottom())));
                assert!(
                    (28.0..=32.0).contains(&(button.center().y - details.center().y)),
                    "status: {details:?}, button: {button:?}"
                );
                assert!(button.left() >= 0.0 && button.right() <= width);
                for other in &buttons[index + 1..] {
                    assert!(!button.intersects(*other));
                }
            }
        }
    }
}

#[tokio::test]
async fn update_scope_popup_closes_outside_and_single_mod_check_keeps_profile() {
    let mut test = TestApp::new();
    test.app.has_run_init = true;
    test.app.state.config.drg_pak_path = None;
    let spec = ModSpecification::new("https://mod.io/g/drg/m/example#1".into());
    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .push(ModOrGroup::Individual(mod_config(&spec.url)));
    let before = (**test.app.state.mod_data).clone();
    test.full_button("Check mod updates", false);
    assert!(!button_rects(&test.full_frame(vec![]), "Current profile").is_empty());
    test.full_click(egui::pos2(15.0, 200.0), false);
    assert!(button_rects(&test.full_frame(vec![]), "Current profile").is_empty());
    message::UpdateCache::send_specs(&mut test.app, &test.context, vec![spec]);
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        test.full_frame(vec![]);
        if test.app.update_rid.is_none() {
            break;
        }
    }
    assert!(test.app.update_rid.is_none());
    let report = test
        .app
        .last_action
        .as_ref()
        .unwrap()
        .report
        .as_ref()
        .unwrap();
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].reason.contains("Settings"));
    assert_eq!(**test.app.state.mod_data, before);
}

#[tokio::test]
async fn first_run_explains_empty_profile_and_missing_game_installation() {
    let mut test = TestApp::new();
    test.full_frame(vec![]);
    let output = test.full_frame(vec![]);
    text_rect(
        &output,
        "This profile has no mods yet. Paste a mod.io link, URL or file path into Add mod, or drop .pak files onto this window.",
    );
    text_rect(
        &output,
        "Select your DRG installation in Settings to install mods.",
    );

    test.app
        .state
        .mod_data
        .get_active_profile_mut()
        .mods
        .push(ModOrGroup::Individual(mod_config(
            "https://example.com/a.pak",
        )));
    let output = test.full_frame(vec![]);
    assert!(!output.shapes.iter().any(|shape| matches!(
        &shape.shape,
        egui::Shape::Text(text) if text.galley.text().starts_with("This profile has no mods")
    )));
}

#[test]
fn installation_status_is_checked_in_the_background_and_follows_profile_changes() {
    let mut test = TestApp::new();
    let game = test.directory.path().join("game/FSD/Content/Paks");
    std::fs::create_dir_all(&game).unwrap();
    let game = game.join("FSD-WindowsNoEditor.pak");
    std::fs::write(&game, b"game path fixture").unwrap();
    let alpha = test.local_mod("alpha.pak");
    test.app.state.mod_data.get_active_profile_mut().mods = vec![ModOrGroup::Individual(alpha)];
    test.app.state.config.drg_pak_path = Some(game);
    let wait_for = |test: &mut TestApp, summary: &str| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            test.app
                .installation_status
                .poll(&test.app.state, &test.context, false);
            if test.app.installation_status.summary == summary {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{}",
                test.app.installation_status.summary
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    wait_for(
        &mut test,
        "No mods installed. Click Install mods to apply this profile.",
    );
    assert!(test.app.installation_status.needs_install);
    assert_eq!(test.app.installation_status.rows.len(), 1);

    // Disabling the only mod is a new request, answered without waiting for a periodic check.
    let ModOrGroup::Individual(alpha) =
        &mut test.app.state.mod_data.get_active_profile_mut().mods[0]
    else {
        unreachable!()
    };
    alpha.enabled = false;
    test.app
        .installation_status
        .poll(&test.app.state, &test.context, false);
    assert_eq!(
        test.app.installation_status.summary,
        "Checking installed mod versions..."
    );
    assert!(test.app.installation_status.rows.is_empty());
    wait_for(
        &mut test,
        "No mods installed. Click Install mods to apply this profile.",
    );
    assert!(!test.app.installation_status.needs_install);

    test.app.state.config.drg_pak_path = None;
    test.app
        .installation_status
        .poll(&test.app.state, &test.context, false);
    assert!(test.app.installation_status.summary.is_empty());
    assert!(!test.app.installation_status.needs_install);
}

#[test]
fn footer_time_repaints_only_when_its_text_changes() {
    use std::time::Duration;
    let after = |ms: u64| LastAction::timeago_changes_after(Duration::from_millis(ms));
    let text = |ms: u64| LastAction::timeago_after(Duration::from_millis(ms));
    for ms in [
        0, 400, 59_999, 60_000, 61_500, 3_599_999, 3_600_000, 90_000_000,
    ] {
        match after(ms) {
            Some(next) => {
                let next = next.as_millis() as u64;
                assert!(next > 0, "{ms}");
                assert_eq!(text(ms), text(ms + next - 1), "{ms}");
                assert_ne!(text(ms), text(ms + next), "{ms}");
            }
            None => assert_eq!(text(ms), text(ms + 86_400_000), "{ms}"),
        }
    }
    assert_eq!(after(61_500), Some(Duration::from_millis(58_500)));
    assert_eq!(after(3_600_000), None);
}
