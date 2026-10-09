use super::*;
use crate::state::config::ConfigWrapper;
use std::io::{Read, Write};

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
    .with(LoggingMiddleware::default())
    .build();
    let response = client.get(format!("http://{address}/fixture")).send().await;
    done.store(true, std::sync::atomic::Ordering::Release);
    let requests = server.join().unwrap();
    (response.unwrap().status().as_u16(), requests)
}

#[tokio::test]
async fn malformed_and_unsupported_retry_delays_return_without_panicking() {
    for value in [
        "-1",
        "0",
        "invalid",
        "9999999999999999999999999",
        "Wed, 21 Oct 2037 07:28:00 GMT",
    ] {
        assert_eq!(
            response_sequence(vec![format!(
                "429 Too Many Requests\r\nRetry-After: {value}"
            )])
            .await,
            (429, 1)
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
