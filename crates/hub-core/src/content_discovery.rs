//! Bounded observations of candidate CDDA resource roots; no loader precedence or trust promotion.
use crate::{ContentConfigReport, ContentDefinition, backup_format as format, paths};
use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentScope {
    pub path: String,
    pub kind: String,
    pub basis: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct DiscoveredContent {
    pub path: String,
    pub kind: String,
    pub metadata_sha256: String,
    pub definitions: Vec<ContentDefinition>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentFinding {
    pub code: String,
    pub context: String,
    pub content_id: String,
    pub related_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentDiscovery {
    pub scopes: Vec<ContentScope>,
    pub packages: Vec<DiscoveredContent>,
    pub findings: Vec<ContentFinding>,
    pub truncated: bool,
}
impl ContentDiscovery {
    fn finding(&mut self, code: &str, context: &str, id: &str, related: Option<&str>) {
        if self.findings.len() >= 2048 {
            self.truncated = true;
            return;
        }
        self.findings.push(ContentFinding {
            code: code.into(),
            context: context.into(),
            content_id: id.into(),
            related_id: related.map(str::to_owned),
        });
    }
}
struct Budget {
    entries: usize,
    bytes: usize,
    definitions: usize,
}
fn metadata(
    dir: &Dir,
    kind: &str,
    budget: &mut Budget,
) -> std::result::Result<Option<(Vec<ContentDefinition>, String)>, &'static str> {
    let name = if kind == "mod" {
        "modinfo.json"
    } else {
        "soundpack.txt"
    };
    let m = match dir.symlink_metadata(name) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("unreadable"),
    };
    if !m.is_file() || m.file_type().is_symlink() {
        return Err("unsafe_metadata");
    }
    let input = format::open_file(dir, name).map_err(|_| "unreadable")?;
    let limit = budget.bytes.min(1024 * 1024);
    let mut bytes = Vec::new();
    input
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable")?;
    budget.bytes = budget.bytes.saturating_sub(bytes.len());
    if bytes.len() > limit {
        return Err("limit_exceeded");
    }
    let definitions =
        crate::content::parse_metadata(kind, &bytes).map_err(|_| "invalid_metadata")?;
    if definitions.len() > budget.definitions {
        return Err("limit_exceeded");
    }
    budget.definitions -= definitions.len();
    Ok(Some((definitions, format::sha(&bytes))))
}
fn walk(
    dir: &Dir,
    path: &Path,
    kind: &str,
    depth: usize,
    budget: &mut Budget,
    out: &mut ContentDiscovery,
) -> std::result::Result<(), &'static str> {
    if depth > 8 {
        return Err("limit_exceeded");
    }
    let text = paths::text(path).map_err(|_| "unsupported_path")?;
    match metadata(dir, kind, budget) {
        Ok(Some((definitions, hash))) => out.packages.push(DiscoveredContent {
            path: text.clone(),
            kind: kind.into(),
            metadata_sha256: hash,
            definitions,
        }),
        Ok(None) => (),
        Err(code) => {
            out.finding(code, &text, "", None);
            return Err(code);
        }
    }
    let mut names = Vec::new();
    for item in dir.entries().map_err(|_| "unreadable")? {
        if budget.entries == 0 {
            return Err("limit_exceeded");
        }
        budget.entries -= 1;
        let item = item.map_err(|_| "unreadable")?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| "unsupported_path")?;
        format::validate_path(&name).map_err(|_| "unsupported_path")?;
        names.push(name);
    }
    names.sort();
    let mut status = Ok(());
    for name in names {
        let m = dir.symlink_metadata(&name).map_err(|_| "unreadable")?;
        if m.file_type().is_symlink() {
            out.finding("unsafe_path", &format!("{text}/{name}"), "", None);
            status = Err("partial");
        } else if m.is_dir() {
            let child = dir.open_dir_nofollow(&name).map_err(|_| "unreadable")?;
            if let Err(code) = walk(&child, &path.join(&name), kind, depth + 1, budget, out) {
                out.finding(code, &format!("{text}/{name}"), "", None);
                status = Err("partial");
                if code == "limit_exceeded" {
                    out.truncated = true;
                    break;
                }
            }
        } else if !m.is_file() {
            out.finding("unsafe_path", &format!("{text}/{name}"), "", None);
            status = Err("partial");
        }
    }
    status
}
fn open_root(base: &Path, relative: &[&str]) -> std::result::Result<Dir, &'static str> {
    if std::fs::canonicalize(base).map_err(|_| "unavailable_base")? != base {
        return Err("unsafe_path");
    }
    let mut dir = Dir::open_ambient_dir(base, cap_std::ambient_authority())
        .map_err(|_| "unavailable_base")?;
    for name in relative {
        let m = dir.symlink_metadata(name).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "missing"
            } else {
                "unreadable"
            }
        })?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err("unsafe_path");
        }
        dir = dir.open_dir_nofollow(name).map_err(|_| "unreadable")?;
    }
    Ok(dir)
}
pub(crate) fn discover(instance: &Path, working: &str) -> ContentDiscovery {
    let work = PathBuf::from(working);
    // Match the native entry layout already used by Hub. These remain candidate
    // default paths: custom datadir/base-path/build prefixes are not inferred.
    let installation_base = if work.file_name().is_some_and(|n| n == "MacOS")
        && work
            .parent()
            .is_some_and(|p| p.file_name().is_some_and(|n| n == "Contents"))
        && paths::bundle_root(&work).is_some()
    {
        work.parent().unwrap().join("Resources")
    } else {
        work
    };
    let mut out = ContentDiscovery {
        scopes: vec![],
        packages: vec![],
        findings: vec![],
        truncated: false,
    };
    let mut budget = Budget {
        entries: 20_000,
        bytes: 16 * 1024 * 1024,
        definitions: 2048,
    };
    for (base, relative, kind, basis) in [
        (instance, vec!["mods"], "mod", "instance_userdir"),
        (instance, vec!["sound"], "soundpack", "instance_userdir"),
        (
            installation_base.as_path(),
            vec!["data", "mods"],
            "mod",
            "installation_default",
        ),
        (
            installation_base.as_path(),
            vec!["data", "sound"],
            "soundpack",
            "installation_default",
        ),
    ] {
        let path = relative.iter().fold(base.to_path_buf(), |p, n| p.join(n));
        let status = match open_root(base, &relative) {
            Ok(dir) => {
                let first_package = out.packages.len();
                let result = walk(&dir, &path, kind, 0, &mut budget, &mut out);
                let unchanged = dir
                    .try_clone()
                    .ok()
                    .and_then(|d| same_file::Handle::from_file(d.into_std_file()).ok())
                    .zip(same_file::Handle::from_path(&path).ok())
                    .is_some_and(|(a, b)| a == b)
                    && std::fs::canonicalize(&path).is_ok_and(|p| p == path);
                if !unchanged {
                    out.packages.truncate(first_package);
                    "changed"
                } else {
                    result.err().unwrap_or("scanned")
                }
            }
            Err(code) => code,
        };
        if status == "limit_exceeded" {
            out.truncated = true;
        }
        out.scopes.push(ContentScope {
            path: path.to_string_lossy().into(),
            kind: kind.into(),
            basis: basis.into(),
            status: status.into(),
        });
    }
    out.packages
        .sort_by(|a, b| (&a.kind, &a.path).cmp(&(&b.kind, &b.path)));
    out
}
pub(crate) fn diagnose(report: &ContentConfigReport, out: &mut ContentDiscovery) {
    // Clone the bounded index so findings can be appended without mutating facts.
    let mut index: BTreeMap<(String, String), Vec<(String, ContentDefinition)>> = BTreeMap::new();
    for package in &out.packages {
        for definition in &package.definitions {
            index
                .entry((package.kind.clone(), definition.content_id.clone()))
                .or_default()
                .push((package.path.clone(), definition.clone()));
        }
    }
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ((kind, id), definitions) in &index {
        if definitions.len() > 1 {
            out.finding("duplicate_id", kind, id, None);
            continue;
        }
        if kind != "mod" {
            continue;
        }
        let (path, definition) = &definitions[0];
        let Some(dependencies) = &definition.dependencies else {
            out.finding("dependencies_unknown", path, id, None);
            continue;
        };
        graph.insert(id.clone(), dependencies.clone());
        for dep in dependencies {
            match index.get(&("mod".into(), dep.clone())).map(Vec::len) {
                None => out.finding("dependency_not_observed", path, id, Some(dep)),
                Some(n) if n > 1 => out.finding("ambiguous_dependency", path, id, Some(dep)),
                _ => (),
            }
        }
    }
    // Iterative three-color traversal: bounded work, no recursive dependency stack.
    let mut color: BTreeMap<&str, u8> = BTreeMap::new();
    for start in graph.keys() {
        if color.contains_key(start.as_str()) {
            continue;
        }
        color.insert(start, 1);
        let mut stack = vec![(start.as_str(), 0usize)];
        while let Some((node, next_index)) = stack.last_mut() {
            let dependencies = &graph[*node];
            if *next_index == dependencies.len() {
                color.insert(node, 2);
                stack.pop();
                continue;
            }
            let dep = dependencies[*next_index].as_str();
            *next_index += 1;
            if !graph.contains_key(dep) {
                continue;
            }
            match color.get(dep).copied().unwrap_or(0) {
                0 => {
                    color.insert(dep, 1);
                    stack.push((dep, 0));
                }
                1 => out.finding("dependency_cycle", "candidate_mods", node, Some(dep)),
                _ => (),
            }
        }
    }
    for reference in &report.references {
        if !index.contains_key(&(reference.kind.clone(), reference.content_id.clone())) {
            for path in &reference.referenced_by {
                out.finding("reference_not_observed", path, &reference.content_id, None);
            }
        }
    }
    for file in std::iter::once(&report.default_mods)
        .chain(report.worlds.iter())
        .filter(|f| f.status == "read")
    {
        for id in &file.mod_ids {
            if file.path == "config/user-default-mods.json"
                && index
                    .get(&("mod".into(), id.clone()))
                    .is_some_and(|defs| defs.iter().any(|(_, d)| d.obsolete))
            {
                out.finding("obsolete_default_mod", &file.path, id, None);
            }
            if let Some(definitions) = index.get(&("mod".into(), id.clone()))
                && definitions.len() == 1
                && let Some(deps) = &definitions[0].1.dependencies
            {
                for dep in deps {
                    if !file.mod_ids.contains(dep) {
                        out.finding("dependency_not_listed", &file.path, id, Some(dep));
                    }
                }
            }
        }
    }
}
