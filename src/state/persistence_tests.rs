use super::*;

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
