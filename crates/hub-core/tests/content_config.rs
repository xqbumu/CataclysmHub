use hub_core::Hub;
use std::{path::PathBuf, sync::Arc};
struct Fixture {
    temp: tempfile::TempDir,
    hub: Arc<Hub>,
    root: PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("instance");
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let pool = sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            temp.path().join("metadata/metadata.sqlite").display()
        ))
        .await
        .unwrap();
        sqlx::query("INSERT INTO installation VALUES('i','game','/game/exe','/game','hash',NULL,NULL,'test','test','now')").execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO play_instance VALUES('p','i','hash','Example',?,'unverified','now')",
        )
        .bind(root.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
        Self { temp, hub, root }
    }
    fn write(&self, path: &str, bytes: &[u8]) {
        let path = self.root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}
#[tokio::test]
async fn observes_ordered_world_references_separately_from_defaults_without_writes() {
    let f = Fixture::new().await;
    let options=br#"[{"name":"SOUNDPACKS","value":"music"},{"name":"SOUND_ENABLED","value":"false"},{"name":"UNKNOWN","value":"retained"}]"#;
    f.write("config/options.json", options);
    f.write(
        "config/user-default-mods.json",
        br#"{"type":"MOD_INFO","id":"user:default","dependencies":["dda","future"]}"#,
    );
    f.write(
        "save/World B/mods.json",
        br#"["dda","second","first","second"]"#,
    );
    f.write("save/World A/mods.json", b"[]");
    let package = f.temp.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(
        package.join("modinfo.json"),
        br#"{"type":"MOD_INFO","id":"second","name":"Second"}"#,
    )
    .unwrap();
    f.hub
        .register_content(package.to_str().unwrap(), "mod")
        .await
        .unwrap();
    let before = f.hub.dashboard().await.unwrap().operations.len();
    let report = f.hub.inspect_content_config("p").await.unwrap();
    assert_eq!(report.sound.soundpack.as_deref(), Some("music"));
    assert_eq!(report.sound.sound_enabled.as_deref(), Some("false"));
    assert_eq!(report.default_mods.mod_ids, vec!["dda", "future"]);
    assert_eq!(report.worlds[0].status, "read");
    assert!(report.worlds[0].mod_ids.is_empty());
    assert_eq!(
        report.worlds[1].mod_ids,
        vec!["dda", "second", "first", "second"]
    );
    let reference = report
        .references
        .iter()
        .find(|r| r.content_id == "second")
        .unwrap();
    assert_eq!(reference.referenced_by, vec!["save/World B/mods.json"]);
    assert_eq!(reference.registered_roots.len(), 1);
    assert!(
        report
            .references
            .iter()
            .find(|r| r.content_id == "dda")
            .unwrap()
            .registered_roots
            .is_empty()
    );
    assert!(!report.binding_verified);
    assert!(report.installation_version.is_none());
    assert_eq!(
        std::fs::read(f.root.join("config/options.json")).unwrap(),
        options
    );
    assert_eq!(f.hub.dashboard().await.unwrap().operations.len(), before);
}
#[tokio::test]
async fn missing_corrupt_and_duplicate_fields_are_not_treated_as_disabled_or_empty() {
    let f = Fixture::new().await;
    let report = f.hub.inspect_content_config("p").await.unwrap();
    assert_eq!(report.sound.status, "missing");
    assert_eq!(report.default_mods.status, "missing");
    assert_eq!(report.issues, vec!["save: missing"]);
    f.write(
        "config/options.json",
        br#"[{"name":"SOUNDPACKS","value":"a"},{"name":"SOUNDPACKS","value":"b"}]"#,
    );
    f.write("config/user-default-mods.json", b"[]");
    f.write("save/corrupt/mods.json", b"[broken");
    let report = f.hub.inspect_content_config("p").await.unwrap();
    assert_eq!(report.sound.status, "unsupported_format");
    assert!(report.sound.soundpack.is_none());
    assert_eq!(report.default_mods.status, "unsupported_format");
    assert_eq!(report.worlds[0].status, "invalid_json");
    assert!(report.references.is_empty());
    f.write("config/options.json", &vec![b' '; 1024 * 1024 + 1]);
    assert_eq!(
        f.hub
            .inspect_content_config("p")
            .await
            .unwrap()
            .sound
            .status,
        "limit_exceeded"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn linked_world_and_configuration_are_not_followed() {
    let f = Fixture::new().await;
    let outside = f.temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("mods.json"), b"[\"secret\"]").unwrap();
    std::fs::create_dir(f.root.join("save")).unwrap();
    std::os::unix::fs::symlink(&outside, f.root.join("save/linked")).unwrap();
    std::os::unix::fs::symlink(&outside, f.root.join("config")).unwrap();
    let report = f.hub.inspect_content_config("p").await.unwrap();
    assert_eq!(report.sound.status, "unsafe_path");
    assert_eq!(report.worlds[0].status, "unsafe_path");
    assert!(report.references.is_empty());
}
#[tokio::test]
async fn too_many_world_candidates_fail_instead_of_claiming_a_complete_empty_scan() {
    let f = Fixture::new().await;
    for i in 0..129 {
        std::fs::create_dir_all(f.root.join(format!("save/world-{i}"))).unwrap();
    }
    assert_eq!(
        f.hub.inspect_content_config("p").await.unwrap_err().code,
        "CONTENT_CONFIG_UNAVAILABLE"
    );
}

#[tokio::test]
async fn read_only_inspection_keeps_unknown_session_protection() {
    let f = Fixture::new().await;
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f.temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("INSERT INTO session(id,installation_id,state,started_at) VALUES('s','i','interrupted_unknown','now')").execute(&pool).await.unwrap();
    f.hub.inspect_content_config("p").await.unwrap();
    let data = f.hub.dashboard().await.unwrap();
    assert_eq!(data.sessions[0].state, "interrupted_unknown");
    assert!(data.resource_claims.iter().any(|c| c.scope == "global"));
    assert!(data.operations.is_empty());
}
