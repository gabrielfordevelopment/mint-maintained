use super::*;

#[test]
fn notifications_default_on_for_old_configs_and_disabled_choice_survives_reload() {
    assert!(VersionAnnotatedConfig::default().show_notifications);
    for versioned in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let dirs = Dirs::from_path(directory.path()).unwrap();
        let path = dirs.config_dir.join("config.json");
        let mut value = serde_json::json!({
            "provider_parameters": {}, "drg_pak_path": null,
            "gui_theme": null, "sorting_config": null
        });
        if versioned {
            value["version"] = "0.0.0".into();
        }
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        {
            let mut state = State::init(dirs.clone()).unwrap();
            assert!(state.config.show_notifications);
            state.config.show_notifications = false;
            state.config.save().unwrap();
        }
        let state = State::init(dirs).unwrap();
        assert!(!state.config.show_notifications);
        assert!(state.config.provider_parameters.is_empty());
        assert!(state.config.drg_pak_path.is_none());
    }
}

#[test]
fn invalid_notification_setting_preserves_config_bytes() {
    for setting in [serde_json::json!(null), serde_json::json!("false")] {
        let directory = tempfile::tempdir().unwrap();
        let dirs = Dirs::from_path(directory.path()).unwrap();
        let path = dirs.config_dir.join("config.json");
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": "0.0.0", "provider_parameters": {}, "drg_pak_path": null,
            "gui_theme": null, "sorting_config": null, "show_notifications": setting
        }))
        .unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(State::init(dirs).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn group_colors_default_for_old_data_and_survive_reload_and_rename() {
    use crate::state::edits::Edit;
    let old: ModGroup = serde_json::from_str(r#"{"mods":[]}"#).unwrap();
    assert_eq!(old.color, GroupColor::Gray);
    assert_eq!(
        serde_json::to_value(&old).unwrap(),
        serde_json::json!({"mods": []})
    );
    let directory = tempfile::tempdir().unwrap();
    let dirs = Dirs::from_path(directory.path()).unwrap();
    let mut state = State::init(dirs.clone()).unwrap();
    state
        .mod_data
        .profiles
        .insert("second".into(), Default::default());
    state
        .mod_data
        .apply_edit(Edit::CreateGroup {
            profile: "default".into(),
            name: "Shared".into(),
        })
        .unwrap();
    state
        .mod_data
        .apply_edit(Edit::AttachGroup {
            profile: "second".into(),
            name: "Shared".into(),
        })
        .unwrap();
    state
        .mod_data
        .apply_edit(Edit::SetGroupColor {
            name: "Shared".into(),
            color: GroupColor::Blue,
        })
        .unwrap();
    state
        .mod_data
        .apply_edit(Edit::RenameGroup {
            name: "Shared".into(),
            replacement: "Renamed".into(),
        })
        .unwrap();
    state.mod_data.save().unwrap();
    let loaded = State::init(dirs).unwrap();
    assert_eq!(loaded.mod_data.groups["Renamed"].color, GroupColor::Blue);
    assert_eq!(
        loaded.mod_data.group_users("Renamed"),
        ["default", "second"]
    );
    let before = (**state.mod_data).clone();
    assert!(
        state
            .mod_data
            .apply_edit(Edit::SetGroupColor {
                name: "missing".into(),
                color: GroupColor::Red
            })
            .is_err()
    );
    assert_eq!(**state.mod_data, before);
}

#[test]
fn unsupported_group_color_is_reported_without_rewriting_saved_data() {
    let directory = tempfile::tempdir().unwrap();
    let dirs = Dirs::from_path(directory.path()).unwrap();
    let data = r#"{"version":"0.1.0","active_profile":"default","profiles":{"default":{"mods":[]}},"groups":{"future":{"mods":[],"color":"unknown_future_color"}}}"#;
    let path = dirs.config_dir.join("mod_data.json");
    fs::create_dir_all(&dirs.config_dir).unwrap();
    fs::write(&path, data).unwrap();
    assert!(State::init(dirs).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), data);
}

#[test]
fn unsupported_mod_data_version_does_not_fall_back_to_legacy_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let dirs = Dirs::from_path(directory.path()).unwrap();
    let data = r#"{"version":"99.0.0","active_profile":"default","profiles":{"default":{"mods":[]}},"groups":{}}"#;
    let path = dirs.config_dir.join("mod_data.json");
    fs::create_dir_all(&dirs.config_dir).unwrap();
    fs::write(&path, data).unwrap();
    assert!(State::init(dirs).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), data);
}

#[test]
fn invalid_profile_references_are_rejected_without_rewriting_data() {
    for data in [
        r#"{"version":"0.1.0","active_profile":"default","profiles":{"default":{"mods":[{"group_name":"missing","enabled":true}]}},"groups":{}}"#,
        r#"{"version":"0.1.0","active_profile":"missing","profiles":{"default":{"mods":[]}},"groups":{}}"#,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let dirs = Dirs::from_path(directory.path()).unwrap();
        let path = dirs.config_dir.join("mod_data.json");
        fs::write(&path, data).unwrap();
        let result = State::init(dirs);
        assert!(matches!(result, Err(StateError::InvalidModData { .. })));
        assert_eq!(fs::read_to_string(path).unwrap(), data);
    }
}

#[test]
fn load_order_respects_priorities_groups_and_stable_ties() {
    let config = |name: &str, priority, enabled| ModConfig {
        spec: ModSpecification::new(name.into()),
        priority,
        enabled,
        required: false,
    };
    let data = ModData_v0_1_0 {
        active_profile: "default".into(),
        profiles: [(
            "default".into(),
            ModProfile_v0_1_0 {
                mods: vec![
                    ModOrGroup::Individual(config("normal", 0, true)),
                    ModOrGroup::Group {
                        group_name: "group".into(),
                        enabled: true,
                    },
                    ModOrGroup::Individual(config("last", i32::MIN, true)),
                    ModOrGroup::Individual(config("disabled", i32::MAX, false)),
                ],
            },
        )]
        .into(),
        groups: [(
            "group".into(),
            ModGroup {
                mods: vec![config("high", 10, true), config("tie", 0, true)],
                ..Default::default()
            },
        )]
        .into(),
    };
    assert_eq!(
        data.enabled_mods_ordered("default")
            .unwrap()
            .iter()
            .map(|spec| spec.url.as_str())
            .collect::<Vec<_>>(),
        ["high", "normal", "tie", "last"]
    );
    assert!(data.enabled_mods_ordered("missing").is_err());
}

const LEGACY_PROFILES: &str = r#"{
    "active_profile":"custom",
    "profiles":{"custom":{"mods":[
        {"spec":{"url":"legacy.pak"},"required":true,"enabled":false,"priority":25}
    ]}}
}"#;

#[test]
fn migration_keeps_the_legacy_file_after_a_failed_save_and_can_retry() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("profiles.json");
    let current = directory.path().join("mod_data.json");
    fs::write(&legacy, LEGACY_PROFILES).unwrap();
    let data = read_mod_data_or_default(&current, legacy.clone()).unwrap();

    fs::create_dir(&current).unwrap();
    let sentinel = current.join("keep.txt");
    fs::write(&sentinel, "existing data").unwrap();
    let config = ConfigWrapper::new(&current, data);
    assert!(config.save().is_err());
    drop(config);
    assert_eq!(fs::read_to_string(&legacy).unwrap(), LEGACY_PROFILES);
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "existing data");

    fs::remove_file(sentinel).unwrap();
    fs::remove_dir(&current).unwrap();
    let retry = read_mod_data_or_default(&current, legacy.clone()).unwrap();
    let config = ConfigWrapper::new(&current, retry);
    config.save().unwrap();
    drop(config);
    let loaded = read_mod_data_or_default(&current, legacy.clone()).unwrap();
    assert_eq!(loaded.active_profile, "custom");
    let ModOrGroup::Individual(mod_) = &loaded.profiles["custom"].mods[0] else {
        panic!("Legacy individual mod must stay an individual mod");
    };
    assert_eq!(mod_.spec.url, "legacy.pak");
    assert!(mod_.required);
    assert!(!mod_.enabled);
    assert_eq!(mod_.priority, 25);
    assert_eq!(fs::read_to_string(legacy).unwrap(), LEGACY_PROFILES);
}

#[test]
fn initialized_profiles_take_precedence_over_the_retained_legacy_backup() {
    let directory = tempfile::tempdir().unwrap();
    let dirs = Dirs::from_path(directory.path()).unwrap();
    let legacy = dirs.config_dir.join("profiles.json");
    fs::write(&legacy, LEGACY_PROFILES).unwrap();
    fs::write(
        dirs.config_dir.join("config.json"),
        r#"{"version":"0.0.0","provider_parameters":{},"drg_pak_path":null,"gui_theme":null,"sorting_config":null}"#,
    ).unwrap();
    let mut state = State::init(dirs).unwrap();
    assert_eq!(state.mod_data.active_profile, "custom");
    state
        .mod_data
        .profiles
        .get_mut("custom")
        .unwrap()
        .mods
        .clear();
    state.mod_data.save().unwrap();
    drop(state);

    let state = State::init(Dirs::from_path(directory.path()).unwrap()).unwrap();
    assert!(state.mod_data.profiles["custom"].mods.is_empty());
    assert_eq!(fs::read_to_string(legacy).unwrap(), LEGACY_PROFILES);
}

#[test]
fn malformed_legacy_profiles_are_not_removed_or_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("profiles.json");
    let current = directory.path().join("mod_data.json");
    fs::write(&legacy, "{broken").unwrap();
    assert!(read_mod_data_or_default(&current, legacy.clone()).is_err());
    assert_eq!(fs::read_to_string(legacy).unwrap(), "{broken");
    assert!(!current.exists());
}

#[test]
fn mod_list_settings_default_for_old_configs_and_survive_reload() {
    let defaults = ModListSettings::default();
    assert_eq!(defaults.view, ModListView::Compact);
    assert_eq!(defaults.priority, PriorityDisplay::Always);
    assert!(!defaults.compact_thumbnails && defaults.normal_thumbnails);
    assert!(defaults.download_thumbnails && defaults.show_tags);
    assert_eq!(defaults.striping, RowStriping::Medium);
    for versioned in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let dirs = Dirs::from_path(directory.path()).unwrap();
        let path = dirs.config_dir.join("config.json");
        let mut value = serde_json::json!({
            "provider_parameters": {}, "drg_pak_path": null,
            "gui_theme": null, "sorting_config": null
        });
        if versioned {
            value["version"] = "0.0.0".into();
        }
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        {
            let mut state = State::init(dirs.clone()).unwrap();
            assert_eq!(state.config.mod_list, defaults);
            state.config.mod_list = ModListSettings {
                view: ModListView::Normal,
                compact_thumbnails: true,
                normal_thumbnails: false,
                download_thumbnails: false,
                priority: PriorityDisplay::Hidden,
                show_tags: false,
                striping: RowStriping::Strong,
                row_lines: true,
                details_width: Some(320.0),
            };
            state.config.save().unwrap();
        }
        let state = State::init(dirs).unwrap();
        assert_eq!(state.config.mod_list.view, ModListView::Normal);
        assert_eq!(state.config.mod_list.priority, PriorityDisplay::Hidden);
        assert!(state.config.mod_list.compact_thumbnails);
        assert!(!state.config.mod_list.normal_thumbnails);
        assert!(!state.config.mod_list.download_thumbnails);
        assert!(!state.config.mod_list.show_tags);
        assert_eq!(state.config.mod_list.striping, RowStriping::Strong);
        assert!(state.config.mod_list.row_lines);
        assert_eq!(state.config.mod_list.details_width, Some(320.0));
    }
}

#[test]
fn development_priority_value_reads_as_always() {
    let settings: ModListSettings = serde_json::from_str(r#"{"priority":"non_zero"}"#).unwrap();
    assert_eq!(settings.priority, PriorityDisplay::Always);
    assert_eq!(
        serde_json::to_value(&settings).unwrap()["priority"],
        "always"
    );
}

#[test]
fn partial_mod_list_settings_keep_remaining_defaults() {
    let settings: ModListSettings = serde_json::from_str(r#"{"view":"normal"}"#).unwrap();
    assert_eq!(
        settings,
        ModListSettings {
            view: ModListView::Normal,
            ..Default::default()
        }
    );
}

#[test]
fn invalid_mod_list_settings_preserve_config_bytes() {
    for setting in [
        serde_json::json!({"view": "grid"}),
        serde_json::json!({"priority": "sometimes"}),
        serde_json::json!({"show_tags": "yes"}),
        serde_json::json!({"striping": "loud"}),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let dirs = Dirs::from_path(directory.path()).unwrap();
        let path = dirs.config_dir.join("config.json");
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": "0.0.0", "provider_parameters": {}, "drg_pak_path": null,
            "gui_theme": null, "sorting_config": null, "mod_list": setting
        }))
        .unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(State::init(dirs).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn last_updated_sorting_survives_reload_with_a_stable_wire_name() {
    let directory = tempfile::tempdir().unwrap();
    let dirs = Dirs::from_path(directory.path()).unwrap();
    {
        let mut state = State::init(dirs.clone()).unwrap();
        state.config.sorting_config = Some(SortingConfig {
            sort_category: SortBy::LastUpdated,
            is_ascending: true,
        });
        state.config.save().unwrap();
    }
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(dirs.config_dir.join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["sorting_config"]["sort_category"], "LastUpdated");
    let state = State::init(dirs).unwrap();
    let sorting = state.config.sorting_config.as_ref().unwrap();
    assert_eq!(sorting.sort_category, SortBy::LastUpdated);
    assert!(sorting.is_ascending);
}
