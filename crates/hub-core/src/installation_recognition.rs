//! Recognition from recorded binary samples, without executing the selected program.
use crate::{Hub, HubError, Result, paths};
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
pub struct InstallationRecognition {
    pub installation_id: String,
    pub executable_sha256: String,
    pub version: Option<String>,
    pub recommended_profile: Option<String>,
    pub evidence: String,
}
// Provenance: docs/research/cdda-real-game-acceptance-contract.md, sample outputs table.
fn sample_version(hash: &str) -> Option<&'static str> {
    match hash {
        "ba92c296d23758eab4fe4a74ee5ee624f01edaac051c3777c3c138cac45de21d" => Some("0.H-RELEASE"),
        "3f37ff041654edbb88773666344be0bf8eba67cb44d71c15908aeb961d4aac3a" => Some("0.I-1"),
        _ => None,
    }
}
/// Describe the imported binary record, without claiming a fresh disk inspection.
pub(crate) fn describe_registered(installation: &mut crate::Installation) {
    if installation.version.is_none()
        && installation.branch.is_none()
        && let Some(version) = sample_version(&installation.sha256)
    {
        installation.version = Some(version.into());
        installation.branch = Some("DDA".into());
    }
}
fn recommend(version: Option<&str>, executable: &Path, macos: bool) -> Option<String> {
    if !macos {
        return None;
    }
    let parent = executable.parent()?;
    let contents = parent.parent()?;
    if contents.file_name()?.to_str()? != "Contents"
        || !contents.parent()?.extension()?.eq_ignore_ascii_case("app")
    {
        return None;
    }
    let suffix = match parent.file_name()?.to_str()? {
        "MacOS" => "native-trial-v1",
        "Resources" => "trial-v2",
        _ => return None,
    };
    let prefix = match version {
        Some("0.H-RELEASE") => "cdda-0h-release-macos",
        Some("0.I-1") => "cdda-0i-1-macos",
        _ => return None,
    };
    Some(format!("{prefix}-{suffix}"))
}
impl Hub {
    pub async fn inspect_installation_recognition(
        &self,
        id: String,
    ) -> Result<InstallationRecognition> {
        let installation = self.installation(&id).await?;
        let hash = paths::digest(PathBuf::from(&installation.executable)).await?;
        if hash != installation.sha256 {
            return Err(HubError::new(
                "SOURCE_CHANGED",
                "游戏程序已变化，请核对安装记录后再配置启动。",
            ));
        }
        let version = sample_version(&hash);
        Ok(InstallationRecognition {
            installation_id: id,
            executable_sha256: hash.clone(),
            version: version.map(str::to_owned),
            recommended_profile: recommend(
                version,
                Path::new(&installation.executable),
                cfg!(target_os = "macos"),
            ),
            evidence: if version.is_some() {
                "程序内容匹配已记录的 CDDA 发布样本；不代表目录绑定或模组兼容已验收。"
            } else {
                "程序尚未匹配已记录样本，请核对游戏版本后手动选择。"
            }
            .into(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_sample_hashes_identify_versions() {
        assert_eq!(
            sample_version("3f37ff041654edbb88773666344be0bf8eba67cb44d71c15908aeb961d4aac3a"),
            Some("0.I-1")
        );
        assert_eq!(sample_version("0.I-1"), None);
    }
    #[test]
    fn recommendation_requires_known_version_platform_and_bundle_layout() {
        assert_eq!(
            recommend(
                Some("0.I-1"),
                Path::new("/Game.app/Contents/MacOS/game"),
                true
            )
            .as_deref(),
            Some("cdda-0i-1-macos-native-trial-v1")
        );
        assert_eq!(
            recommend(
                Some("0.H-RELEASE"),
                Path::new("/Game.app/Contents/Resources/game"),
                true
            )
            .as_deref(),
            Some("cdda-0h-release-macos-trial-v2")
        );
        assert!(
            recommend(
                Some("0.I-1"),
                Path::new("/Game.app/Contents/MacOS/game"),
                false
            )
            .is_none()
        );
        assert!(recommend(None, Path::new("/Game.app/Contents/MacOS/game"), true).is_none());
        assert!(recommend(Some("0.I-1"), Path::new("/fake/Contents/MacOS/game"), true).is_none());
    }
}
