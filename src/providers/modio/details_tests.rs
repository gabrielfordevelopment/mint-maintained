use super::*;

fn cached_mod(id: u32, details: Option<ModioDetails>) -> ModioMod {
    ModioMod {
        name_id: format!("mod-{id}"),
        name: format!("Mod {id}"),
        latest_modfile: Some(id * 10 + 1),
        modfiles: vec![
            ModioFile {
                id: id * 10,
                date_added: 1,
                version: Some("1.0".into()),
                changelog: Some("  ".into()),
            },
            ModioFile {
                id: id * 10 + 1,
                date_added: 2,
                version: None,
                changelog: Some("Fixed things".into()),
            },
        ],
        tags: HashSet::new(),
        details,
    }
}

fn details(id: u32) -> ModioDetails {
    ModioDetails {
        logo: Some(format!("https://thumb.modcdn.io/{id}.png")),
        summary: format!("Summary {id}"),
        author: format!("Author {id}"),
    }
}

fn store_with(dir: &std::path::Path, mods: impl IntoIterator<Item = (u32, ModioMod)>) -> ModStore {
    let store = ModStore::new(dir, &Default::default()).unwrap();
    {
        let mut lock = store.cache.write().unwrap();
        let cache = lock.get_mut::<ModioCache>(MODIO_PROVIDER_ID);
        for (id, mod_) in mods {
            cache.mod_id_map.insert(mod_.name_id.clone(), id);
            cache.dependencies.insert(id, vec![]);
            cache.mods.insert(id, mod_);
        }
    }
    store
}

fn install(store: &ModStore, mock: MockDrgModio) {
    store
        .providers
        .write()
        .unwrap()
        .insert(MODIO_PROVIDER_ID, Arc::new(ModioProvider::new(mock)));
}

fn saved_details(dir: &std::path::Path, id: u32) -> Option<ModioDetails> {
    crate::providers::cache::read_cache_metadata_or_default(&dir.join("cache.json"))
        .unwrap()
        .get::<ModioCache>(MODIO_PROVIDER_ID)
        .unwrap()
        .mods[&id]
        .details
        .clone()
}

#[test]
fn cached_mods_without_details_load_and_keep_their_old_shape() {
    let old = serde_json::json!({
        "name_id": "old", "name": "Old", "latest_modfile": 1,
        "modfiles": [], "tags": []
    });
    let parsed: ModioMod = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(parsed.details, None);
    assert_eq!(serde_json::to_value(&parsed).unwrap(), old);

    let mut with_details = parsed;
    with_details.details = Some(details(1));
    let value = serde_json::to_value(&with_details).unwrap();
    assert_eq!(value["details"]["logo"], "https://thumb.modcdn.io/1.png");
    assert_eq!(
        serde_json::from_value::<ModioMod>(value).unwrap(),
        with_details
    );
    let partial: ModioDetails = serde_json::from_str("{}").unwrap();
    assert_eq!(partial, ModioDetails::default());
}

#[test]
fn details_list_versions_newest_first_and_hide_blank_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut blank = details(1);
    blank.summary = " ".into();
    let store = store_with(
        dir.path(),
        [(1, cached_mod(1, Some(blank))), (2, cached_mod(2, None))],
    );
    install(&store, MockDrgModio::new());
    let info = store
        .get_mod_details(&format_spec("mod-1", 1, Some(10)))
        .unwrap();
    assert_eq!(
        info.thumbnail_url.as_deref(),
        Some("https://thumb.modcdn.io/1.png")
    );
    assert_eq!(info.summary, None);
    assert_eq!(info.author.as_deref(), Some("Author 1"));
    assert_eq!(info.updated, Some(2), "newest file upload time");
    let versions = store.get_mod_versions(&format_spec("mod-1", 1, Some(10)));
    let names: Vec<_> = versions.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["11", "10 - 1.0"]);
    assert!(versions[0].latest && !versions[1].latest);
    assert_eq!(versions[0].changelog.as_deref(), Some("Fixed things"));
    assert_eq!(versions[1].changelog, None);

    let missing = store
        .get_mod_details(&ModSpecification::new(
            "https://mod.io/g/drg/m/mod-2".into(),
        ))
        .unwrap();
    assert_eq!(missing.thumbnail_url, None);
    assert_eq!(
        store
            .get_mod_versions(&ModSpecification::new(
                "https://mod.io/g/drg/m/mod-2".into()
            ))
            .len(),
        2
    );
    assert!(
        store
            .get_mod_details(&format_spec("unknown", 9, None))
            .is_none()
    );
    assert!(
        store
            .get_mod_versions(&format_spec("unknown", 9, None))
            .is_empty()
    );
}

#[tokio::test]
async fn backfill_fetches_only_missing_details_once_and_saves_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        [
            (1, cached_mod(1, None)),
            (2, cached_mod(2, Some(details(2)))),
        ],
    );
    let mut mock = MockDrgModio::new();
    mock.expect_fetch_details().times(1).returning(|ids| {
        assert_eq!(ids, [1]);
        Ok(vec![(1, details(1)), (7, details(7))])
    });
    install(&store, mock);
    assert!(store.backfill_details().await.unwrap());
    assert_eq!(saved_details(dir.path(), 1), Some(details(1)));
    assert_eq!(saved_details(dir.path(), 2), Some(details(2)));
    assert!(!store.backfill_details().await.unwrap());
}

#[tokio::test]
async fn failed_backfill_batch_keeps_earlier_batches_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(dir.path(), (1..=150).map(|id| (id, cached_mod(id, None))));
    let mut mock = MockDrgModio::new();
    let mut calls = 0;
    mock.expect_fetch_details().times(3).returning(move |ids| {
        calls += 1;
        match calls {
            1 => {
                assert_eq!(ids, (1..=100).collect::<Vec<_>>());
                Ok(ids.into_iter().map(|id| (id, details(id))).collect())
            }
            2 => Err(DrgModioError::GenericError {
                msg: "fixture outage",
            }),
            _ => {
                assert_eq!(ids, (101..=150).collect::<Vec<_>>());
                Ok(ids.into_iter().map(|id| (id, details(id))).collect())
            }
        }
    });
    install(&store, mock);
    assert!(store.backfill_details().await.is_err());
    assert_eq!(saved_details(dir.path(), 100), Some(details(100)));
    assert_eq!(saved_details(dir.path(), 101), None);
    assert!(store.backfill_details().await.unwrap());
    assert_eq!(saved_details(dir.path(), 150), Some(details(150)));
}
