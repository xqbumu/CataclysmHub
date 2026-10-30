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

#[tokio::test]
async fn discovers_candidates_and_diagnoses_dependencies_without_choosing_duplicate_ids() {
    let f = Fixture::new().await;
    let game = f.temp.path().join("game");
    std::fs::create_dir_all(game.join("data/mods/core")).unwrap();
    std::fs::write(game.join("data/mods/core/modinfo.json"), br#"[{"type":"MOD_INFO","id":"dda","name":"Core","dependencies":[]},{"type":"MOD_INFO","id":"duplicate","name":"Built in","dependencies":[]}]"#).unwrap();
    let game = std::fs::canonicalize(game).unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f.temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("UPDATE installation SET working_dir=? WHERE id='i'")
        .bind(game.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    f.write(
        "mods/local/modinfo.json",
        br#"[
      {"type":"MOD_INFO","id":"local","name":"Local","dependencies":["dda","missing","duplicate"]},
      {"type":"MOD_INFO","id":"duplicate","name":"User","dependencies":[]},
      {"type":"MOD_INFO","id":"unknown","name":"Unknown"},
      {"type":"MOD_INFO","id":"a","name":"A","dependencies":["b"]},
      {"type":"MOD_INFO","id":"b","name":"B","dependencies":["a"]}
    ]"#,
    );
    f.write("sound/music/soundpack.txt", b"NAME: music\nVIEW: Music\n");
    f.write("save/World/mods.json", br#"["local","external"]"#);
    f.write(
        "config/options.json",
        br#"[{"name":"SOUNDPACKS","value":"music"}]"#,
    );
    let external = f.temp.path().join("external");
    std::fs::create_dir(&external).unwrap();
    std::fs::write(
        external.join("modinfo.json"),
        br#"{"type":"MOD_INFO","id":"external","name":"External","dependencies":[]}"#,
    )
    .unwrap();
    f.hub
        .register_content(external.to_str().unwrap(), "mod")
        .await
        .unwrap();
    let report = f.hub.inspect_content_config("p").await.unwrap();
    let d = &report.discovery;
    assert_eq!(d.packages.len(), 3);
    assert!(!d.truncated);
    assert_eq!(d.scopes.iter().filter(|s| s.status == "scanned").count(), 3);
    let has = |code: &str, id: &str, dep: Option<&str>| {
        d.findings
            .iter()
            .any(|x| x.code == code && x.content_id == id && x.related_id.as_deref() == dep)
    };
    assert!(has("duplicate_id", "duplicate", None));
    assert!(has("dependency_not_observed", "local", Some("missing")));
    assert!(has("ambiguous_dependency", "local", Some("duplicate")));
    assert!(has("dependencies_unknown", "unknown", None));
    assert!(has("dependency_cycle", "b", Some("a")));
    assert!(has("dependency_not_listed", "local", Some("dda")));
    assert!(has("reference_not_observed", "external", None));
    assert!(!has("dependency_not_observed", "local", Some("dda")));
    assert!(!has("reference_not_observed", "music", None));
    assert_eq!(
        std::fs::read(f.root.join("save/World/mods.json")).unwrap(),
        br#"["local","external"]"#
    );
}

#[tokio::test]
async fn candidate_discovery_reports_invalid_metadata_and_bounded_depth() {
    let f = Fixture::new().await;
    f.write("mods/bad/modinfo.json", b"broken");
    f.write(
        "mods/deep/a/b/c/d/e/f/g/h/i/modinfo.json",
        br#"{"type":"MOD_INFO","id":"deep","name":"Deep"}"#,
    );
    f.write("sound/huge/soundpack.txt", &vec![b'a'; 1024 * 1024 + 1]);
    let report = f.hub.inspect_content_config("p").await.unwrap();
    assert!(report.discovery.truncated);
    assert!(report.discovery.packages.is_empty());
    assert!(
        report
            .discovery
            .findings
            .iter()
            .any(|f| f.code == "invalid_metadata")
    );
    assert!(
        report
            .discovery
            .scopes
            .iter()
            .filter(|s| s.basis == "instance_userdir")
            .all(|s| s.status == "partial")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn candidate_discovery_never_follows_external_directory_or_metadata_links() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new().await;
    let external = f.temp.path().join("outside");
    std::fs::create_dir(&external).unwrap();
    std::fs::write(
        external.join("modinfo.json"),
        br#"{"type":"MOD_INFO","id":"outside","name":"Outside"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.root.join("mods/file")).unwrap();
    symlink(&external, f.root.join("mods/directory")).unwrap();
    symlink(
        external.join("modinfo.json"),
        f.root.join("mods/file/modinfo.json"),
    )
    .unwrap();
    symlink(&external, f.root.join("sound")).unwrap();
    let report = f.hub.inspect_content_config("p").await.unwrap();
    assert!(report.discovery.packages.is_empty());
    assert_eq!(report.discovery.scopes[0].status, "partial");
    assert_eq!(report.discovery.scopes[1].status, "unsafe_path");
    assert!(
        report
            .discovery
            .findings
            .iter()
            .any(|f| f.code == "unsafe_metadata")
    );
}

#[tokio::test]
async fn fresh_config_preview_preserves_other_fields_and_never_writes() {
    let f = Fixture::new().await;
    let original = br#"[{"name":"SOUNDPACKS","value":"old","extra":42},{"name":"SOUND_ENABLED","value":"false"},{"name":"OTHER","value":"untouched"}]"#;
    f.write("config/options.json", original);
    f.write(
        "config/user-default-mods.json",
        br#"{"type":"MOD_INFO","id":"user:default","dependencies":["dda"],"custom":{"keep":true}}"#,
    );
    f.write("save/W/mods.json", br#"["dda"]"#);
    let before = f.hub.dashboard().await.unwrap().operations.len();
    let input = hub_core::ContentConfigInput {
        default_mods: Some(vec![]),
        soundpack: Some("new".into()),
        sound_enabled: Some(true),
    };
    let plan = f.hub.preview_content_config("p", input).await.unwrap();
    assert!(!plan.write_supported);
    assert_eq!(plan.files.len(), 2);
    let options: serde_json::Value = serde_json::from_str(&plan.files[1].proposed_json).unwrap();
    assert_eq!(options[0]["value"], "new");
    assert_eq!(options[0]["extra"], 42);
    assert_eq!(options[1]["value"], "true");
    assert_eq!(options[2]["value"], "untouched");
    let defaults: serde_json::Value = serde_json::from_str(&plan.files[0].proposed_json).unwrap();
    assert_eq!(defaults["dependencies"], serde_json::json!([]));
    assert_eq!(defaults["custom"]["keep"], true);
    assert_eq!(
        std::fs::read(f.root.join("config/options.json")).unwrap(),
        original
    );
    assert_eq!(
        std::fs::read(f.root.join("save/W/mods.json")).unwrap(),
        br#"["dda"]"#
    );
    assert_eq!(f.hub.dashboard().await.unwrap().operations.len(), before);
    f.write(
        "config/options.json",
        br#"[{"name":"SOUND_ENABLED","value":"true"}]"#,
    );
    let plan = f
        .hub
        .preview_content_config(
            "p",
            hub_core::ContentConfigInput {
                default_mods: None,
                soundpack: None,
                sound_enabled: Some(false),
            },
        )
        .await
        .unwrap();
    assert_eq!(plan.report.sound.sound_enabled.as_deref(), Some("true"));
    assert_eq!(plan.files.len(), 1);
}

#[tokio::test]
async fn config_preview_rejects_unknown_baselines_and_invalid_drafts() {
    let f = Fixture::new().await;
    for input in [
        hub_core::ContentConfigInput {
            default_mods: Some(vec!["a".into(), "a".into()]),
            soundpack: None,
            sound_enabled: None,
        },
        hub_core::ContentConfigInput {
            default_mods: Some(vec![]),
            soundpack: None,
            sound_enabled: None,
        },
        hub_core::ContentConfigInput {
            default_mods: None,
            soundpack: Some("music".into()),
            sound_enabled: None,
        },
    ] {
        assert!(f.hub.preview_content_config("p", input).await.is_err());
    }
    f.write(
        "config/options.json",
        br#"[{"name":"OTHER","value":"keep"}]"#,
    );
    assert!(
        f.hub
            .preview_content_config(
                "p",
                hub_core::ContentConfigInput {
                    default_mods: None,
                    soundpack: None,
                    sound_enabled: Some(true)
                }
            )
            .await
            .is_err()
    );
    let plan = f
        .hub
        .preview_content_config(
            "p",
            hub_core::ContentConfigInput {
                default_mods: None,
                soundpack: None,
                sound_enabled: None,
            },
        )
        .await
        .unwrap();
    assert!(plan.files.is_empty());
    assert!(!f.root.join("config/user-default-mods.json").exists());
}

#[tokio::test]
async fn config_preview_rejects_duplicate_json_keys_and_preserves_noop_bytes() {
    let f = Fixture::new().await;
    f.write(
        "config/options.json",
        br#"[{"name":"SOUND_ENABLED","value":"true","value":"false"}]"#,
    );
    let input = hub_core::ContentConfigInput {
        default_mods: None,
        soundpack: None,
        sound_enabled: Some(false),
    };
    assert!(
        f.hub
            .preview_content_config("p", input.clone())
            .await
            .is_err()
    );
    let original = br#"[ { "name" : "SOUND_ENABLED", "value" : "false", "extra": null } ]"#;
    f.write("config/options.json", original);
    let plan = f.hub.preview_content_config("p", input).await.unwrap();
    assert!(plan.files.is_empty());
    assert_eq!(
        std::fs::read(f.root.join("config/options.json")).unwrap(),
        original
    );
}

#[tokio::test]
async fn obsolete_mods_remain_visible_for_old_worlds_but_diagnose_new_defaults() {
    let f = Fixture::new().await;
    f.write(
        "mods/old/modinfo.json",
        br#"{"type":"MOD_INFO","id":"old","name":"Old","dependencies":[],"obsolete":true}"#,
    );
    f.write(
        "config/user-default-mods.json",
        br#"{"type":"MOD_INFO","id":"user:default","dependencies":[]}"#,
    );
    f.write("save/Legacy/mods.json", br#"["old"]"#);
    let report = f.hub.inspect_content_config("p").await.unwrap();
    assert!(report.discovery.packages[0].definitions[0].obsolete);
    assert!(
        !report
            .discovery
            .findings
            .iter()
            .any(|f| f.code == "obsolete_default_mod")
    );
    let plan = f
        .hub
        .preview_content_config(
            "p",
            hub_core::ContentConfigInput {
                default_mods: Some(vec!["old".into()]),
                soundpack: None,
                sound_enabled: None,
            },
        )
        .await
        .unwrap();
    assert!(
        plan.proposed_discovery
            .findings
            .iter()
            .any(|f| f.code == "obsolete_default_mod")
    );
    assert_eq!(
        std::fs::read(f.root.join("save/Legacy/mods.json")).unwrap(),
        br#"["old"]"#
    );
}
