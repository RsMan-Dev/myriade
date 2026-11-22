use super::*;
use super::prepared_files::missing_resources;

#[test]
fn a_folder_without_prepared_files_is_named_before_loading() {
    let root = std::env::temp_dir().join(format!("horizon-unprepared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let unprepared = root.join(".local/horizon-client/26.3");
    let missing = missing_resources(&unprepared, None).expect("no manifest is reported");
    assert!(missing.contains("horizon-resources.json"), "{missing}");
    assert!(missing.contains("horizon-unprepared-"), "{missing}");
    assert!(missing.contains("horizon-prepare --prepare"), "{missing}");
    // No hook: no site, no launcher.
    assert!(!missing.contains("playhorizon"), "{missing}");
    assert!(!missing.contains("launcher"), "{missing}");
    let get = horizon_hooks::GetTheGame {
        address: "https://example.org/get",
        title: "",
        paragraphs: &[],
        button: "",
    };
    let missing = missing_resources(&unprepared, Some(get)).expect("no manifest is reported");
    assert!(
        missing.contains("launcher: download it at https://example.org/get"),
        "{missing}"
    );
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("horizon-resources.json"), "{}").unwrap();
    assert_eq!(missing_resources(&root, None), None);

    // A killed export leaves an empty file behind; the error names it
    // instead of serde's "expected value at line 1 column 1".
    std::fs::write(root.join("gameplay-facts.json"), "").unwrap();
    let damaged = missing_resources(&root, None).expect("an empty prepared file is reported");
    assert!(damaged.contains("gameplay-facts.json"), "{damaged}");
    assert!(damaged.contains("damaged"), "{damaged}");
    std::fs::write(root.join("gameplay-facts.json"), "{}").unwrap();
    assert_eq!(missing_resources(&root, None), None);

    std::fs::write(root.join("horizon-resources.json"), " \n").unwrap();
    let damaged = missing_resources(&root, None).expect("an empty manifest is reported");
    assert!(damaged.contains("horizon-resources.json"), "{damaged}");
    std::fs::remove_dir_all(&root).unwrap();
}
