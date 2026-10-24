use super::*;

fn release() -> GitHubRelease {
    GitHubRelease {
        tag_name: "v99.0.0".into(),
        html_url: "https://example.com/release".into(),
        body: "## Release notes\n\n- A long explanation of an application improvement that must wrap within the dialog.\n".repeat(60),
    }
}

fn frame(test: &mut TestApp, size: egui::Vec2, events: Vec<egui::Event>) -> egui::FullOutput {
    test.context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| {
            test.app.show_settings(ctx);
            test.app.show_update_window(ctx);
        },
    )
}

fn click(test: &mut TestApp, size: egui::Vec2, pos: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            test,
            size,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[tokio::test]
async fn long_release_notes_keep_actions_visible_in_small_windows_and_both_themes() {
    for dark in [false, true] {
        for size in [
            egui::vec2(900.0, 500.0),
            egui::vec2(480.0, 320.0),
            egui::vec2(360.0, 260.0),
        ] {
            let mut test = TestApp::new();
            test.context.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            test.app.available_update = Some(release());
            test.app.show_app_update = true;
            for _ in 0..3 {
                frame(&mut test, size, vec![]);
            }
            let output = frame(&mut test, size, vec![]);
            for label in ["Install update", "Remind me later", "Close update"] {
                let rect = button_rects(&output, label)[0];
                assert!(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect),
                    "{label}: {rect:?} in {size:?}"
                );
            }
            let pos = button_rects(&output, "Remind me later")[0].center();
            click(&mut test, size, pos);
            assert!(!test.app.show_app_update);
            assert!(test.app.update_dismissed);
            assert!(test.app.available_update.is_some());
        }
    }
}

#[tokio::test]
async fn update_closes_immediately_with_escape_and_reopens_from_settings() {
    let mut test = TestApp::new();
    let size = egui::vec2(900.0, 600.0);
    test.app.available_update = Some(release());
    test.app.show_app_update = true;
    for _ in 0..3 {
        frame(&mut test, size, vec![]);
    }
    frame(
        &mut test,
        size,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!test.app.show_app_update);
    test.app.settings_window = Some(WindowSettings::new(&test.app.state));
    for _ in 0..3 {
        frame(&mut test, size, vec![]);
    }
    let output = frame(&mut test, size, vec![]);
    let pos = button_rects(&output, "View update")[0].center();
    click(&mut test, size, pos);
    assert!(test.app.show_app_update);
    for _ in 0..3 {
        frame(&mut test, size, vec![]);
    }
    let output = frame(&mut test, size, vec![]);
    click(
        &mut test,
        size,
        button_rects(&output, "Close update")[0].center(),
    );
    assert!(!test.app.show_app_update);
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

#[tokio::test]
async fn running_update_ignores_escape_and_outside_clicks_and_close_only_cancels() {
    let mut test = TestApp::new();
    let size = egui::vec2(900.0, 600.0);
    let control = std::sync::Arc::new(crate::app_update::Control::default());
    test.app.available_update = Some(release());
    test.app.show_app_update = true;
    test.app.self_update_control = Some(control.clone());
    test.app.self_update_rid = Some(MessageHandle {
        rid: test.app.request_counter.next(),
        handle: tokio::spawn(async {}),
        state: crate::app_update::Progress::Downloading {
            downloaded: 10,
            total: Some(100),
        },
        cancellation: None,
    });
    for _ in 0..3 {
        frame(&mut test, size, vec![]);
    }

    frame(&mut test, size, vec![key(egui::Key::Escape)]);
    click(&mut test, size, egui::pos2(5.0, 5.0));
    assert!(test.app.show_app_update);
    assert!(control.can_cancel(), "the download must keep running");

    let output = frame(&mut test, size, vec![]);
    assert!(button_rects(&output, "Close update").is_empty());
    click(
        &mut test,
        size,
        button_rects(&output, "Cancel update")[0].center(),
    );
    assert!(
        test.app.show_app_update,
        "the cancellation result stays visible"
    );
    assert!(!control.can_cancel());
}

#[tokio::test]
async fn update_heading_shows_current_version_and_failure_offers_retry() {
    let mut test = TestApp::new();
    let size = egui::vec2(900.0, 600.0);
    test.app.available_update = Some(release());
    test.app.show_app_update = true;
    for _ in 0..3 {
        frame(&mut test, size, vec![]);
    }
    let output = frame(&mut test, size, vec![]);
    text_rect(
        &output,
        &format!(
            "Update available: v99.0.0 (current: v{})",
            mint_lib::built_info::version()
        ),
    );
    assert!(!button_rects(&output, "Install update").is_empty());
    assert!(button_rects(&output, "Retry").is_empty());

    test.app.app_update_status = "Update failed: the download was interrupted".into();
    frame(&mut test, size, vec![]);
    let output = frame(&mut test, size, vec![]);
    assert!(button_rects(&output, "Install update").is_empty());
    assert!(!button_rects(&output, "Retry").is_empty());
    assert!(!button_rects(&output, "Details").is_empty());
}

// Opt-in native fixture: the ordinary test suite never opens a window or installs a release.
#[test]
fn native_update_preview() {
    let Some(directory) = std::env::var_os("MINT_NATIVE_UPDATE_PREVIEW") else {
        return;
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let directory = PathBuf::from(directory);
    assert_eq!(
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap(),
        directory.canonicalize().unwrap(),
        "Copy the test executable beside its disposable app data before opening the update fixture"
    );
    let dirs = Dirs::from_path(&directory).unwrap();
    mint_lib::setup_logging(dirs.data_dir.join("mint.log"), "mint").unwrap();
    eframe::run_native(
        "MINT update test",
        eframe::NativeOptions {
            #[cfg(windows)]
            event_loop_builder: Some(Box::new(|builder| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            })),
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([900.0, 550.0])
                .with_icon(icons::app_icon()),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = App::new(cc, dirs, None).unwrap();
            app.state.config.drg_pak_path = None;
            app.has_run_init = true;
            cc.egui_ctx.set_theme(egui::ThemePreference::Light);
            let mut fixture = release();
            fixture.tag_name = concat!("v", env!("CARGO_PKG_VERSION")).into();
            fixture.html_url = mint_lib::update::GITHUB_REPOSITORY_URL.into();
            if std::env::var_os("MINT_NATIVE_UPDATE_URL").is_some() {
                fixture.body = format!("This is an isolated update demo. Install update uses the local test archive and changes only this preview.\n\n{}", include_str!("../../../CHANGELOG.md"));
            }
            app.available_update = Some(fixture);
            app.show_app_update = true;
            app.restart_arguments = vec!["--appdata".into(), directory.into_os_string()];
            Ok(Box::new(app))
        }),
    )
    .unwrap();
}
