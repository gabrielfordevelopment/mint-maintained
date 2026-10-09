use mint::{
    Dirs,
    providers::{ModInfo, ModResolution, ModSpecification, ModStore},
    state::State,
};
use std::io::{Read, Write};

#[test]
fn failed_integration_preserves_the_installed_bundle_and_hook() {
    let dir = tempfile::tempdir().unwrap();
    let paks = dir.path().join("FSD/Content/Paks");
    let binaries = dir.path().join("FSD/Binaries/Win64");
    std::fs::create_dir_all(&paks).unwrap();
    std::fs::create_dir_all(&binaries).unwrap();
    let game = paks.join("FSD-WindowsNoEditor.pak");
    let installed = paks.join("mods_P.pak");
    let hook = binaries.join("x3daudio1_7.dll");
    std::fs::write(&installed, b"working pak").unwrap();
    std::fs::write(&hook, b"working hook").unwrap();
    let mut registry = vec![0u8; 36];
    registry.extend_from_slice(&0x12345679u32.to_le_bytes());
    registry.extend_from_slice(&[0u8; 48]);
    registry.extend_from_slice(&0x87654321u32.to_le_bytes());
    registry.extend_from_slice(&[0u8; 20]);
    let mut writer = repak::PakBuilder::new().writer(
        std::fs::File::create(&game).unwrap(),
        repak::Version::V11,
        "../../../".into(),
        None,
    );
    writer
        .write_file("FSD/AssetRegistry.bin", false, registry)
        .unwrap();
    writer.write_file("FSD/FSD.uproject", false, b"{}").unwrap();
    writer.write_index().unwrap();
    let bad = dir.path().join("broken.pak");
    std::fs::write(&bad, b"bad").unwrap();
    let spec = ModSpecification::new(bad.to_string_lossy().into_owned());
    let info = ModInfo {
        provider: "file",
        name: "Broken fixture".into(),
        spec: spec.clone(),
        versions: vec![],
        resolution: ModResolution::resolvable(spec.url.into()),
        suggested_require: false,
        suggested_dependencies: vec![],
        modio_tags: None,
        modio_id: None,
    };
    for mods in [vec![(info, bad)], vec![]] {
        assert!(
            mint::integrate::integrate(&game, mint_lib::mod_info::MetaConfig {}, mods).is_err()
        );
        assert_eq!(std::fs::read(&installed).unwrap(), b"working pak");
        assert_eq!(std::fs::read(&hook).unwrap(), b"working hook");
    }
}

#[tokio::test]
async fn partial_import_keeps_good_entries_but_integration_resolution_stays_strict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("good.pak");
    std::fs::write(&path, b"fixture").unwrap();
    let store = ModStore::new(dir.path(), &Default::default()).unwrap();
    let good = ModSpecification::new(path.to_string_lossy().into_owned());
    let bad = ModSpecification::new("invalid-provider://missing".into());
    let specs = [good.clone(), bad.clone(), good.clone()];
    let partial = store.resolve_mods_partial(&specs, false).await;
    assert_eq!(partial.mods.len(), 1);
    assert_eq!(partial.mods[0].0, good);
    assert_eq!(partial.errors.len(), 1);
    assert_eq!(partial.errors[0].0, bad);
    assert!(store.resolve_mods(&specs, false).await.is_err());
}

#[tokio::test]
async fn ordered_resolution_preserves_identity_despite_reversed_download_completion() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let workers: Vec<_> = (0..2).map(|_| {
            let (mut socket, _) = listener.accept().unwrap();
            std::thread::spawn(move || {
                socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).unwrap();
                let slow = String::from_utf8_lossy(&buffer[..count]).contains("/slow");
                if slow { std::thread::sleep(std::time::Duration::from_millis(150)); }
                let body = if slow { "slow" } else { "fast" };
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 4\r\nConnection: close\r\n\r\n{body}").unwrap();
            })
        }).collect();
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let root = tempfile::tempdir().unwrap();
    let mut state = State::init(Dirs::from_path(root.path()).unwrap()).unwrap();
    state.config.drg_pak_path = None;
    let specs =
        ["slow", "fast"].map(|name| ModSpecification::new(format!("http://{address}/{name}")));
    let paths = mint::resolve_ordered(&state, &specs).await.unwrap();
    server.join().unwrap();
    assert_eq!(
        paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>(),
        ["slow", "fast"]
    );
}

#[cfg(windows)]
#[test]
fn readonly_cache_returns_error_instead_of_panicking() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    let original = br#"{"version":"0.0.0","cache":{}}"#;
    std::fs::write(&path, original).unwrap();
    let permissions = std::fs::metadata(&path).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&path, readonly).unwrap();
    let result = ModStore::new(directory.path(), &Default::default());
    std::fs::set_permissions(&path, permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}
