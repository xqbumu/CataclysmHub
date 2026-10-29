//! Bounded extraction into a fresh capability directory; never runs archive contents.
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, OpenOptions};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek},
    path::{Component, Path},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const MAX_FILES: usize = 100_000;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn components(name: &str) -> Result<Vec<String>, String> {
    if name.contains(['\\', ':', '\0']) || name.len() > 2048 {
        return Err("归档路径不受支持".into());
    }
    let mut result = vec![];
    for part in Path::new(name).components() {
        match part {
            Component::CurDir => (),
            Component::Normal(s) => {
                let s = s.to_str().ok_or("归档路径编码无效")?;
                let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
                if s.ends_with(['.', ' '])
                    || s.chars().any(char::is_control)
                    || s.contains(['*', '?', '"', '<', '>', '|'])
                    || [
                        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                        "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
                        "LPT7", "LPT8", "LPT9",
                    ]
                    .contains(&stem.as_str())
                {
                    return Err("归档包含系统保留路径".into());
                }
                result.push(s.to_owned());
            }
            _ => return Err("拒绝绝对路径或目录穿越".into()),
        }
    }
    if result.len() > 32 {
        return Err("归档目录过深".into());
    }
    Ok(result)
}
struct Writer<'a> {
    root: &'a Dir,
    names: HashSet<String>,
    path_case: HashMap<String, String>,
    bytes: u64,
    count: usize,
}
impl Writer<'_> {
    fn entry(
        &mut self,
        name: &str,
        directory: bool,
        size: u64,
        mode: u32,
        modified: Option<SystemTime>,
        reader: &mut impl Read,
    ) -> Result<(), String> {
        self.count += 1;
        if directory && size != 0 {
            return Err("目录条目不能携带文件数据".into());
        }
        if self.count > MAX_FILES || size > MAX_FILE || self.bytes.saturating_add(size) > MAX_BYTES
        {
            return Err("归档超过文件数或解压大小限制".into());
        }
        let parts = components(name)?;
        if parts.is_empty() {
            return if directory {
                Ok(())
            } else {
                Err("空文件路径".into())
            };
        }
        for end in 1..=parts.len() {
            let path = parts[..end].join("/");
            let lower = path.to_lowercase();
            if self
                .path_case
                .get(&lower)
                .is_some_and(|existing| existing != &path)
            {
                return Err("归档包含大小写冲突目录".into());
            }
            self.path_case.insert(lower, path);
        }
        let key = parts.join("/").to_lowercase();
        if !self.names.insert(key) {
            return Err("归档包含重复或大小写冲突路径".into());
        }
        let mut parent = self.root.try_clone().map_err(err)?;
        for part in &parts[..parts.len() - 1] {
            match parent.create_dir(part) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(err(e)),
            }
            parent = parent.open_dir_nofollow(part).map_err(err)?;
        }
        let leaf = parts.last().unwrap();
        if directory {
            match parent.create_dir(leaf) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    parent.open_dir_nofollow(leaf).map_err(err)?;
                }
                Err(e) => return Err(err(e)),
            }
        } else {
            let mut f = parent
                .open_with(leaf, OpenOptions::new().write(true).create_new(true))
                .map_err(err)?
                .into_std();
            let copied = std::io::copy(&mut reader.take(size + 1), &mut f).map_err(err)?;
            if copied != size {
                return Err("归档文件长度不符".into());
            }
            self.bytes += copied;
            if let Some(time) = modified {
                f.set_times(std::fs::FileTimes::new().set_modified(time))
                    .map_err(err)?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                f.set_permissions(std::fs::Permissions::from_mode(if mode & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                }))
                .map_err(err)?;
            }
            #[cfg(not(unix))]
            let _ = mode;
            f.sync_all().map_err(err)?;
        }
        Ok(())
    }
}
pub fn supported(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".zip") || n.ends_with(".tar.gz") || n.ends_with(".tgz") || n.ends_with(".tar")
}
pub fn extract(file: std::fs::File, name: &str, target: &Dir) -> Result<(usize, u64), String> {
    if !supported(name) {
        return Err("内置解压支持 ZIP、TAR、TAR.GZ；DMG、7z 等请使用系统工具处理".into());
    }
    let mut writer = Writer {
        root: target,
        names: HashSet::new(),
        path_case: HashMap::new(),
        bytes: 0,
        count: 0,
    };
    let name = name.to_lowercase();
    if name.ends_with(".zip") {
        unzip(file, &mut writer)?;
    } else if name.ends_with(".tar") {
        untar(file, &mut writer)?;
    } else {
        untar(flate2::read::GzDecoder::new(file), &mut writer)?;
    }
    if writer.count == 0 {
        return Err("归档为空".into());
    }
    Ok((writer.count, writer.bytes))
}
// Reject oversized/ZIP64 directories before the ZIP library allocates its index.
fn zip_preflight(file: &mut (impl Read + Seek)) -> Result<(), String> {
    use std::io::SeekFrom;
    let length = file.seek(SeekFrom::End(0)).map_err(err)?;
    let tail_len = length.min(65557) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64))).map_err(err)?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail).map_err(err)?;
    let offset = (0..tail_len.saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..].starts_with(b"PK\x05\x06")
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail_len
        })
        .ok_or("ZIP 结束记录无效")?;
    if (offset >= 20 && &tail[offset - 20..offset - 16] == b"PK\x06\x07")
        || tail[offset + 4..].windows(4).any(|w| w == b"PK\x05\x06")
    {
        return Err("ZIP64 或歧义结束记录不受支持".into());
    }
    let h = &tail[offset..];
    let u16_at = |n| u16::from_le_bytes([h[n], h[n + 1]]);
    let u32_at = |n| u32::from_le_bytes([h[n], h[n + 1], h[n + 2], h[n + 3]]) as u64;
    let count = u16_at(10) as usize;
    let size = u32_at(12);
    let start = u32_at(16);
    if u16_at(4) != 0
        || u16_at(6) != 0
        || u16_at(8) as usize != count
        || count == 65535
        || size == u32::MAX as u64
        || start == u32::MAX as u64
    {
        return Err("内置解压不支持 ZIP64 或分卷 ZIP，请使用系统工具".into());
    }
    if count > MAX_FILES
        || size > 32 * 1024 * 1024
        || start.checked_add(size) != Some(length - tail_len as u64 + offset as u64)
    {
        return Err("ZIP 中央目录超出预算或布局不支持".into());
    }
    file.seek(SeekFrom::Start(start)).map_err(err)?;
    let mut used = 0u64;
    for _ in 0..count {
        if used + 46 > size {
            return Err("ZIP 中央目录长度不符".into());
        }
        let mut header = [0; 46];
        file.read_exact(&mut header).map_err(err)?;
        if &header[..4] != b"PK\x01\x02" {
            return Err("ZIP 中央目录条目无效".into());
        }
        let field = |n| u16::from_le_bytes([header[n], header[n + 1]]) as u64;
        if field(28) > 2048 {
            return Err("ZIP 路径过长".into());
        }
        let rest = field(28) + field(30) + field(32);
        used += 46 + rest;
        if used > size {
            return Err("ZIP 中央目录长度不符".into());
        }
        file.seek(SeekFrom::Current(rest as i64)).map_err(err)?;
    }
    if used != size {
        return Err("ZIP 中央目录条目数量不符".into());
    }
    file.seek(SeekFrom::Start(0)).map_err(err)?;
    Ok(())
}
fn unzip(mut file: impl Read + Seek, writer: &mut Writer<'_>) -> Result<(), String> {
    zip_preflight(&mut file)?;
    let mut zip = zip::ZipArchive::with_config(
        zip::read::Config {
            archive_offset: zip::read::ArchiveOffset::Known(0),
        },
        file,
    )
    .map_err(err)?;
    if zip.len() > MAX_FILES {
        return Err("归档文件数量超过限制".into());
    }
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(err)?;
        let mode = f.unix_mode().unwrap_or(0o644);
        let kind = mode & 0o170000;
        if kind != 0 && kind != 0o100000 && kind != 0o040000 {
            return Err("内置解压拒绝链接和特殊文件，请使用系统工具核对原包".into());
        }
        let modified = f
            .last_modified()
            .and_then(|d| {
                chrono::NaiveDate::from_ymd_opt(d.year().into(), d.month().into(), d.day().into())?
                    .and_hms_opt(d.hour().into(), d.minute().into(), d.second().into())
            })
            .and_then(|t| u64::try_from(t.and_utc().timestamp()).ok())
            .map(|t| UNIX_EPOCH + Duration::from_secs(t));
        let name = f.name().map_err(err)?.into_owned();
        let directory = f.is_dir();
        let size = f.size();
        writer.entry(&name, directory, size, mode, modified, &mut f)?;
    }
    Ok(())
}
fn untar(reader: impl Read, writer: &mut Writer<'_>) -> Result<(), String> {
    let mut tar = tar::Archive::new(reader);
    for entry in tar.entries().map_err(err)?.raw(true) {
        let mut f = entry.map_err(err)?;
        let kind = f.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err("内置解压拒绝链接和特殊文件，请使用系统工具核对原包".into());
        }
        let name = f
            .path()
            .map_err(err)?
            .to_str()
            .ok_or("归档路径编码无效")?
            .to_owned();
        let size = f.size();
        let mode = f.header().mode().map_err(err)?;
        let modified = f
            .header()
            .mtime()
            .ok()
            .and_then(|t| UNIX_EPOCH.checked_add(Duration::from_secs(t)));
        writer.entry(&name, kind.is_dir(), size, mode, modified, &mut f)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn target(temp: &tempfile::TempDir) -> Dir {
        Dir::open_ambient_dir(temp.path(), cap_std::ambient_authority()).unwrap()
    }
    fn zip_file(temp: &tempfile::TempDir, entries: &[(&str, &[u8])]) -> std::fs::File {
        let path = temp.path().join("fixture.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        std::fs::File::open(path).unwrap()
    }
    #[test]
    fn zip_extracts_files_and_rejects_traversal_duplicates_and_case_collisions() {
        for entries in [
            vec![("mods/a/modinfo.json", &b"[]"[..])],
            vec![("../escape", &b"x"[..])],
            vec![("A", &b"x"[..]), ("a", &b"y"[..])],
        ] {
            let input = tempfile::tempdir().unwrap();
            let output = tempfile::tempdir().unwrap();
            let file = zip_file(&input, &entries);
            let result = extract(file, "test.zip", &target(&output));
            if entries.len() == 1 && entries[0].0.starts_with("mods/") {
                assert_eq!(result.unwrap(), (1, 2));
                assert_eq!(
                    std::fs::read(output.path().join(entries[0].0)).unwrap(),
                    b"[]"
                );
            } else {
                assert!(result.is_err());
            }
            assert!(!output.path().parent().unwrap().join("escape").exists());
        }
    }
    #[test]
    fn rejects_portability_hazards_and_limits_before_writing() {
        for path in [
            "/absolute",
            "../x",
            "x/../../y",
            "C:/a",
            "a\\b",
            "a/NUL.txt",
            "a/trailing.",
        ] {
            assert!(components(path).is_err(), "{path}");
        }
        let output = tempfile::tempdir().unwrap();
        let root = target(&output);
        let mut writer = Writer {
            root: &root,
            names: HashSet::new(),
            path_case: HashMap::new(),
            bytes: 0,
            count: 0,
        };
        assert!(
            writer
                .entry("huge", false, MAX_FILE + 1, 0, None, &mut &b""[..])
                .is_err()
        );
        assert!(!output.path().join("huge").exists());
        assert!(
            writer
                .entry("directory", true, 1, 0, None, &mut &b"x"[..])
                .is_err()
        );
        assert!(!output.path().join("directory").exists());
    }
    #[test]
    fn tar_preserves_file_time_and_executable_bit_and_refuses_links() {
        for link in [false, true] {
            let input = tempfile::tempdir().unwrap();
            let path = input.path().join("test.tar");
            let mut tar = tar::Builder::new(std::fs::File::create(&path).unwrap());
            let mut header = tar::Header::new_gnu();
            header.set_path("game").unwrap();
            header.set_mode(0o755);
            header.set_mtime(1234567);
            if link {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("/outside").unwrap();
                header.set_size(0);
            } else {
                header.set_size(3);
            }
            header.set_cksum();
            tar.append(&header, if link { &b""[..] } else { &b"abc"[..] })
                .unwrap();
            tar.finish().unwrap();
            let output = tempfile::tempdir().unwrap();
            let result = extract(
                std::fs::File::open(path).unwrap(),
                "game.tar",
                &target(&output),
            );
            if link {
                assert!(result.is_err());
                assert!(!output.path().join("game").exists());
            } else {
                assert_eq!(result.unwrap(), (1, 3));
                let m = std::fs::metadata(output.path().join("game")).unwrap();
                assert_eq!(
                    m.modified()
                        .unwrap()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    1234567
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(m.permissions().mode() & 0o777, 0o755);
                }
            }
        }
    }
    #[test]
    fn zip_index_budget_is_checked_before_library_construction() {
        use std::io::Cursor;
        for (count, size) in [(65535u16, 0u32), (1, 33 * 1024 * 1024)] {
            let mut bytes = vec![0u8; 22];
            bytes[..4].copy_from_slice(b"PK\x05\x06");
            bytes[8..10].copy_from_slice(&count.to_le_bytes());
            bytes[10..12].copy_from_slice(&count.to_le_bytes());
            bytes[12..16].copy_from_slice(&size.to_le_bytes());
            assert!(zip_preflight(&mut Cursor::new(bytes)).is_err());
        }
        let temp = tempfile::tempdir().unwrap();
        let mut valid = zip_file(&temp, &[("file", b"data")]);
        assert!(zip_preflight(&mut valid).is_ok());
    }
}
