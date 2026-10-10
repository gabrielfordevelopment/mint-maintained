#![cfg(windows)]

use std::process::Command;

#[test]
fn windows_gui_binary_keeps_cli_help_version_errors_and_redirected_output() {
    let executable = env!("CARGO_BIN_EXE_mint");
    let bytes = std::fs::read(executable).unwrap();
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    let subsystem = pe + 4 + 20 + 68;
    assert_eq!(
        u16::from_le_bytes(bytes[subsystem..subsystem + 2].try_into().unwrap()),
        2,
        "Explorer launch must not allocate a console"
    );
    let help = Command::new(executable).arg("--help").output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for command in ["integrate", "profile", "launch", "lint", "--appdata"] {
        assert!(help.contains(command));
    }
    let version = Command::new(executable).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION"))
    );
    let invalid = Command::new(executable)
        .arg("--invalid-fixture-argument")
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("--invalid-fixture-argument")
    );

    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config");
    std::fs::create_dir(&config).unwrap();
    std::fs::write(config.join("config.json"), br#"{"version":"0.0.0","provider_parameters":{},"drg_pak_path":null,"gui_theme":null,"sorting_config":null}"#).unwrap();
    let failed = Command::new(executable)
        .arg("--appdata")
        .arg(directory.path())
        .args(["lint", "missing", "--fsd-pak"])
        .arg(directory.path().join("missing.pak"))
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!failed.stderr.is_empty());
    assert!(directory.path().join("data/mint.log").exists());
}
