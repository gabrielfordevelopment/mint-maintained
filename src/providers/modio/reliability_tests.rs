use super::*;
use crate::state::config::ConfigWrapper;
use std::io::{Read, Write};

#[tokio::test]
async fn cancellation_keeps_completed_refreshes_and_only_resumes_remaining_mods() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path(), &Default::default()).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel = cancelled.clone();
    let progress = Arc::new(AtomicUsize::new(0));
    let mut mock = MockDrgModio::new();
    mock.expect_fetch_mod().times(1).returning(move |_, id| {
        assert_eq!(id, 1);
        cancel.store(true, Ordering::Release);
        Ok(ModioMod {
            name_id: "first".into(),
            name: "Completed mod".into(),
            latest_modfile: Some(10),
            modfiles: vec![],
            tags: HashSet::new(),
            details: None,
        })
    });
    mock.expect_fetch_dependencies()
        .times(1)
        .returning(|_, _| Ok(vec![]));
    store
        .providers
        .write()
        .unwrap()
        .insert(MODIO_PROVIDER_ID, Arc::new(ModioProvider::new(mock)));
    let remaining = format_spec("second", 2, None);
    let report = store
        .refresh_mods(
            vec![format_spec("first", 1, None), remaining.clone()],
            cancelled,
            progress.clone(),
        )
        .await;
    assert!(report.cancelled);
    assert_eq!(report.checked, 1);
    assert_eq!(progress.load(Ordering::Acquire), 1);
    assert_eq!(report.retry_specs(), [remaining]);
    assert!(report.save_error.is_none());
    let saved =
        crate::providers::cache::read_cache_metadata_or_default(&dir.path().join("cache.json"))
            .unwrap();
    assert_eq!(
        saved.get::<ModioCache>(MODIO_PROVIDER_ID).unwrap().mods[&1].name,
        "Completed mod"
    );
}

#[tokio::test]
async fn targeted_refresh_preserves_pins_and_offline_data_and_retries_only_failures() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path(), &Default::default()).unwrap();
    let cached_mod = |id: u32| ModioMod {
        name_id: format!("mod-{id}"),
        name: format!("Mod {id}"),
        latest_modfile: Some(id * 10),
        modfiles: vec![ModioFile {
            id: id * 10,
            date_added: 1,
            version: None,
            changelog: None,
        }],
        tags: HashSet::new(),
        details: None,
    };
    {
        let mut lock = store.cache.write().unwrap();
        let c = lock.get_mut::<ModioCache>(MODIO_PROVIDER_ID);
        for id in [1, 2] {
            c.mods.insert(id, cached_mod(id));
            c.dependencies.insert(id, vec![]);
        }
    }
    let mut mock = MockDrgModio::new();
    mock.expect_fetch_mod()
        .withf(|_, id| *id == 1)
        .times(1)
        .returning(move |_, _| {
            let mut updated = cached_mod(1);
            updated.latest_modfile = Some(11);
            updated.modfiles.insert(
                0,
                ModioFile {
                    id: 11,
                    date_added: 2,
                    version: Some("New".into()),
                    changelog: None,
                },
            );
            Ok(updated)
        });
    let mut attempts = 0;
    mock.expect_fetch_mod()
        .withf(|_, id| *id == 2)
        .times(2)
        .returning(move |_, _| {
            attempts += 1;
            if attempts == 1 {
                Err(DrgModioError::GenericError {
                    msg: "Temporary fixture failure",
                })
            } else {
                Ok(cached_mod(2))
            }
        });
    mock.expect_fetch_dependencies()
        .times(2)
        .returning(|_, _| Ok(vec![]));
    store
        .providers
        .write()
        .unwrap()
        .insert(MODIO_PROVIDER_ID, Arc::new(ModioProvider::new(mock)));
    let pinned = format_spec("mod-1", 1, Some(10));
    let selected = pinned.clone();
    let report = store
        .refresh_mods(
            vec![
                pinned.clone(),
                format_spec("mod-1", 1, None),
                format_spec("mod-2", 2, None),
            ],
            Default::default(),
            Default::default(),
        )
        .await;
    assert_eq!(report.total, 2);
    assert_eq!(report.checked, 2);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.new_versions, ["Mod 1"]);
    assert_eq!(pinned, selected);
    assert!(
        store
            .get_mod_info(&pinned)
            .unwrap()
            .resolution
            .url
            .0
            .ends_with("/10")
    );
    assert!(
        store
            .get_mod_info(&format_spec("mod-1", 1, None))
            .unwrap()
            .resolution
            .url
            .0
            .ends_with("/11")
    );
    assert_eq!(
        store
            .get_mod_info(&format_spec("mod-2", 2, None))
            .unwrap()
            .name,
        "Mod 2"
    );
    let retried = store
        .refresh_mods(report.retry_specs(), Default::default(), Default::default())
        .await;
    assert_eq!(retried.checked, 1);
    assert!(retried.failures.is_empty());
}

#[tokio::test]
async fn targeted_refresh_does_not_commit_half_refreshed_metadata() {
    let cache = Arc::new(RwLock::new(ConfigWrapper::memory(
        VersionAnnotatedCache::default(),
    )));
    let old = ModioMod {
        name_id: "example".into(),
        name: "Offline copy".into(),
        latest_modfile: Some(10),
        modfiles: vec![],
        tags: HashSet::new(),
        details: None,
    };
    cache
        .write()
        .unwrap()
        .get_mut::<ModioCache>(MODIO_PROVIDER_ID)
        .mods
        .insert(1, old.clone());
    let mut mock = MockDrgModio::new();
    let mut new = old.clone();
    new.name = "Remote change".into();
    mock.expect_fetch_mod()
        .times(1)
        .return_once(move |_, _| Ok(new));
    mock.expect_fetch_dependencies().times(1).returning(|_, _| {
        Err(DrgModioError::GenericError {
            msg: "No connection",
        })
    });
    assert!(
        ModioProvider::new(mock)
            .refresh_mod(&format_spec("example", 1, None), cache.clone())
            .await
            .is_err()
    );
    assert_eq!(
        cache
            .read()
            .unwrap()
            .get::<ModioCache>(MODIO_PROVIDER_ID)
            .unwrap()
            .mods[&1],
        old
    );
}

async fn response_sequence(headers: Vec<String>) -> (u16, usize) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = done.clone();
    let server = std::thread::spawn(move || {
        let mut count = 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while !stop.load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 4096];
                    let mut received = Vec::new();
                    while !received.windows(4).any(|part| part == b"\r\n\r\n") {
                        let count = socket.read(&mut request).unwrap();
                        assert!(count > 0 && received.len() < 8192);
                        received.extend_from_slice(&request[..count]);
                    }
                    let response = headers
                        .get(count)
                        .unwrap_or_else(|| headers.last().unwrap());
                    write!(
                        socket,
                        "HTTP/1.1 {response}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                    count += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(2))
                }
                Err(error) => panic!("{error}"),
            }
        }
        count
    });
    let client = reqwest_middleware::ClientBuilder::new(
        reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap(),
    )
    .with(LoggingMiddleware {
        requests: Default::default(),
        gate: Arc::new(rate_limit::RequestGate::default()),
    })
    .build();
    let response = client.get(format!("http://{address}/fixture")).send().await;
    done.store(true, std::sync::atomic::Ordering::Release);
    let requests = server.join().unwrap();
    (response.unwrap().status().as_u16(), requests)
}

#[tokio::test]
async fn malformed_retry_delays_use_a_safe_fallback_without_panicking() {
    for value in [
        "-1",
        "0",
        "invalid",
        "9999999999999999999999999",
        "Wed, 21 Oct 2037 07:28:00 GMT",
    ] {
        assert_eq!(
            rate_limit::retry_delay(Some(&value.parse().unwrap())),
            std::time::Duration::from_secs(60)
        );
    }
    assert_eq!(
        response_sequence(vec!["200 OK\r\nRetry-After: 1".into()]).await,
        (200, 1)
    );
}

#[tokio::test]
async fn transient_retries_succeed_or_stop_after_three_attempts() {
    assert_eq!(
        response_sequence(vec![
            "429 Too Many Requests\r\nRetry-After: 1".into(),
            "200 OK".into()
        ])
        .await,
        (200, 2)
    );
    assert_eq!(
        response_sequence(vec!["503 Service Unavailable\r\nRetry-After: 1".into()]).await,
        (503, 3)
    );
}

#[tokio::test]
async fn cache_refresh_keeps_healthy_updates_and_retries_unavailable_entries() {
    let cache = Arc::new(RwLock::new(ConfigWrapper::memory(
        VersionAnnotatedCache::default(),
    )));
    let old_time = UNIX_EPOCH + std::time::Duration::from_secs(100);
    let mod_data = |id: u32, name: &str| ModioMod {
        name_id: format!("mod-{id}"),
        name: name.into(),
        latest_modfile: Some(id * 10),
        modfiles: vec![],
        tags: HashSet::new(),
        details: None,
    };
    {
        let mut guard = cache.write().unwrap();
        let cached = guard.get_mut::<ModioCache>(MODIO_PROVIDER_ID);
        cached.last_update_time = Some(old_time);
        cached.mods.insert(1, mod_data(1, "Offline copy"));
        cached.mods.insert(2, mod_data(2, "Old metadata"));
    }
    let unavailable = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let failure = unavailable.clone();
    let mut mock = MockDrgModio::new();
    mock.expect_fetch_mod_updates_since()
        .times(3)
        .returning(|mut ids, time| {
            ids.sort();
            assert_eq!(ids, [1, 2]);
            assert_eq!(time, 100);
            Ok(HashSet::from([1, 2]))
        });
    mock.expect_fetch_mod().returning(move |_, id| {
        if id == 1 && failure.load(std::sync::atomic::Ordering::Acquire) {
            Err(DrgModioError::GenericError {
                msg: "fixture unavailable mod",
            })
        } else {
            Ok(ModioMod {
                name_id: format!("mod-{id}"),
                name: "Refreshed".into(),
                latest_modfile: Some(id * 10),
                modfiles: vec![],
                tags: HashSet::new(),
                details: None,
            })
        }
    });
    mock.expect_fetch_dependencies()
        .returning(|_, _| Ok(vec![]));
    let provider = ModioProvider::new(mock);
    for _ in 0..2 {
        assert!(matches!(
            provider.update_cache(cache.clone()).await,
            Err(ProviderError::PartialUpdate { .. })
        ));
        let guard = cache.read().unwrap();
        let cached = guard.get::<ModioCache>(MODIO_PROVIDER_ID).unwrap();
        assert_eq!(cached.last_update_time, Some(old_time));
        assert_eq!(cached.mods[&1].name, "Offline copy");
        assert_eq!(cached.mods[&2].name, "Refreshed");
    }
    unavailable.store(false, std::sync::atomic::Ordering::Release);
    provider.update_cache(cache.clone()).await.unwrap();
    assert_ne!(
        cache
            .read()
            .unwrap()
            .get::<ModioCache>(MODIO_PROVIDER_ID)
            .unwrap()
            .last_update_time,
        Some(old_time)
    );
}
