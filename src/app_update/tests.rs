use super::*;
use std::io::{Read, Write};

fn server(response: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/update.zip", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).unwrap() > 0);
        let _ = socket.write_all(&response);
    });
    (url, worker)
}

#[tokio::test]
async fn download_preserves_all_bytes_and_reports_unknown_size() {
    let directory = tempfile::tempdir().unwrap();
    let payload = vec![137; 512 * 1024];
    let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    response.extend(&payload);
    let (url, worker) = server(response);
    let destination = directory.path().join("update.zip");
    let observed = std::sync::Mutex::new(Vec::new());
    download(&url, &destination, &|p| observed.lock().unwrap().push(p))
        .await
        .unwrap();
    worker.join().unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), payload);
    assert_eq!(
        observed.lock().unwrap().last(),
        Some(&Progress::Downloading {
            downloaded: payload.len() as u64,
            total: None
        })
    );
}

#[tokio::test]
async fn failed_and_truncated_downloads_do_not_reach_replacement() {
    for response in [
        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nshort".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\ninvalid".to_vec(),
    ] {
        let (url, worker) = server(response);
        let control = Arc::new(Control::default());
        assert!(install(url, control.clone(), |_| {}).await.is_err());
        assert!(control.can_cancel(), "Replacement must not have started");
        worker.join().unwrap();
    }
}

#[tokio::test]
async fn cancellation_interrupts_a_stalled_download() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/update.zip", listener.local_addr().unwrap());
    let control = Arc::new(Control::default());
    let task_control = control.clone();
    let worker = tokio::spawn(install(url, task_control, |_| {}));
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(control.cancel());
    assert!(
        tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .is_none()
    );
    assert!(!control.begin_install());
}

#[test]
fn cancel_and_install_are_mutually_exclusive() {
    let control = Control::default();
    assert!(control.cancel());
    assert!(!control.begin_install());
    let control = Control::default();
    assert!(control.begin_install());
    assert!(!control.cancel());
    assert!(!control.can_cancel());
}

#[test]
fn invalid_archives_never_replace_the_application() {
    use zip::write::SimpleFileOptions;
    for contents in [
        None,
        Some(b"not an executable".as_slice()),
        Some(b"".as_slice()),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("update.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        if let Some(contents) = contents {
            zip.start_file(
                if cfg!(windows) { "mint.exe" } else { "mint" },
                SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
        assert!(extract(&archive, directory.path()).is_err());
    }
}

#[test]
fn replacement_child() {
    let Ok(case) = std::env::var("MINT_REPLACEMENT_TEST") else {
        return;
    };
    let executable = std::env::current_exe().unwrap();
    let candidate = executable.with_file_name("candidate.exe");
    let before = std::fs::read(&executable).unwrap();
    #[cfg(windows)]
    let _lock = if case == "locked" {
        use std::os::windows::fs::OpenOptionsExt;
        Some(
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&executable)
                .unwrap(),
        )
    } else {
        None
    };
    if case == "missing" {
        assert!(replace_current_executable(&candidate).is_err());
        assert_eq!(std::fs::read(&executable).unwrap(), before);
    } else {
        let mut updated = before;
        updated.extend_from_slice(b"MINT replacement regression fixture");
        std::fs::write(&candidate, &updated).unwrap();
        if case == "locked" {
            assert!(replace_current_executable(&candidate).is_err());
            assert_eq!(
                std::fs::read(&executable).unwrap(),
                updated[..updated.len() - b"MINT replacement regression fixture".len()]
            );
        } else {
            replace_current_executable(&candidate).unwrap();
            assert_eq!(std::fs::read(&executable).unwrap(), updated);
        }
    }
}

#[test]
fn a_running_executable_can_update_and_failed_updates_remain_launchable() {
    for case in ["success", "missing"]
        .into_iter()
        .chain(cfg!(windows).then_some("locked"))
    {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("mint-update-test.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let result = std::process::Command::new(&executable)
            .args([
                "--exact",
                "app_update::tests::replacement_child",
                "--nocapture",
            ])
            .env("MINT_REPLACEMENT_TEST", case)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{case}: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let result = std::process::Command::new(&executable)
            .arg("--list")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "The application must still launch after {case}"
        );
    }
}
