use evj_core::log;

#[test]
fn writes_lines_and_keeps_the_previous_log() {
    let dir = std::env::temp_dir().join(format!("evj-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("evj.log"), "old session\n").unwrap();
    log::init(&dir);
    log::warn("engine", "output 2: swapchain failed");
    log::info("app", "opened show.vjproj");
    let now = std::fs::read_to_string(dir.join("evj.log")).unwrap();
    assert!(now.contains("WARN engine: output 2: swapchain failed"), "{now}");
    assert!(now.contains("INFO app: opened show.vjproj"));
    assert_eq!(std::fs::read_to_string(dir.join("evj.old.log")).unwrap(), "old session\n");
}
