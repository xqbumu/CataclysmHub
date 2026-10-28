//! Read-only observations of the explicit Hub instance layout, never configuration authority.
use crate::{ContentEntry, Hub, HubError, Result, backup_format as format, database::now, paths};
use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, io::Read};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct ModReferenceFile {
    pub path: String,
    pub status: String,
    pub sha256: Option<String>,
    pub mod_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct SoundConfiguration {
    pub path: String,
    pub status: String,
    pub sha256: Option<String>,
    pub soundpack: Option<String>,
    pub sound_enabled: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentReference {
    pub kind: String,
    pub content_id: String,
    pub referenced_by: Vec<String>,
    pub registered_roots: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentConfigReport {
    pub instance_id: String,
    pub data_root: String,
    pub installation_version: Option<String>,
    pub checked_at: String,
    pub sound: SoundConfiguration,
    pub default_mods: ModReferenceFile,
    pub worlds: Vec<ModReferenceFile>,
    pub references: Vec<ContentReference>,
    pub issues: Vec<String>,
    pub binding_verified: bool,
}
fn error() -> HubError {
    HubError::new(
        "CONTENT_CONFIG_UNAVAILABLE",
        "实例配置范围不可访问或超出读取限制。",
    )
}
fn child(root: &Dir, name: &str) -> std::result::Result<Dir, &'static str> {
    let m = root.symlink_metadata(name).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "missing"
        } else {
            "unreadable"
        }
    })?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err("unsafe_path");
    }
    root.open_dir_nofollow(name).map_err(|_| "unreadable")
}
fn document(
    root: &Dir,
    path: &str,
    remaining: &mut usize,
) -> std::result::Result<(Value, String), &'static str> {
    let mut parts = path.split('/').collect::<Vec<_>>();
    let name = parts.pop().ok_or("invalid_path")?;
    let mut dir = root.try_clone().map_err(|_| "unreadable")?;
    for part in parts {
        dir = child(&dir, part)?;
    }
    let m = dir.symlink_metadata(name).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "missing"
        } else {
            "unreadable"
        }
    })?;
    if !m.is_file() || m.file_type().is_symlink() {
        return Err("unsafe_path");
    }
    let input = format::open_file(&dir, name).map_err(|_| "unreadable")?;
    let limit = (*remaining).min(1024 * 1024);
    let mut bytes = Vec::new();
    input
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable")?;
    *remaining = remaining.saturating_sub(bytes.len());
    if bytes.len() > limit {
        return Err("limit_exceeded");
    }
    let json = serde_json::from_slice(&bytes).map_err(|_| "invalid_json")?;
    Ok((json, format::sha(&bytes)))
}
fn identifier(v: &Value) -> Option<String> {
    v.as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .map(str::to_owned)
}
fn mods(root: &Dir, path: &str, remaining: &mut usize) -> ModReferenceFile {
    let mut result = ModReferenceFile {
        path: path.into(),
        status: "read".into(),
        sha256: None,
        mod_ids: vec![],
    };
    match document(root, path, remaining) {
        Ok((json, hash)) => {
            result.sha256 = Some(hash);
            let values = if path == "config/user-default-mods.json" {
                if json.get("type").and_then(Value::as_str) == Some("MOD_INFO")
                    && json.get("id").and_then(Value::as_str) == Some("user:default")
                {
                    json.get("dependencies").and_then(Value::as_array)
                } else {
                    None
                }
            } else {
                json.as_array()
            };
            if let Some(values) = values.filter(|a| a.len() <= 256) {
                if let Some(ids) = values.iter().map(identifier).collect::<Option<Vec<_>>>() {
                    result.mod_ids = ids;
                } else {
                    result.status = "unsupported_format".into();
                }
            } else {
                result.status = "unsupported_format".into();
            }
        }
        Err(status) => result.status = status.into(),
    }
    result
}
fn sound(root: &Dir, remaining: &mut usize) -> SoundConfiguration {
    let mut result = SoundConfiguration {
        path: "config/options.json".into(),
        status: "read".into(),
        sha256: None,
        soundpack: None,
        sound_enabled: None,
    };
    match document(root, &result.path, remaining) {
        Ok((json, hash)) => {
            result.sha256 = Some(hash);
            let Some(values) = json.as_array().filter(|a| a.len() <= 4096) else {
                result.status = "unsupported_format".into();
                return result;
            };
            let mut seen = std::collections::HashSet::new();
            for value in values {
                let Some(name) = value.get("name").and_then(Value::as_str) else {
                    result.status = "unsupported_format".into();
                    break;
                };
                if matches!(name, "SOUNDPACKS" | "SOUND_ENABLED") {
                    let setting = value.get("value").and_then(identifier);
                    if !seen.insert(name) || setting.is_none() {
                        result.status = "unsupported_format".into();
                        break;
                    }
                    if name == "SOUNDPACKS" {
                        result.soundpack = setting;
                    } else {
                        result.sound_enabled = setting;
                    }
                }
            }
            if result.status != "read" {
                result.soundpack = None;
                result.sound_enabled = None;
            }
        }
        Err(status) => result.status = status.into(),
    }
    result
}
type Observed = (
    SoundConfiguration,
    ModReferenceFile,
    Vec<ModReferenceFile>,
    Vec<ContentReference>,
    Vec<String>,
);
fn observe(root: &Dir, entries: &[ContentEntry]) -> Result<Observed> {
    let mut budget = 16 * 1024 * 1024;
    let sound = sound(root, &mut budget);
    let default_mods = mods(root, "config/user-default-mods.json", &mut budget);
    let mut worlds = Vec::new();
    let mut issues = Vec::new();
    match child(root, "save") {
        Ok(save) => {
            let mut names = Vec::new();
            for (index, item) in save.entries().map_err(|_| error())?.enumerate() {
                if index >= 512 {
                    return Err(error());
                }
                let item = item.map_err(|_| error())?;
                let name = item.file_name().into_string().map_err(|_| error())?;
                format::validate_path(&name)?;
                let meta = save.symlink_metadata(&name).map_err(|_| error())?;
                if meta.is_dir() || meta.file_type().is_symlink() {
                    names.push(name);
                }
            }
            names.sort();
            if names.len() > 128 {
                return Err(error());
            }
            for name in names {
                worlds.push(mods(root, &format!("save/{name}/mods.json"), &mut budget));
            }
        }
        Err(status) => issues.push(format!("save: {status}")),
    }
    let mut refs: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for file in std::iter::once(&default_mods)
        .chain(worlds.iter())
        .filter(|f| f.status == "read")
    {
        for id in &file.mod_ids {
            let list = refs.entry(("mod".into(), id.clone())).or_default();
            if !list.contains(&file.path) {
                list.push(file.path.clone());
            }
        }
    }
    if sound.status == "read"
        && let Some(id) = &sound.soundpack
    {
        refs.entry(("soundpack".into(), id.clone()))
            .or_default()
            .push(sound.path.clone());
    }
    let references = refs
        .into_iter()
        .map(|((kind, id), referenced_by)| {
            let registered_roots = entries
                .iter()
                .filter(|e| {
                    e.kind == kind
                        && e.last_error.is_none()
                        && e.definitions.iter().any(|d| d.content_id == id)
                })
                .map(|e| e.root.clone())
                .collect();
            ContentReference {
                kind,
                content_id: id,
                referenced_by,
                registered_roots,
            }
        })
        .collect();
    Ok((sound, default_mods, worlds, references, issues))
}
impl Hub {
    pub async fn inspect_content_config(&self, instance_id: &str) -> Result<ContentConfigReport> {
        let _gate = self.mutation.lock().await;
        let instance = self.instance(instance_id).await?;
        let installation = self.installation(&instance.installation_id).await?;
        let path = paths::canonical(&instance.data_root, true)?;
        if paths::text(&path)? != instance.data_root {
            return Err(error());
        }
        let entries = self.content_entries().await?;
        tokio::task::spawn_blocking(move || {
            let root =
                Dir::open_ambient_dir(&path, cap_std::ambient_authority()).map_err(|_| error())?;
            let (sound, default_mods, worlds, references, issues) = observe(&root, &entries)?;
            let held = same_file::Handle::from_file(
                root.try_clone().map_err(|_| error())?.into_std_file(),
            )
            .map_err(|_| error())?;
            if std::fs::canonicalize(&path).map_err(|_| error())? != path
                || held != same_file::Handle::from_path(&path).map_err(|_| error())?
            {
                return Err(error());
            }

            Ok(ContentConfigReport {
                instance_id: instance.id,
                data_root: instance.data_root,
                installation_version: installation.version,
                checked_at: now(),
                sound,
                default_mods,
                worlds,
                references,
                issues,
                binding_verified: false,
            })
        })
        .await
        .map_err(|_| error())?
    }
}
