//! Explicit experimental admission, separate from verified game support.
use crate::{HubError, Result, paths};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) const H: &str = "cdda-0h-macos-concurrency-candidate-v1";
pub(crate) const I: &str = "cdda-0i-1-macos-concurrency-candidate-v1";

pub(crate) fn base_profile(profile: &str) -> &str {
    match profile {
        H => "cdda-0h-release-macos-native-trial-v1",
        I => "cdda-0i-1-macos-native-trial-v1",
        #[cfg(test)]
        "synthetic-candidate" => "cdda-0h-source-trial-v1",
        _ => profile,
    }
}

pub(crate) struct CandidateBinding {
    pub roots: [String; 2],
    handles: [same_file::Handle; 2],
    fingerprint: String,
}
impl CandidateBinding {
    pub fn paths_unchanged(&self) -> bool {
        self.roots.iter().zip(&self.handles).all(|(path, handle)| {
            std::fs::canonicalize(path).is_ok_and(|p| p == Path::new(path))
                && same_file::Handle::from_path(path).is_ok_and(|current| current == *handle)
        })
    }
    pub fn matches(&self, other: &Self) -> bool {
        self.roots == other.roots
            && self.handles == other.handles
            && self.fingerprint == other.fingerprint
    }
}
fn unsupported() -> HubError {
    HubError::new(
        "CONCURRENCY_CANDIDATE_UNSUPPORTED",
        "此构建或游戏组合不属于并发实验候选；请使用普通试运行方案。",
    )
}

pub(crate) fn ensure_enabled(profile: &str) -> Result<()> {
    if base_profile(profile) == profile {
        return Ok(());
    }
    #[cfg(test)]
    if profile == "synthetic-candidate" {
        return Ok(());
    }
    if cfg!(feature = "candidate-concurrency")
        && cfg!(all(target_os = "macos", target_arch = "aarch64"))
    {
        Ok(())
    } else {
        Err(unsupported())
    }
}

pub(crate) async fn prepare(
    profile: &str,
    executable: &str,
    working: &str,
    instance: &str,
) -> Result<Option<Arc<CandidateBinding>>> {
    if base_profile(profile) == profile {
        return Ok(None);
    }
    #[cfg(test)]
    let synthetic = profile == "synthetic-candidate";
    #[cfg(not(test))]
    let synthetic = false;
    ensure_enabled(profile)?;
    let expected = match profile {
        H => "033a9cdbd5ce0fe2ff040c5fa9c96360b4bb620c796316751db67ac355741b67",
        I => "b09651201f62aeede5798347ec2c7e7a3dd55ebd6935033afda581a7c4453982",
        #[cfg(test)]
        "synthetic-candidate" => "",
        _ => return Err(unsupported()),
    };
    let root = paths::installation_scope(working);
    if !synthetic && Path::new(executable) != root.join("Contents/MacOS/cataclysm-hub-qa") {
        return Err(unsupported());
    }
    let roots = [paths::text(&root)?, instance.to_owned()];
    tokio::task::spawn_blocking(move || {
        let fingerprint = static_fingerprint(&root)?;
        if !synthetic && fingerprint != expected {
            return Err(unsupported());
        }
        let handles = [
            same_file::Handle::from_path(&roots[0]).map_err(|_| unsupported())?,
            same_file::Handle::from_path(&roots[1]).map_err(|_| unsupported())?,
        ];
        let binding = CandidateBinding {
            roots,
            handles,
            fingerprint,
        };
        if !binding.paths_unchanged() {
            return Err(unsupported());
        }
        Ok(Some(Arc::new(binding)))
    })
    .await
    .map_err(|_| unsupported())?
}

// Only these two known generated-cache trees are mutable in the package.
// Include paths, types, link targets and file contents; ignore timestamps so a
// cache rebuild or a byte-identical copy does not invalidate static identity.
fn static_fingerprint(path: &Path) -> Result<String> {
    use cap_fs_ext::DirExt;
    let root = cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())
        .map_err(|_| unsupported())?;
    let mut pending = vec![(PathBuf::new(), 0)];
    let mut records = Vec::new();
    let mut count = 0;
    let mut remaining = 2_u64 * 1024 * 1024 * 1024;
    while let Some((prefix, depth)) = pending.pop() {
        let mut dir = root.try_clone().map_err(|_| unsupported())?;
        for part in prefix.components() {
            dir = dir
                .open_dir_nofollow(part.as_os_str())
                .map_err(|_| unsupported())?;
        }
        for item in dir.entries().map_err(|_| unsupported())? {
            count += 1;
            if count > 20_000 || depth > 32 {
                return Err(unsupported());
            }
            let name = item.map_err(|_| unsupported())?.file_name();
            let relative = prefix.join(&name);
            let text = paths::text(&relative)?;
            let meta = dir.symlink_metadata(&name).map_err(|_| unsupported())?;
            if matches!(
                text.as_str(),
                "Contents/Resources/cache" | "Contents/Resources/data/cache"
            ) {
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    return Err(unsupported());
                }
                continue;
            }
            let (kind, value) = if meta.file_type().is_symlink() {
                (
                    "link",
                    paths::text(&dir.read_link_contents(&name).map_err(|_| unsupported())?)?,
                )
            } else if meta.is_dir() {
                pending.push((relative, depth + 1));
                ("dir", String::new())
            } else if meta.is_file() {
                let mut file = crate::backup_format::open_file(&root, &text)?;
                let mut digest = Sha256::new();
                let mut buffer = [0_u8; 65536];
                loop {
                    let size = file.read(&mut buffer).map_err(|_| unsupported())?;
                    if size == 0 {
                        break;
                    }
                    remaining = remaining.checked_sub(size as u64).ok_or_else(unsupported)?;
                    digest.update(&buffer[..size]);
                }
                ("file", format!("{:x}", digest.finalize()))
            } else {
                return Err(unsupported());
            };
            records.push((text, kind, value));
        }
    }
    records.sort();
    let mut digest = Sha256::new();
    for (path, kind, value) in records {
        for field in [path.as_bytes(), kind.as_bytes(), value.as_bytes()] {
            digest.update(field);
            digest.update([0]);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
#[path = "candidate_launch_tests.rs"]
mod tests;
