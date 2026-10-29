//! Curated upstream releases and immutable local download receipts; never executes archives.
use cap_fs_ext::{DirExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use tokio::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub repo: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub url: String,
    pub digest: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub prerelease: bool,
    pub published: String,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Listing {
    pub attempted: bool,
    pub updated: Option<String>,
    pub error: Option<String>,
    pub releases: Vec<Release>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Download {
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub asset_id: u64,
    #[serde(default)]
    pub downloaded_at: String,
    pub id: String,
    pub source: String,
    pub tag: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub upstream_verified: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub listings: std::collections::BTreeMap<String, Listing>,
    pub downloads: Vec<Download>,
}
#[derive(Serialize)]
pub struct View {
    pub sources: Vec<Source>,
    pub catalog: Catalog,
}
pub fn sources() -> Vec<Source> {
    [
        (
            "dda",
            "Dark Days Ahead",
            "game",
            "CleverRaven/Cataclysm-DDA",
        ),
        ("bn", "Bright Nights", "game", "cataclysmbn/Cataclysm-BN"),
        ("eod", "Era of Decay", "game", "AtomicFox556/Cataclysm-EOD"),
        (
            "tish",
            "There Is Still Hope",
            "game",
            "Cataclysm-TISH-team/Cataclysm-TISH",
        ),
        (
            "tlg",
            "The Last Generation",
            "game",
            "Cataclysm-TLG/Cataclysm-TLG",
        ),
        (
            "sound",
            "CC-Sounds / CDDA Soundpacks",
            "soundpack",
            "Fris0uman/CDDA-Soundpacks",
        ),
        (
            "kenan",
            "Kenan DDA 模组合集（停更 / 历史测试包）",
            "mod",
            "Kenan2000/CDDA-Structured-Kenan-Modpack",
        ),
        (
            "bnmods",
            "Kenan BN 模组合集（固定提交）",
            "mod",
            "Zlorthishen/BrightNights-Structured-Kenan-Modpack",
        ),
    ]
    .into_iter()
    .map(|(id, name, kind, repo)| Source {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        repo: repo.into(),
    })
    .collect()
}
fn failure(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}
#[derive(Clone, Serialize)]
pub struct Transfer {
    pub name: String,
    pub received: u64,
    pub total: u64,
}
pub struct Store {
    progress: std::sync::Mutex<Option<Transfer>>,
    cancel: tokio::sync::watch::Sender<bool>,
    dir: Dir,
    state: Mutex<Catalog>,
    client: reqwest::Client,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self, String> {
        let dir = Dir::open_ambient_dir(root, cap_std::ambient_authority()).map_err(failure)?;
        let state = match dir.open_with(
            "remote-catalog.json",
            OpenOptions::new()
                .read(true)
                .follow(cap_fs_ext::FollowSymlinks::No),
        ) {
            Ok(file) => serde_json::from_reader(file.take(16 * 1024 * 1024)).map_err(failure)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Catalog::default(),
            Err(e) => return Err(failure(e)),
        };
        let client = reqwest::Client::builder()
            .user_agent("CataclysmHub/0.1")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(1200))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let host = attempt.url().host_str().unwrap_or("");
                if attempt.previous().len() < 5
                    && attempt.url().scheme() == "https"
                    && [
                        "github.com",
                        "api.github.com",
                        "release-assets.githubusercontent.com",
                        "objects.githubusercontent.com",
                        "codeload.github.com",
                    ]
                    .contains(&host)
                {
                    attempt.follow()
                } else {
                    attempt.error("不接受此下载重定向")
                }
            }))
            .build()
            .map_err(failure)?;
        Ok(Self {
            progress: std::sync::Mutex::new(None),
            cancel: tokio::sync::watch::channel(false).0,
            dir,
            state: Mutex::new(state),
            client,
        })
    }
    pub fn progress(&self) -> Option<Transfer> {
        self.progress.lock().ok().and_then(|p| p.clone())
    }
    pub fn cancel(&self) {
        if self.progress().is_some() {
            self.cancel.send_replace(true);
        }
    }
    fn persist(&self, value: &Catalog) -> Result<(), String> {
        let name = format!("catalog-{}.tmp", uuid::Uuid::new_v4());
        let mut f = self
            .dir
            .open_with(&name, OpenOptions::new().write(true).create_new(true))
            .map_err(failure)?;
        f.write_all(&serde_json::to_vec(value).map_err(failure)?)
            .map_err(failure)?;
        f.sync_all().map_err(failure)?;
        self.dir
            .rename(&name, &self.dir, "remote-catalog.json")
            .map_err(failure)
    }
    pub async fn view(&self) -> View {
        View {
            sources: sources(),
            catalog: self.state.lock().await.clone(),
        }
    }
    pub async fn sync(&self, id: &str, manual: bool) -> Result<View, String> {
        let source = sources()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or("未知来源")?;
        let mut state = self.state.lock().await;
        if !manual && state.listings.get(id).is_some_and(|l| l.attempted) {
            return Ok(View {
                sources: sources(),
                catalog: state.clone(),
            });
        }
        let mut next = state.clone();
        next.listings.entry(id.into()).or_default().attempted = true;
        // Record first attempt before I/O: failed/offline attempts never become background retries.
        self.persist(&next)?;
        *state = next;
        let result = self.fetch(&source).await;
        let mut next = state.clone();
        let listing = next.listings.entry(id.into()).or_default();
        match result {
            Ok(releases) => {
                listing.releases = releases;
                listing.updated = Some(timestamp());
                listing.error = None;
            }
            Err(error) => listing.error = Some(error),
        }
        self.persist(&next)?;
        *state = next;
        Ok(View {
            sources: sources(),
            catalog: state.clone(),
        })
    }
    async fn fetch(&self, source: &Source) -> Result<Vec<Release>, String> {
        if source.id == "bnmods" {
            let response = self
                .client
                .get(format!(
                    "https://api.github.com/repos/{}/commits/master",
                    source.repo
                ))
                .timeout(Duration::from_secs(45))
                .send()
                .await
                .map_err(failure)?
                .error_for_status()
                .map_err(failure)?;
            let bytes = bounded_body(response, 2 * 1024 * 1024).await?;
            let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(failure)?;
            let sha = value["sha"].as_str().ok_or("缺少上游提交")?;
            if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("非法提交摘要".into());
            }
            return Ok(vec![Release {
                tag: sha.into(),
                prerelease: false,
                published: value["commit"]["committer"]["date"]
                    .as_str()
                    .unwrap_or("")
                    .into(),
                assets: vec![Asset {
                    id: 0,
                    name: format!("BN-Kenan-{sha}.zip"),
                    size: 0,
                    url: format!("https://github.com/{}/archive/{sha}.zip", source.repo),
                    digest: None,
                }],
            }]);
        }
        let url = format!(
            "https://api.github.com/repos/{}/releases?per_page=30",
            source.repo
        );
        let mut response = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(45))
            .send()
            .await
            .map_err(failure)?
            .error_for_status()
            .map_err(failure)?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failure)? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err("发布目录超过限制".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let mut releases = parse_releases(&bytes, &source.repo)?;
        if source.kind == "game" {
            let latest = self
                .client
                .get(format!(
                    "https://api.github.com/repos/{}/releases/latest",
                    source.repo
                ))
                .timeout(Duration::from_secs(45))
                .send()
                .await
                .map_err(failure)?;
            if latest.status() != reqwest::StatusCode::NOT_FOUND {
                let body =
                    bounded_body(latest.error_for_status().map_err(failure)?, 8 * 1024 * 1024)
                        .await?;
                let mut array = vec![b'['];
                array.extend(body);
                array.push(b']');
                for release in parse_releases(&array, &source.repo)? {
                    if !releases.iter().any(|r| r.tag == release.tag) {
                        releases.insert(0, release);
                    }
                }
            }
        }
        Ok(releases)
    }
    pub async fn remove_download(&self, id: &str, parent: &Dir) -> Result<View, String> {
        uuid::Uuid::parse_str(id).map_err(failure)?;
        let mut state = self.state.lock().await;
        let entry = state
            .downloads
            .iter()
            .find(|d| d.id == id)
            .ok_or("下载记录不存在")?;
        let source = sources()
            .into_iter()
            .find(|s| s.id == entry.source)
            .ok_or("来源不存在")?;
        // Validate the receipt leaf independently; never trust its display path for deletion.
        let dummy = Asset {
            id: 1,
            name: entry.name.clone(),
            size: 1,
            url: format!("https://github.com/{}/releases/download/x/x", source.repo),
            digest: None,
        };
        validate_asset(&dummy, &source.repo)?;
        match parent.open_dir_nofollow(id) {
            Ok(dir) => match dir.remove_file(&entry.name) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(failure(e)),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(failure(e)),
        }
        // Leave any other files intact (including anything the user extracted there).
        let _ = parent.remove_dir(id);
        let mut next = state.clone();
        next.downloads.retain(|d| d.id != id);
        self.persist(&next)?;
        *state = next;
        Ok(View {
            sources: sources(),
            catalog: state.clone(),
        })
    }
    pub async fn download(
        &self,
        source_id: &str,
        tag: &str,
        asset_id: u64,
        parent: &Dir,
        root: &Path,
    ) -> Result<Download, String> {
        let source = sources()
            .into_iter()
            .find(|s| s.id == source_id)
            .ok_or("未知来源")?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "另一项同步或下载正在进行，请稍后重试")?;
        let asset = state
            .listings
            .get(source_id)
            .and_then(|l| l.releases.iter().find(|r| r.tag == tag))
            .and_then(|r| r.assets.iter().find(|a| a.id == asset_id))
            .cloned()
            .ok_or("版本附件已变化，请重新选择")?;
        validate_asset(&asset, &source.repo)?;
        let id = uuid::Uuid::new_v4().to_string();
        parent.create_dir(&id).map_err(failure)?;
        let dir = parent.open_dir_nofollow(&id).map_err(failure)?;
        self.cancel.send_replace(false);
        let mut cancelled = self.cancel.subscribe();
        *self.progress.lock().map_err(failure)? = Some(Transfer {
            name: asset.name.clone(),
            received: 0,
            total: asset.size,
        });
        let operation = async {
            let mut response = self
                .client
                .get(&asset.url)
                .send()
                .await
                .map_err(failure)?
                .error_for_status()
                .map_err(failure)?;
            let mut file = dir
                .open_with(
                    "download.partial",
                    OpenOptions::new().write(true).create_new(true),
                )
                .map_err(failure)?;
            let mut hash = Sha256::new();
            let mut size = 0_u64;
            while let Some(chunk) = response.chunk().await.map_err(failure)? {
                size += chunk.len() as u64;
                if let Some(progress) = self.progress.lock().map_err(failure)?.as_mut() {
                    progress.received = size;
                }
                if (asset.size > 0 && size > asset.size) || size > 2 * 1024 * 1024 * 1024 {
                    return Err("下载大小与目录记录不符或超过 2 GiB".into());
                }
                file.write_all(&chunk).map_err(failure)?;
                hash.update(&chunk);
            }
            if size == 0 || (asset.size > 0 && size != asset.size) {
                return Err("下载不完整：长度不匹配".into());
            }
            let sha256 = format!("{:x}", hash.finalize());
            verify_digest(&asset, &sha256)?;
            file.sync_all().map_err(failure)?;
            dir.rename("download.partial", &dir, &asset.name)
                .map_err(failure)?;
            let download = Download {
                repo: source.repo.clone(),
                url: asset.url.clone(),
                asset_id: asset.id,
                downloaded_at: timestamp(),
                id,
                source: source_id.into(),
                tag: tag.into(),
                name: asset.name.clone(),
                path: root.join(&asset.name).to_string_lossy().into_owned(),
                size,
                sha256,
                upstream_verified: asset.digest.is_some(),
            };
            Ok(download)
        };
        let result = tokio::select! { result=operation => result, _=cancelled.changed()=>Err("下载已取消".into()) };
        *self.progress.lock().map_err(failure)? = None;
        let mut download = match result {
            Ok(d) => d,
            Err(e) => {
                let _ = dir.remove_file("download.partial");
                return Err(e);
            }
        };
        download.path = root
            .join(&download.id)
            .join(&asset.name)
            .to_string_lossy()
            .into_owned();
        let mut next = state.clone();
        next.downloads.push(download.clone());
        self.persist(&next)?;
        *state = next;
        Ok(download)
    }
}
async fn bounded_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failure)? {
        if bytes.len() + chunk.len() > limit {
            return Err("上游响应超过限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn verify_digest(asset: &Asset, actual: &str) -> Result<(), String> {
    if let Some(expected) = &asset.digest
        && expected != &format!("sha256:{actual}")
    {
        return Err("上游 SHA-256 校验失败".into());
    }
    Ok(())
}
fn validate_asset(a: &Asset, repo: &str) -> Result<(), String> {
    let stem = a
        .name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        return Err("附件名称是系统保留名称".into());
    }

    if a.name.is_empty()
        || a.name == "."
        || a.name == ".."
        || a.name
            .contains(['/', '\\', ':', '\0', '*', '?', '"', '<', '>', '|'])
        || a.name.ends_with(['.', ' '])
        || a.name.len() > 180
        || a.name.chars().any(char::is_control)
    {
        return Err("非法附件名称".into());
    }
    let archive = a.id == 0
        && a.url
            .strip_prefix(&format!("https://github.com/{repo}/archive/"))
            .and_then(|v| v.strip_suffix(".zip"))
            .is_some_and(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()));
    if (a.size == 0 && !archive) || a.size > 2 * 1024 * 1024 * 1024 {
        return Err("附件大小超出支持范围（1 B–2 GiB）".into());
    }
    let url = reqwest::Url::parse(&a.url).map_err(failure)?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || (!archive
            && !url
                .path()
                .starts_with(&format!("/{repo}/releases/download/")))
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("附件不属于所选上游仓库".into());
    }
    if a.digest.as_ref().is_some_and(|d| {
        !d.starts_with("sha256:") || d.len() != 71 || !d[7..].bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        return Err("不支持的上游摘要".into());
    }
    Ok(())
}
fn parse_releases(bytes: &[u8], repo: &str) -> Result<Vec<Release>, String> {
    #[derive(Deserialize)]
    struct Raw {
        tag_name: String,
        prerelease: bool,
        published_at: Option<String>,
        draft: bool,
        assets: Vec<RawAsset>,
    }
    #[derive(Deserialize)]
    struct RawAsset {
        id: u64,
        name: String,
        size: u64,
        browser_download_url: String,
        digest: Option<String>,
    }
    let raw: Vec<Raw> = serde_json::from_slice(bytes).map_err(failure)?;
    Ok(raw
        .into_iter()
        .filter(|r| !r.draft)
        .map(|r| Release {
            tag: r.tag_name,
            prerelease: r.prerelease,
            published: r.published_at.unwrap_or_default(),
            assets: r
                .assets
                .into_iter()
                .map(|a| Asset {
                    id: a.id,
                    name: a.name,
                    size: a.size,
                    url: a.browser_download_url,
                    digest: a.digest,
                })
                .filter(|a| validate_asset(a, repo).is_ok())
                .collect(),
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_foreign_urls_and_paths() {
        let mut a = Asset {
            id: 1,
            name: "x.zip".into(),
            size: 2,
            url: "https://github.com/a/b/releases/download/v/x.zip".into(),
            digest: None,
        };
        assert!(validate_asset(&a, "a/b").is_ok());
        a.name = "../x".into();
        assert!(validate_asset(&a, "a/b").is_err());
        a.name = "NUL.zip".into();
        assert!(validate_asset(&a, "a/b").is_err());
        a.name = "x.zip".into();
        assert!(validate_asset(&a, "other/repo").is_err());
        a.digest = Some("sha256:wrong".into());
        assert!(validate_asset(&a, "a/b").is_err());
    }
    #[test]
    fn rejects_digest_mismatch() {
        let a = Asset {
            id: 1,
            name: "x".into(),
            size: 1,
            url: String::new(),
            digest: Some(format!("sha256:{}", "a".repeat(64))),
        };
        assert!(verify_digest(&a, &"b".repeat(64)).is_err());
        assert!(verify_digest(&a, &"a".repeat(64)).is_ok());
    }
    #[tokio::test]
    async fn attempted_source_never_automatically_retries_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut state = Catalog::default();
        state.listings.insert(
            "dda".into(),
            Listing {
                attempted: true,
                error: Some("offline".into()),
                ..Default::default()
            },
        );
        store.persist(&state).unwrap();
        let reopened = Store::open(dir.path()).unwrap();
        let view = reopened.sync("dda", false).await.unwrap();
        assert_eq!(
            view.catalog.listings["dda"].error.as_deref(),
            Some("offline")
        );
    }
    #[test]
    fn archive_requires_fixed_commit_and_expected_repository() {
        let mut a = Asset {
            id: 0,
            name: "mods.zip".into(),
            size: 0,
            url: format!("https://github.com/a/b/archive/{}.zip", "a".repeat(40)),
            digest: None,
        };
        assert!(validate_asset(&a, "a/b").is_ok());
        a.url = "https://github.com/a/b/archive/master.zip".into();
        assert!(validate_asset(&a, "a/b").is_err());
    }
    #[test]
    fn release_parser_preserves_prerelease_and_excludes_draft_or_foreign_asset() {
        let raw = serde_json::json!([
          {"tag_name":"draft","prerelease":false,"published_at":null,"draft":true,"assets":[]},
          {"tag_name":"experimental","prerelease":true,"published_at":"today","draft":false,"assets":[
            {"id":1,"name":"game.zip","size":2,"browser_download_url":"https://github.com/a/b/releases/download/v/game.zip","digest":null},
            {"id":2,"name":"evil.zip","size":2,"browser_download_url":"https://example.com/file","digest":null}
          ]}
        ]);
        let releases = parse_releases(&serde_json::to_vec(&raw).unwrap(), "a/b").unwrap();
        assert_eq!(releases.len(), 1);
        assert!(releases[0].prerelease);
        assert_eq!(releases[0].assets.len(), 1);
    }
    #[tokio::test]
    async fn removing_download_preserves_extracted_files_and_does_not_follow_display_path() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let parent = Dir::open_ambient_dir(temp.path(), cap_std::ambient_authority()).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        parent.create_dir(&id).unwrap();
        let dir = parent.open_dir(&id).unwrap();
        dir.write("package.zip", b"data").unwrap();
        dir.write("extracted.txt", b"keep").unwrap();
        parent.write("outside.zip", b"keep").unwrap();
        store.state.lock().await.downloads.push(Download {
            repo: String::new(),
            url: String::new(),
            asset_id: 0,
            downloaded_at: String::new(),
            id: id.clone(),
            source: "dda".into(),
            tag: "x".into(),
            name: "package.zip".into(),
            path: temp.path().join("outside.zip").to_string_lossy().into(),
            size: 4,
            sha256: String::new(),
            upstream_verified: false,
        });
        let view = store.remove_download(&id, &parent).await.unwrap();
        assert!(view.catalog.downloads.is_empty());
        assert!(!dir.exists("package.zip"));
        assert_eq!(dir.read("extracted.txt").unwrap(), b"keep");
        assert_eq!(parent.read("outside.zip").unwrap(), b"keep");
    }
    #[tokio::test]
    #[ignore = "explicit network smoke against public upstreams; uses temporary directories"]
    async fn live_upstream_catalog_and_small_download() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let mut small = None;
        for source in sources() {
            let view = store.sync(&source.id, true).await.unwrap();
            let listing = &view.catalog.listings[&source.id];
            assert!(
                listing.error.is_none(),
                "{}: {:?}",
                source.id,
                listing.error
            );
            assert!(
                !listing.releases.is_empty(),
                "{} returned no releases",
                source.id
            );
            let count: usize = listing.releases.iter().map(|r| r.assets.len()).sum();
            assert!(count > 0, "{} has no accepted assets", source.id);
            println!(
                "{}: {} releases, {} assets",
                source.id,
                listing.releases.len(),
                count
            );
            if small.is_none() {
                for r in &listing.releases {
                    if let Some(a) = r.assets.iter().find(|a| a.size > 0 && a.size < 20_000_000) {
                        small = Some((source.id.clone(), r.tag.clone(), a.id));
                        break;
                    }
                }
            }
        }
        if let Some((source, tag, id)) = small {
            let folder = tempfile::tempdir().unwrap();
            let dir = Dir::open_ambient_dir(folder.path(), cap_std::ambient_authority()).unwrap();
            let d = store
                .download(&source, &tag, id, &dir, folder.path())
                .await
                .unwrap();
            assert_eq!(std::fs::metadata(&d.path).unwrap().len(), d.size);
            println!(
                "small download: {} {} bytes; upstream digest checked={}",
                d.name, d.size, d.upstream_verified
            );
            store.remove_download(&d.id, &dir).await.unwrap();
            assert!(!Path::new(&d.path).exists());
        } else {
            panic!("No bounded download fixture found");
        }
    }
}
