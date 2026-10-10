use super::*;
use crate::state::{ModProfile_v0_1_0 as ModProfile, State};
use crate::{Dirs, providers::ModSpecification};

fn config(name: &str) -> ModConfig {
    ModConfig {
        spec: ModSpecification::new(name.into()),
        enabled: true,
        required: true,
        priority: 25,
    }
}

fn fixture() -> ModData {
    let mut data = ModData::default();
    data.profiles.insert("other".into(), ModProfile::default());
    data.profiles.get_mut("default").unwrap().mods = vec![
        ModOrGroup::Individual(config("first")),
        ModOrGroup::Individual(config("second")),
    ];
    data
}

fn target(group: Option<&str>, index: usize) -> ListTarget {
    ListTarget {
        profile: "default".into(),
        group: group.map(str::to_owned),
        index,
    }
}

#[test]
fn shared_groups_create_move_rename_detach_and_reload() {
    let mut data = fixture();
    data.apply_edit(Edit::CreateGroup {
        profile: "default".into(),
        name: " Shared ".into(),
    })
    .unwrap();
    data.apply_edit(Edit::AttachGroup {
        profile: "other".into(),
        name: "Shared".into(),
    })
    .unwrap();
    data.apply_edit(Edit::AttachGroup {
        profile: "other".into(),
        name: "Shared".into(),
    })
    .unwrap();
    assert_eq!(data.profiles["other"].mods.len(), 1);
    data.apply_edit(Edit::MoveMod {
        source: target(None, 0),
        destination: Some("Shared".into()),
    })
    .unwrap();
    assert_eq!(data.groups["Shared"].mods, vec![config("first")]);
    assert_eq!(data.group_users("Shared"), ["default", "other"]);
    data.apply_edit(Edit::RenameGroup {
        name: "Shared".into(),
        replacement: "Renamed".into(),
    })
    .unwrap();
    assert_eq!(data.group_users("Renamed"), ["default", "other"]);
    assert!(!data.groups.contains_key("Shared"));
    data.apply_edit(Edit::DeleteEntry(target(None, 1))).unwrap();
    assert_eq!(data.group_users("Renamed"), ["other"]);
    assert_eq!(data.groups["Renamed"].mods, vec![config("first")]);
    let directory = tempfile::tempdir().unwrap();
    let mut state = State::init(Dirs::from_path(directory.path()).unwrap()).unwrap();
    **state.mod_data = data.clone();
    state.mod_data.save().unwrap();
    let loaded = State::init(Dirs::from_path(directory.path()).unwrap()).unwrap();
    assert_eq!(**loaded.mod_data, data);
}

#[test]
fn ungroup_preserves_order_and_effective_enabled_state_without_changing_shared_data() {
    let mut data = fixture();
    data.groups.insert(
        "Shared".into(),
        ModGroup {
            mods: vec![config("a"), config("b")],
        },
    );
    data.profiles.get_mut("default").unwrap().mods.insert(
        1,
        ModOrGroup::Group {
            group_name: "Shared".into(),
            enabled: false,
        },
    );
    data.apply_edit(Edit::AttachGroup {
        profile: "other".into(),
        name: "Shared".into(),
    })
    .unwrap();
    data.apply_edit(Edit::Ungroup {
        profile: "default".into(),
        index: 1,
    })
    .unwrap();
    let mods: Vec<_> = data.profiles["default"]
        .mods
        .iter()
        .map(|entry| match entry {
            ModOrGroup::Individual(mc) => (mc.spec.url.as_str(), mc.enabled, mc.priority),
            _ => panic!(),
        })
        .collect();
    assert_eq!(
        mods,
        [
            ("first", true, 25),
            ("a", false, 25),
            ("b", false, 25),
            ("second", true, 25)
        ]
    );
    assert_eq!(data.groups["Shared"].mods, [config("a"), config("b")]);
    assert_eq!(data.group_users("Shared"), ["other"]);
}

#[test]
fn moves_and_global_deletion_preserve_valid_references_and_other_entries() {
    let mut data = fixture();
    for name in ["A", "B"] {
        data.apply_edit(Edit::CreateGroup {
            profile: "default".into(),
            name: name.into(),
        })
        .unwrap();
    }
    data.apply_edit(Edit::MoveMod {
        source: target(None, 0),
        destination: Some("A".into()),
    })
    .unwrap();
    data.apply_edit(Edit::MoveMod {
        source: target(Some("A"), 0),
        destination: Some("B".into()),
    })
    .unwrap();
    assert!(data.groups["A"].mods.is_empty());
    assert_eq!(data.groups["B"].mods, [config("first")]);
    data.apply_edit(Edit::MoveMod {
        source: target(Some("B"), 0),
        destination: None,
    })
    .unwrap();
    assert!(data.groups["B"].mods.is_empty());
    assert!(
        matches!(data.profiles["default"].mods.last(), Some(ModOrGroup::Individual(mc)) if mc.spec.url == "first")
    );
    data.apply_edit(Edit::AttachGroup {
        profile: "other".into(),
        name: "A".into(),
    })
    .unwrap();
    data.apply_edit(Edit::DeleteGroup("A".into())).unwrap();
    assert!(data.group_users("A").is_empty());
    assert!(!data.groups.contains_key("A"));
    data.validate().unwrap();
}

#[test]
fn invalid_and_stale_operations_are_atomic_and_the_last_profile_is_protected() {
    let mut data = fixture();
    for edit in [
        Edit::CreateGroup {
            profile: "default".into(),
            name: "  ".into(),
        },
        Edit::CreateGroup {
            profile: "missing".into(),
            name: "A".into(),
        },
        Edit::RenameGroup {
            name: "default".into(),
            replacement: " ".into(),
        },
        Edit::DeleteEntry(target(None, 99)),
        Edit::DeleteEntry(target(Some("default"), 0)),
        Edit::MoveMod {
            source: target(None, 0),
            destination: Some("missing".into()),
        },
    ] {
        let before = data.clone();
        assert!(data.apply_edit(edit).is_err());
        assert_eq!(data, before);
    }
    let pending = PreparedEdit::new(&data, Edit::DeleteEntry(target(None, 0)));
    data.profiles.get_mut("default").unwrap().mods.swap(0, 1);
    let before = data.clone();
    assert!(pending.apply(&mut data).is_err());
    assert_eq!(data, before);
    data.apply_edit(Edit::DeleteProfile("default".into()))
        .unwrap();
    assert_eq!(data.active_profile, "other");
    assert!(
        data.apply_edit(Edit::DeleteProfile("other".into()))
            .is_err()
    );
    data.validate().unwrap();
}
