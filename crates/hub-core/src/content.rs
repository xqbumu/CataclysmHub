//! Local metadata registry. Registration never installs or enables game content.
use crate::{
    Hub, HubError, Result,
    database::{id, now},
    paths,
};
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentDefinition {
    pub content_id: String,
    pub name: String,
    pub dependencies: Option<Vec<String>>,
    pub version: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentEntry {
    pub id: String,
    pub kind: String,
    pub root: String,
    pub definitions: Vec<ContentDefinition>,
    pub metadata_sha256: String,
    pub checked_at: String,
    pub last_error: Option<String>,
}
fn invalid() -> HubError {
    HubError::new(
        "CONTENT_METADATA_INVALID",
        "资源元数据无效或超出限制；请选择单个包目录。",
    )
}
fn text(v: &Value) -> Option<String> {
    v.as_str()
        .or_else(|| v.get("str").and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty() && s.len() <= 4096)
        .map(str::to_owned)
}
fn inspect(root: &str, kind: &str) -> Result<ContentEntry> {
    let file = match kind {
        "mod" => "modinfo.json",
        "soundpack" => "soundpack.txt",
        _ => return Err(invalid()),
    };
    let path = paths::canonical(root, true)?;
    if paths::text(&path)? != root {
        return Err(invalid());
    }
    let dir = Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|_| invalid())?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No).nonblock(true);
    let input = dir.open_with(file, &options).map_err(|_| invalid())?;
    if !input.metadata().map_err(|_| invalid())?.is_file() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    input
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > 1024 * 1024 {
        return Err(invalid());
    }
    let mut definitions = Vec::new();
    if kind == "mod" {
        let json: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let values = match &json {
            Value::Array(items) => items.as_slice(),
            Value::Object(_) => std::slice::from_ref(&json),
            _ => return Err(invalid()),
        };
        for value in values
            .iter()
            .filter(|v| v.get("type").and_then(Value::as_str) == Some("MOD_INFO"))
        {
            let dependencies = match value.get("dependencies") {
                None => None,
                Some(Value::Array(items)) if items.len() <= 256 => Some(
                    items
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .filter(|s| !s.is_empty() && s.len() <= 256)
                                .map(str::to_owned)
                                .ok_or_else(invalid)
                        })
                        .collect::<Result<Vec<_>>>()?,
                ),
                _ => return Err(invalid()),
            };
            definitions.push(ContentDefinition {
                content_id: value
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .ok_or_else(invalid)?
                    .into(),
                name: value.get("name").and_then(text).ok_or_else(invalid)?,
                dependencies,
                version: value.get("version").and_then(text),
            });
        }
    } else {
        let source = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
        let mut fields = std::collections::HashMap::new();
        for line in source
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#') && !l.is_empty())
        {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                if matches!(key, "NAME" | "VIEW")
                    && (value.is_empty()
                        || value.len() > 4096
                        || fields.insert(key, value).is_some())
                {
                    return Err(invalid());
                }
            }
        }
        definitions.push(ContentDefinition {
            content_id: fields.get("NAME").ok_or_else(invalid)?.to_string(),
            name: fields.get("VIEW").ok_or_else(invalid)?.to_string(),
            dependencies: None,
            version: None,
        });
    }
    if definitions.is_empty() || definitions.len() > 256 {
        return Err(invalid());
    }
    Ok(ContentEntry {
        id: id(),
        kind: kind.into(),
        root: root.into(),
        definitions,
        metadata_sha256: format!("{:x}", Sha256::digest(&bytes)),
        checked_at: now(),
        last_error: None,
    })
}
impl Hub {
    pub async fn content_entries(&self) -> Result<Vec<ContentEntry>> {
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT snapshot FROM content_entry ORDER BY root,id")
                .fetch_all(&self.pool)
                .await
                .map_err(HubError::db)?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(|_| invalid()))
            .collect()
    }
    pub async fn register_content(&self, root: &str, kind: &str) -> Result<ContentEntry> {
        let _gate = self.mutation.lock().await;
        let root = paths::text(&paths::canonical(root, true)?)?;
        let existing = self.content_entries().await?;
        if existing.len() >= 1000 && !existing.iter().any(|e| e.root == root && e.kind == kind) {
            return Err(HubError::new(
                "CONTENT_LIMIT",
                "资源登记已达上限，请先移除不需要的登记。",
            ));
        }
        let input = root.clone();
        let kind = kind.to_owned();
        let mut entry = tokio::task::spawn_blocking(move || inspect(&input, &kind))
            .await
            .map_err(|_| invalid())??;
        if let Some(old) = existing
            .iter()
            .find(|e| e.root == root && e.kind == entry.kind)
        {
            entry.id = old.id.clone();
        }
        self.save_content(&entry, "content.register").await?;
        Ok(entry)
    }
    pub async fn refresh_content(&self, entry_id: &str) -> Result<ContentEntry> {
        let _gate = self.mutation.lock().await;
        let mut old = self
            .content_entries()
            .await?
            .into_iter()
            .find(|e| e.id == entry_id)
            .ok_or_else(invalid)?;
        let root = old.root.clone();
        let kind = old.kind.clone();
        match tokio::task::spawn_blocking(move || inspect(&root, &kind))
            .await
            .map_err(|_| invalid())?
        {
            Ok(mut entry) => {
                entry.id = old.id;
                self.save_content(&entry, "content.refresh").await?;
                Ok(entry)
            }
            Err(error) => {
                old.last_error = Some(error.code.into());
                self.save_content(&old, "content.unavailable").await?;
                Ok(old)
            }
        }
    }
    async fn save_content(&self, entry: &ContentEntry, action: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO content_entry(id,root,kind,snapshot) VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET snapshot=excluded.snapshot")
            .bind(&entry.id).bind(&entry.root).bind(&entry.kind).bind(serde_json::to_string(entry).map_err(|_| invalid())?).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&entry.id)
            .bind(action)
            .bind("succeeded")
            .bind(entry.last_error.as_deref())
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)
    }
    pub async fn forget_content(&self, entry_id: &str) -> Result<()> {
        let _gate = self.mutation.lock().await;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let result = sqlx::query("DELETE FROM content_entry WHERE id=?")
            .bind(entry_id)
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        if result.rows_affected() > 0 {
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(entry_id)
                .bind("content.forget")
                .bind("succeeded")
                .bind(Option::<String>::None)
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        tx.commit().await.map_err(HubError::db)
    }
}
