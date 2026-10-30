//! Fresh, read-only JSON proposals. No plan persistence or write authorization.
use crate::{
    ContentConfigReport, ContentDiscovery, ContentReference, Hub, HubError, Result,
    backup_format as format,
};
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ContentConfigInput {
    #[serde(default)]
    #[ts(optional)]
    pub game_options: Option<std::collections::BTreeMap<String, String>>,
    pub default_mods: Option<Vec<String>>,
    pub soundpack: Option<String>,
    pub sound_enabled: Option<bool>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentConfigFilePreview {
    pub path: String,
    pub before_sha256: String,
    pub proposed_sha256: String,
    pub proposed_json: String,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentConfigPlan {
    pub report: ContentConfigReport,
    pub files: Vec<ContentConfigFilePreview>,
    pub proposed_discovery: ContentDiscovery,
    pub write_supported: bool,
}
fn invalid(message: &'static str) -> HubError {
    HubError::new("CONTENT_CONFIG_PREVIEW_UNAVAILABLE", message)
}
fn validate(input: &ContentConfigInput) -> Result<()> {
    if input.game_options.as_ref().is_some_and(|options| {
        options.len() > crate::game_options::KEYS.len()
            || options
                .iter()
                .any(|(key, value)| !crate::game_options::valid(key, value))
    }) {
        return Err(invalid("通用游戏配置包含不支持的字段或值。"));
    }
    let valid = |id: &str| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control);
    if let Some(ids) = &input.default_mods {
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        if ids.len() > 256 || unique.len() != ids.len() || ids.iter().any(|id| !valid(id)) {
            return Err(invalid("默认模组 ID 无效、重复或超过限制。"));
        }
    }
    if input.soundpack.as_deref().is_some_and(|id| !valid(id)) {
        return Err(invalid("声音包 ID 无效或超过限制。"));
    }
    Ok(())
}
fn source(
    root: &Dir,
    path: &str,
    status: &str,
    hash: Option<&str>,
    budget: &mut usize,
) -> Result<(Value, String)> {
    if status != "read" {
        return Err(invalid("目标配置不可读或格式不支持；不能生成替换内容。"));
    }
    let (json, actual) = crate::content_config::document(root, path, budget)
        .map_err(|_| invalid("重新读取目标配置失败。"))?;
    if Some(actual.as_str()) != hash {
        return Err(invalid("配置在读取期间变化，请重新预览。"));
    }
    Ok((json, actual))
}
fn proposal(
    path: &str,
    before: Value,
    after: Value,
    hash: String,
    files: &mut Vec<ContentConfigFilePreview>,
) -> Result<()> {
    if before == after {
        return Ok(());
    }
    let json =
        serde_json::to_string_pretty(&after).map_err(|_| invalid("配置无法序列化。"))? + "\n";
    if json.len() > 1024 * 1024 {
        return Err(invalid("生成配置超过大小限制。"));
    }
    files.push(ContentConfigFilePreview {
        path: path.into(),
        before_sha256: hash,
        proposed_sha256: format::sha(json.as_bytes()),
        proposed_json: json,
    });
    Ok(())
}
impl Hub {
    pub async fn preview_content_config(
        &self,
        instance_id: &str,
        input: ContentConfigInput,
    ) -> Result<ContentConfigPlan> {
        validate(&input)?;
        let _gate = self.mutation.lock().await;
        self.preview_content_config_locked(instance_id, input).await
    }
    pub(crate) async fn preview_content_config_locked(
        &self,
        instance_id: &str,
        input: ContentConfigInput,
    ) -> Result<ContentConfigPlan> {
        validate(&input)?;
        let instance = self.instance(instance_id).await?;
        let root = Dir::open_ambient_dir(&instance.data_root, cap_std::ambient_authority())
            .map_err(|_| invalid("实例目录不可访问。"))?;
        let report = self.inspect_content_config_locked(instance_id).await?;
        tokio::task::spawn_blocking(move || {
            let no_mod_edits = input.default_mods.is_none();
            let mut budget = 2 * 1024 * 1024;
            let mut files = vec![];
            let mut proposed = report.clone();
            proposed.references.clear();
            // Only requested configuration files participate; existing worlds are never edited.
            proposed.worlds.clear();
            proposed.default_mods.status = "not_requested".into();
            if let Some(ids) = input.default_mods {
                let original = &report.default_mods;
                let (before, hash) = source(
                    &root,
                    &original.path,
                    &original.status,
                    original.sha256.as_deref(),
                    &mut budget,
                )?;
                let mut after = before.clone();
                after["dependencies"] = serde_json::json!(ids);
                proposal(&original.path, before, after, hash, &mut files)?;
                proposed.default_mods.mod_ids = ids.clone();
                proposed.default_mods.status = "read".into();
                for id in ids {
                    proposed.references.push(ContentReference {
                        kind: "mod".into(),
                        content_id: id,
                        referenced_by: vec![original.path.clone()],
                        registered_roots: vec![],
                    });
                }
            }
            let edits_sound = input.soundpack.is_some() || input.sound_enabled.is_some();
            if edits_sound || input.game_options.as_ref().is_some_and(|o| !o.is_empty()) {
                let original = &report.sound;
                let (before, hash) = source(
                    &root,
                    &original.path,
                    &original.status,
                    original.sha256.as_deref(),
                    &mut budget,
                )?;
                // A switch change still uses the retained soundpack, when declared.
                if edits_sound
                    && input.soundpack.is_none()
                    && let Some(id) = &original.soundpack
                {
                    proposed.references.push(ContentReference {
                        kind: "soundpack".into(),
                        content_id: id.clone(),
                        referenced_by: vec![original.path.clone()],
                        registered_roots: vec![],
                    });
                }
                let mut after = before.clone();
                for (name, value) in [
                    ("SOUNDPACKS", input.soundpack),
                    ("SOUND_ENABLED", input.sound_enabled.map(|v| v.to_string())),
                ] {
                    if let Some(value) = value {
                        let entry = after
                            .as_array_mut()
                            .and_then(|items| {
                                items.iter_mut().find(|item| {
                                    item.get("name").and_then(Value::as_str) == Some(name)
                                })
                            })
                            .ok_or_else(|| invalid("目标声音字段尚未声明，不推断版本默认格式。"))?;
                        entry["value"] = Value::String(value.clone());
                        if name == "SOUNDPACKS" {
                            proposed.references.push(ContentReference {
                                kind: "soundpack".into(),
                                content_id: value,
                                referenced_by: vec![original.path.clone()],
                                registered_roots: vec![],
                            });
                        }
                    }
                }
                for (name, value) in input.game_options.into_iter().flatten() {
                    let entry = after
                        .as_array_mut()
                        .and_then(|items| {
                            items.iter_mut().find(|item| {
                                item.get("name").and_then(Value::as_str) == Some(name.as_str())
                            })
                        })
                        .ok_or_else(|| {
                            invalid("目标游戏尚未声明该配置项；请先在游戏中保存设置。")
                        })?;
                    if entry.get("value").and_then(Value::as_str).is_none() {
                        return Err(invalid("目标游戏配置值格式不支持。"));
                    }
                    entry["value"] = Value::String(value);
                }
                proposal(&original.path, before, after, hash, &mut files)?;
            }
            let held = same_file::Handle::from_file(root.into_std_file())
                .map_err(|_| invalid("实例目录身份不可读取。"))?;
            if held
                != same_file::Handle::from_path(&report.data_root)
                    .map_err(|_| invalid("实例目录发生变化。"))?
                || std::fs::canonicalize(&report.data_root)
                    .map_err(|_| invalid("实例目录发生变化。"))?
                    != std::path::Path::new(&report.data_root)
            {
                return Err(invalid("实例目录发生变化，请重新预览。"));
            }
            let mut discovery = report.discovery.clone();
            discovery.findings.clear();
            // Game-only edits have no resource references to authorize. Preserve
            // resource observations in report, without gating language/autosave on them.
            if no_mod_edits && !edits_sound {
                discovery = ContentDiscovery {
                    scopes: vec![],
                    packages: vec![],
                    findings: vec![],
                    truncated: false,
                };
            }
            // Sound and mod changes retain their existing resource scan guards.
            let mut diagnostic_scope = discovery.clone();
            if no_mod_edits {
                diagnostic_scope
                    .packages
                    .retain(|p| edits_sound && p.kind == "soundpack");
            }
            crate::content_discovery::diagnose(&proposed, &mut diagnostic_scope);
            discovery.findings = diagnostic_scope.findings;
            discovery.truncated |= diagnostic_scope.truncated;
            Ok(ContentConfigPlan {
                report,
                files,
                proposed_discovery: discovery,
                write_supported: false,
            })
        })
        .await
        .map_err(|_| invalid("配置预览任务失败。"))?
    }
}
