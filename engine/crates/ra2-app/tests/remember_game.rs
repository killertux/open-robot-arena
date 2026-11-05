//! A folder named once with `--game` is remembered in the save folder and is
//! found again without a flag. Own test binary: it configures the process-wide
//! save directory.

#[test]
fn a_named_install_is_remembered_in_the_save_folder() {
    let root = std::env::temp_dir().join(format!("ra2-remember-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let install = root.join("Robot Arena 2");
    std::fs::create_dir_all(install.join("UI")).unwrap();
    std::fs::create_dir_all(install.join("Scripts")).unwrap();
    std::fs::write(install.join("RA2.cfz"), b"x").unwrap();
    let save = root.join("save");
    ra2fmt::saves::configure(Some(save.clone()));

    assert_eq!(ra2_app::remembered_game(), None);
    ra2_app::remember_game(&install);
    assert!(save.join("game-dir.txt").is_file());
    assert_eq!(ra2_app::remembered_game().as_deref(), Some(&*install));

    // A remembered folder that stopped being an install is ignored.
    std::fs::remove_file(install.join("RA2.cfz")).unwrap();
    assert_eq!(ra2_app::remembered_game(), None);
    std::fs::remove_dir_all(&root).ok();
}
