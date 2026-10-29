use hub_core::Hub;

#[tokio::test]
async fn local_registry_survives_restart_refresh_failure_and_forget_without_touching_files() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    let package = temp.path().join("mod");
    std::fs::create_dir(&package).unwrap();
    let source = br#"[{"type":"MOD_INFO","id":"example","name":{"str":"Example"},"dependencies":["dda"],"version":"1"},{"type":"MOD_INFO","id":"other","name":"Other"}]"#;
    std::fs::write(package.join("modinfo.json"), source).unwrap();
    let hub = Hub::open(&data).await.unwrap();
    let entry = hub
        .register_content(package.to_str().unwrap(), "mod")
        .await
        .unwrap();
    assert_eq!(entry.definitions.len(), 2);
    assert_eq!(entry.definitions[0].dependencies, Some(vec!["dda".into()]));
    assert_eq!(entry.definitions[1].dependencies, None);
    assert_eq!(
        hub.register_content(package.to_str().unwrap(), "mod")
            .await
            .unwrap()
            .id,
        entry.id
    );
    assert_eq!(hub.content_entries().await.unwrap().len(), 1);
    drop(hub);
    let hub = Hub::open(&data).await.unwrap();
    assert_eq!(
        hub.content_entries().await.unwrap()[0].metadata_sha256,
        entry.metadata_sha256
    );
    std::fs::write(package.join("modinfo.json"), b"invalid").unwrap();
    let stale = hub.refresh_content(&entry.id).await.unwrap();
    assert!(stale.last_error.is_some());
    assert_eq!(stale.metadata_sha256, entry.metadata_sha256);
    std::fs::write(package.join("modinfo.json"), source).unwrap();
    assert!(
        hub.refresh_content(&entry.id)
            .await
            .unwrap()
            .last_error
            .is_none()
    );
    hub.forget_content(&entry.id).await.unwrap();
    assert!(hub.content_entries().await.unwrap().is_empty());
    assert_eq!(std::fs::read(package.join("modinfo.json")).unwrap(), source);
}

#[tokio::test]
async fn soundpack_requires_unique_name_and_view_and_rejects_invalid_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
    let root = temp.path().join("sound");
    std::fs::create_dir(&root).unwrap();
    for bad in ["NAME: only", "NAME: a\nVIEW: b\nNAME: c", "NAME: \nVIEW: b"] {
        std::fs::write(root.join("soundpack.txt"), bad).unwrap();
        assert!(
            hub.register_content(root.to_str().unwrap(), "soundpack")
                .await
                .is_err()
        );
    }
    std::fs::write(
        root.join("soundpack.txt"),
        "# comment\r\nNAME: pack\r\nVIEW: My Pack\r\n",
    )
    .unwrap();
    let entry = hub
        .register_content(root.to_str().unwrap(), "soundpack")
        .await
        .unwrap();
    assert_eq!(entry.definitions[0].content_id, "pack");
    assert_eq!(entry.definitions[0].name, "My Pack");
    assert!(entry.definitions[0].version.is_none());
    assert!(
        hub.register_content(root.to_str().unwrap(), "tileset")
            .await
            .is_err()
    );
    std::fs::write(root.join("modinfo.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(
        hub.register_content(root.to_str().unwrap(), "mod")
            .await
            .is_err()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn manifest_symlink_is_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
    let root = temp.path().join("mod");
    std::fs::create_dir(&root).unwrap();
    let outside = temp.path().join("external.json");
    std::fs::write(
        &outside,
        br#"{"type":"MOD_INFO","id":"external","name":"External"}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, root.join("modinfo.json")).unwrap();
    assert!(
        hub.register_content(root.to_str().unwrap(), "mod")
            .await
            .is_err()
    );
    assert!(hub.content_entries().await.unwrap().is_empty());
}

#[tokio::test]
async fn audit_failure_rolls_back_registration_and_forget() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    let hub = Hub::open(&data).await.unwrap();
    let root = temp.path().join("mod");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("modinfo.json"),
        br#"{"type":"MOD_INFO","id":"a","name":"A"}"#,
    )
    .unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        data.join("metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER reject_content BEFORE INSERT ON operation_log WHEN NEW.action LIKE 'content.%' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    assert!(
        hub.register_content(root.to_str().unwrap(), "mod")
            .await
            .is_err()
    );
    assert!(hub.content_entries().await.unwrap().is_empty());
    sqlx::query("DROP TRIGGER reject_content")
        .execute(&pool)
        .await
        .unwrap();
    let entry = hub
        .register_content(root.to_str().unwrap(), "mod")
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_content BEFORE INSERT ON operation_log WHEN NEW.action='content.forget' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    assert!(hub.forget_content(&entry.id).await.is_err());
    assert_eq!(hub.content_entries().await.unwrap().len(), 1);
}

#[tokio::test]
async fn schema10_upgrade_backs_up_metadata_and_failed_migration_preserves_version() {
    for fail in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("metadata");
        drop(Hub::open(&data).await.unwrap());
        let url = format!("sqlite://{}", data.join("metadata.sqlite").display());
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        if !fail {
            sqlx::raw_sql("DROP TABLE config_change; DROP TABLE content_install_task; DROP TABLE content_entry")
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("PRAGMA user_version=10")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        assert_eq!(Hub::open(&data).await.is_err(), fail);
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&pool)
                .await
                .unwrap(),
            if fail { 10 } else { 14 }
        );
        assert!(std::fs::read_dir(&data).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("metadata-v10-")
        }));
    }
}
