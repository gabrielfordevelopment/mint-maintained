use super::*;

fn app() -> (tempfile::TempDir, App) {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::load(Dirs::from_path(directory.path()).unwrap(), None).unwrap();
    app.state.config.drg_pak_path = None;
    (directory, app)
}

fn pending<S>(app: &mut App, state: S) -> MessageHandle<S> {
    MessageHandle {
        rid: app.request_counter.next(),
        handle: tokio::spawn(std::future::pending()),
        state,
        cancellation: None,
    }
}

#[tokio::test]
async fn import_keeps_input_order_and_failed_entries_available_for_retry() {
    let (directory, mut app) = app();
    let specs = (0..12)
        .map(|index| {
            let path = directory.path().join(format!("mod-{index:02}.pak"));
            std::fs::write(&path, b"fixture").unwrap();
            ModSpecification::new(path.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    let mut inputs = specs.clone();
    inputs.insert(
        4,
        ModSpecification::new("invalid-provider://fixture".into()),
    );
    for _ in 0..3 {
        app.state.mod_data.get_active_profile_mut().mods.clear();
        let result = app.state.store.resolve_mods_partial(&inputs, false).await;
        app.resolve_mod_rid = Some(pending(&mut app, ()));
        let rid = app.resolve_mod_rid.as_ref().unwrap().rid;
        ResolveMods {
            rid,
            specs: inputs.clone(),
            result: Ok(result),
            is_dependency: false,
        }
        .receive(&mut app);
        assert!(app.resolve_mod_rid.is_none());
        assert_eq!(
            app.state.mod_data.enabled_mods_ordered("default").unwrap(),
            specs
        );
        assert_eq!(app.resolve_mod, "invalid-provider://fixture");
        assert!(matches!(
            app.last_action.as_ref().unwrap().status,
            super::super::LastActionStatus::Failure(_)
        ));
    }
    let reloaded = State::init(Dirs::from_path(directory.path()).unwrap()).unwrap();
    assert_eq!(
        reloaded.mod_data.enabled_mods_ordered("default").unwrap(),
        specs
    );
}

#[tokio::test]
async fn import_does_not_duplicate_a_primary_mod_resolved_again_as_a_dependency() {
    let (directory, mut app) = app();
    let path = directory.path().join("primary.pak");
    std::fs::write(&path, b"fixture").unwrap();
    let original = ModSpecification::new(path.to_string_lossy().into_owned());
    let (_, mut info) = app
        .state
        .store
        .resolve_mod(original.clone(), false)
        .await
        .unwrap();
    let canonical = ModSpecification::new("https://mod.io/g/drg/m/fixture#1".into());
    info.spec = canonical.clone();
    app.resolve_mod_rid = Some(pending(&mut app, ()));
    let rid = app.resolve_mod_rid.as_ref().unwrap().rid;
    ResolveMods {
        rid,
        specs: vec![original.clone()],
        result: Ok(crate::providers::ResolvedMods {
            mods: vec![(original, info.clone()), (canonical.clone(), info.clone())],
            errors: vec![],
        }),
        is_dependency: false,
    }
    .receive(&mut app);
    assert_eq!(
        app.state.mod_data.enabled_mods_ordered("default").unwrap(),
        vec![canonical.clone()]
    );
    let imported = app.history.undo_id();
    assert!(imported.is_some());
    assert_eq!(app.last_action.as_ref().unwrap().undo_id, imported);
    app.resolve_mod_rid = Some(pending(&mut app, ()));
    ResolveMods {
        rid: app.resolve_mod_rid.as_ref().unwrap().rid,
        specs: vec![canonical.clone()],
        result: Ok(crate::providers::ResolvedMods {
            mods: vec![(canonical.clone(), info)],
            errors: vec![],
        }),
        is_dependency: true,
    }
    .receive(&mut app);
    assert_eq!(app.history.undo_id(), imported);
    assert!(
        app.last_action.as_ref().unwrap().undo_id.is_none(),
        "An unchanged import must not undo a previous edit"
    );
    app.restore_history(false, None);
    assert!(
        app.state
            .mod_data
            .enabled_mods_ordered("default")
            .unwrap()
            .is_empty()
    );
    app.restore_history(true, None);
    assert_eq!(
        app.state.mod_data.enabled_mods_ordered("default").unwrap(),
        vec![canonical]
    );
}

#[tokio::test]
async fn lint_failure_clears_only_its_own_request_and_ignores_stale_results() {
    let (_directory, mut app) = app();
    app.lint_rid = Some(pending(&mut app, ()));
    app.integrate_rid = Some(pending(&mut app, HashMap::new()));
    let rid = app.lint_rid.as_ref().unwrap().rid;
    let stale = app.request_counter.next();
    LintMods {
        rid: stale,
        result: Err(IntegrationError::GenericError {
            msg: "stale".into(),
        }),
    }
    .receive(&mut app);
    assert!(app.lint_rid.is_some());
    LintMods {
        rid,
        result: Err(IntegrationError::GenericError {
            msg: "fixture lint error".into(),
        }),
    }
    .receive(&mut app);
    assert!(app.lint_rid.is_none());
    assert!(app.integrate_rid.is_some());
    assert!(app.lint_report.is_none());
    app.integrate_rid.take().unwrap().handle.abort();
}

#[tokio::test]
async fn worker_panic_reports_failure_and_releases_the_matching_operation() {
    let (_directory, mut app) = app();
    let rid = app.request_counter.next();
    let handle = spawn_reported(rid, egui::Context::default(), app.tx.clone(), async {
        panic!("fixture worker panic")
    });
    app.update_rid = Some(MessageHandle {
        rid,
        handle,
        state: (),
        cancellation: None,
    });
    let message = tokio::time::timeout(std::time::Duration::from_secs(2), app.rx.recv())
        .await
        .unwrap()
        .unwrap();
    message.handle(&mut app);
    assert!(app.update_rid.is_none());
    assert!(matches!(
        app.last_action.unwrap().status,
        super::super::LastActionStatus::Failure(_)
    ));
}

#[tokio::test]
async fn cancelled_integration_reports_completion_without_touching_outputs() {
    let (directory, mut app) = app();
    let cancelled = Arc::new(AtomicBool::new(true));
    let rid = app.request_counter.next();
    let result = integrate_async(
        app.state.store.clone(),
        egui::Context::default(),
        vec![],
        directory.path().join("missing.pak"),
        MetaConfig {},
        rid,
        app.tx.clone(),
        cancelled,
    )
    .await;
    assert!(matches!(result, Err(IntegrationError::Cancelled)));
    assert!(!directory.path().join("mods_P.pak").exists());
}
