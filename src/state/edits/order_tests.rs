use super::tests::{config, fixture, target};
use super::*;
use crate::{Dirs, state::State};

fn grouped() -> ModData {
    let mut data = fixture();
    for name in ["A", "B"] {
        data.apply_edit(Edit::CreateGroup {
            profile: "default".into(),
            name: name.into(),
        })
        .unwrap();
        data.groups.get_mut(name).unwrap().mods = vec![config(name)];
    }
    data
}

#[test]
fn every_root_insertion_preserves_blocks_and_member_state() {
    let original = grouped();
    for source in 0..4 {
        for destination in 0..=4 {
            let mut data = original.clone();
            data.apply_edit(Edit::MoveEntry {
                source: target(None, source),
                destination: target(None, destination),
            })
            .unwrap();
            let entries = &data.profiles["default"].mods;
            assert!(!data.profiles["default"].has_mixed_order());
            assert_eq!(entries.len(), 4);
            for entry in &original.profiles["default"].mods {
                assert_eq!(entries.iter().filter(|e| *e == entry).count(), 1);
            }
            assert_eq!(data.groups, original.groups);
            assert_eq!(data.profiles["other"], original.profiles["other"]);
        }
    }
    for destination in 0..=4 {
        let mut data = original.clone();
        data.apply_edit(Edit::MoveEntry {
            source: target(Some("B"), 0),
            destination: target(None, destination),
        })
        .unwrap();
        assert!(!data.profiles["default"].has_mixed_order());
        assert!(data.groups["B"].mods.is_empty());
        assert_eq!(
            data.profiles["default"].mods[destination.min(2)],
            ModOrGroup::Individual(config("B"))
        );
    }
}

#[test]
fn mixed_profiles_round_trip_unchanged_until_explicit_stable_arrangement() {
    let mut data = grouped();
    data.profiles.get_mut("default").unwrap().mods.swap(1, 2);
    data.profiles
        .insert("other".into(), data.profiles["default"].clone());
    let original = data.clone();
    let directory = tempfile::tempdir().unwrap();
    let dirs = Dirs::from_path(directory.path()).unwrap();
    let mut state = State::init(dirs.clone()).unwrap();
    **state.mod_data = data.clone();
    state.mod_data.save().unwrap();
    drop(state);
    let mut loaded = State::init(dirs.clone()).unwrap();
    assert_eq!(**loaded.mod_data, original);
    for edit in [
        Edit::CreateGroup {
            profile: "default".into(),
            name: "New".into(),
        },
        Edit::AttachGroup {
            profile: "default".into(),
            name: "default".into(),
        },
        Edit::Ungroup {
            profile: "default".into(),
            index: 1,
        },
        Edit::MoveMod {
            source: target(None, 0),
            destination: Some("A".into()),
        },
        Edit::MoveEntry {
            source: target(None, 0),
            destination: target(None, 4),
        },
    ] {
        assert!(data.apply_edit(edit).is_err());
        assert_eq!(data, original);
    }
    let prepared = PreparedEdit::new(&loaded.mod_data, Edit::ArrangeProfile("default".into()));
    assert!(prepared.apply(&mut loaded.mod_data).unwrap());
    assert_eq!(
        loaded.mod_data.profiles["default"],
        grouped().profiles["default"]
    );
    assert_eq!(
        loaded.mod_data.profiles["other"],
        original.profiles["other"]
    );
    assert_eq!(loaded.mod_data.groups, original.groups);
    assert_eq!(
        loaded
            .mod_data
            .enabled_mods_ordered("default")
            .unwrap()
            .iter()
            .map(|s| s.url.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "A", "B"]
    );
    loaded.mod_data.save().unwrap();
    let reopened = State::init(dirs).unwrap();
    assert_eq!(**reopened.mod_data, **loaded.mod_data);
}

#[test]
fn ungrouping_later_group_places_members_before_all_remaining_groups() {
    let mut data = grouped();
    data.apply_edit(Edit::AttachGroup {
        profile: "other".into(),
        name: "B".into(),
    })
    .unwrap();
    if let ModOrGroup::Group { enabled, .. } =
        &mut data.profiles.get_mut("default").unwrap().mods[3]
    {
        *enabled = false;
    }
    let other = data.profiles["other"].clone();
    data.apply_edit(Edit::Ungroup {
        profile: "default".into(),
        index: 3,
    })
    .unwrap();
    assert!(!data.profiles["default"].has_mixed_order());
    assert!(
        matches!(&data.profiles["default"].mods[2], ModOrGroup::Individual(mc) if mc.spec.url == "B" && !mc.enabled)
    );
    assert!(
        matches!(&data.profiles["default"].mods[3], ModOrGroup::Group { group_name, .. } if group_name == "A")
    );
    assert_eq!(data.groups["B"].mods, [config("B")]);
    assert_eq!(data.profiles["other"], other);
}
