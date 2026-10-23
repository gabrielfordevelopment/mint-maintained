use std::process::Command;

const CHILD_LOG: &str = "MINT_LOGGING_TEST_CHILD_LOG";
const SECRET: &str = "registered-secret-value";

// The game hook can be terminated without unwinding, so nothing may wait for a guard or
// buffer flush. Abort a child process and check what reached its log file.
#[test]
fn log_and_panic_survive_abort() {
    if let Some(path) = std::env::var_os(CHILD_LOG) {
        mint_lib::setup_logging(&path, "logging").unwrap();
        mint_lib::redaction::register_secrets([SECRET]);
        tracing::info!("written before abort with {SECRET} and ?access_token=plain-token");
        let _ = std::panic::catch_unwind(|| panic!("panic before abort with {SECRET}"));
        std::process::abort();
    }

    let path = std::env::temp_dir().join(format!("mint-logging-test-{}.log", std::process::id()));
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "log_and_panic_survive_abort", "--test-threads=1"])
        .env(CHILD_LOG, &path)
        .status()
        .unwrap();
    let log = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    let log = log.unwrap();

    assert!(!status.success(), "the child process should have aborted");
    assert!(log.contains("version: "), "{log}");
    assert!(log.contains("written before abort"), "{log}");
    assert!(log.contains("panic before abort"), "{log}");
    assert!(log.contains("panic on thread"), "{log}");
    assert!(log.contains("logging.rs"), "{log}");
    assert!(log.contains("access_token=[redacted]"), "{log}");
    assert!(!log.contains(SECRET), "{log}");
    assert!(!log.contains("plain-token"), "{log}");
}
