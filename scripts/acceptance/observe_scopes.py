#!/usr/bin/env python3
"""Read-only snapshots of declared test scopes. Never executes or approves a game."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import stat
import sys
from datetime import datetime, timezone

FORMAT = "cataclysm-hub.scope-observation/v1"
MAX_ENTRIES = 100_000
MAX_BYTES = 8 * 1024**3
MAX_DOCUMENT = 64 * 1024**2


def identity(info):
    return [str(info.st_dev), str(info.st_ino)]


def stamp(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def contains(parent, child):
    return parent == child or parent in child.parents


def output_in_scope(root, output):
    resolved = output.resolve()
    if contains(root, resolved):
        return True
    for ancestor in resolved.parents:
        if ancestor.exists() and os.path.samefile(root, ancestor):
            return True
    return False


def snapshot(scopes, output):
    roots = {name: Path(path).resolve(strict=True) for name, path in scopes.items()}
    if not roots or any(not name or not path.is_dir() for name, path in roots.items()):
        raise ValueError("each named scope must be an existing directory")
    for name, root in roots.items():
        if output_in_scope(root, output):
            raise ValueError("observation output must be outside watched scopes")
        for other, path in roots.items():
            # samefile also catches physical aliases on case-insensitive filesystems.
            if name != other and (contains(root, path) or os.path.samefile(root, path)
                                  or any(os.path.samefile(root, ancestor) for ancestor in path.parents)):
                raise ValueError("watched scopes overlap")
    result = {"format": FORMAT, "captured_at": datetime.now(timezone.utc).isoformat(),
              "coverage": "declared_scopes_only", "binding_verified": False,
              "concurrency_accepted": False, "scopes": {}}
    total_entries = 0
    total_bytes = 0
    for name, root in roots.items():
        root_info = root.stat()
        entries = {}
        pending = [(root, 0)]
        while pending:
            directory, depth = pending.pop()
            if depth > 64:
                raise ValueError("directory depth exceeds observation limit")
            before = directory.lstat()
            if not stat.S_ISDIR(before.st_mode):
                raise ValueError("directory changed during observation")
            with os.scandir(directory) as children:
                for child in children:
                    total_entries += 1
                    if total_entries > MAX_ENTRIES:
                        raise ValueError("entry limit exceeded")
                    path = Path(child.path)
                    key = path.relative_to(root).as_posix()
                    info = path.lstat()
                    item = {"identity": identity(info), "mode": stat.S_IMODE(info.st_mode)}
                    if stat.S_ISLNK(info.st_mode):
                        try:
                            within_scope = contains(root, path.resolve(strict=True))
                        except (OSError, RuntimeError):
                            within_scope = False
                        item.update(kind="symlink", target=os.readlink(path), within_scope=within_scope)
                    elif stat.S_ISDIR(info.st_mode):
                        item.update(kind="directory")
                        pending.append((path, depth + 1))
                    elif stat.S_ISREG(info.st_mode):
                        total_bytes += info.st_size
                        if total_bytes > MAX_BYTES:
                            raise ValueError("byte limit exceeded")
                        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_BINARY", 0)
                        descriptor = os.open(path, flags)
                        with os.fdopen(descriptor, "rb") as source:
                            opened = os.fstat(source.fileno())
                            if stamp(info) != stamp(opened):
                                raise ValueError("file changed before hashing")
                            digest = hashlib.sha256()
                            read_bytes = 0
                            while chunk := source.read(1024 * 1024):
                                read_bytes += len(chunk)
                                if read_bytes > info.st_size:
                                    raise ValueError("file grew while hashing")
                                digest.update(chunk)
                            if stamp(opened) != stamp(os.fstat(source.fileno())) or read_bytes != info.st_size:
                                raise ValueError("file changed while hashing")
                        if stamp(info) != stamp(path.lstat()):
                            raise ValueError("file was replaced while hashing")
                        item.update(kind="file", size=str(info.st_size), sha256=digest.hexdigest())
                    else:
                        item.update(kind="special")
                    entries[key] = item
            if identity(before) != identity(directory.lstat()):
                raise ValueError("directory replaced while observing")
        if identity(root_info) != identity(root.stat()):
            raise ValueError("scope root replaced while observing")
        result["scopes"][name] = {"root": str(root), "identity": identity(root_info), "entries": entries}
    return result


def load_snapshot(path):
    if Path(path).stat().st_size > MAX_DOCUMENT:
        raise ValueError("observation document too large")
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate JSON field")
            result[key] = value
        return result
    with Path(path).open("rb") as source:
        raw = source.read(MAX_DOCUMENT + 1)
    if len(raw) > MAX_DOCUMENT:
        raise ValueError("observation document too large")
    data = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object)
    if not isinstance(data, dict):
        raise ValueError("observation must be an object")
    if data.get("format") != FORMAT or data.get("coverage") != "declared_scopes_only":
        raise ValueError("unsupported observation format")
    if data.get("binding_verified") is not False or data.get("concurrency_accepted") is not False:
        raise ValueError("observation cannot grant acceptance")
    if not isinstance(data.get("scopes"), dict) or not data["scopes"]:
        raise ValueError("missing scopes")
    entry_count = 0
    for scope in data["scopes"].values():
        if not isinstance(scope, dict) or not isinstance(scope.get("root"), str) or not isinstance(scope.get("identity"), list) or not isinstance(scope.get("entries"), dict):
            raise ValueError("invalid scope")
        if not Path(scope["root"]).is_absolute() or len(scope["identity"]) != 2 or not all(isinstance(value, str) and value.isdecimal() for value in scope["identity"]):
            raise ValueError("invalid scope identity")
        entry_count += len(scope["entries"])
        if entry_count > MAX_ENTRIES:
            raise ValueError("entry limit exceeded")
        for name, item in scope["entries"].items():
            if not name or name.startswith("/") or any(part in ("", ".", "..") for part in name.split("/")) or not isinstance(item, dict):
                raise ValueError("invalid entry")
            kind = item.get("kind")
            if kind not in ("file", "directory", "symlink", "special"):
                raise ValueError("invalid entry type")
            if kind == "file" and (not isinstance(item.get("size"), str) or not item["size"].isdecimal() or not isinstance(item.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"])):
                raise ValueError("invalid file digest")
            if kind == "symlink" and (not isinstance(item.get("target"), str) or type(item.get("within_scope")) is not bool):
                raise ValueError("invalid link")
    return data


def compare(before, after, allowed):
    if before["scopes"].keys() != after["scopes"].keys():
        raise ValueError("scope sets differ; cannot compare partial coverage")
    if not set(allowed) <= before["scopes"].keys():
        raise ValueError("unknown allowed scope")
    changes = []
    replaced_roots = []
    unsafe_entries = []
    for name, old in before["scopes"].items():
        new = after["scopes"][name]
        if old["root"] != new["root"] or old["identity"] != new["identity"]:
            replaced_roots.append(name)
        for path in sorted(old["entries"].keys() | new["entries"].keys()):
            original, current = old["entries"].get(path), new["entries"].get(path)
            if any(item and (item.get("kind") == "special" or (item.get("kind") == "symlink" and not item.get("within_scope"))) for item in (original, current)):
                unsafe_entries.append({"scope": name, "path": path})
            if original != current:
                changes.append({"scope": name, "path": path, "change": "created" if original is None else "removed" if current is None else "modified", "before": original, "after": current})
    unexpected = sorted({item["scope"] for item in changes} - set(allowed))
    return {"format": "cataclysm-hub.scope-diff/v1", "coverage": "declared_scopes_only",
            "binding_verified": False, "concurrency_accepted": False,
            "unexpected_scopes": unexpected, "replaced_roots": replaced_roots,
            "unsafe_entries": unsafe_entries, "changes": changes,
            "observation_check": "blocked" if unexpected or replaced_roots or unsafe_entries else "within_declared_allowance",
            "limitations": ["Not an OS write trace or a sandbox", "Temporary writes deleted between captures are not observed", "Captures are not atomic across a running game", "Does not prove game loading, process lineage, binding, or concurrency acceptance"]}


def write_new(path, data):
    encoded = json.dumps(data, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    if len(encoded.encode("utf-8")) > MAX_DOCUMENT:
        raise ValueError("output exceeds observation document limit")
    with Path(path).open("x", encoding="utf-8") as output:
        output.write(encoded)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture = commands.add_parser("snapshot")
    capture.add_argument("--scope", action="append", required=True, help="unique-name=/existing/directory")
    capture.add_argument("--output", type=Path, required=True)
    diff = commands.add_parser("diff")
    diff.add_argument("--before", type=Path, required=True)
    diff.add_argument("--after", type=Path, required=True)
    diff.add_argument("--allow-change", action="append", default=[])
    diff.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "snapshot":
            scopes = {}
            for value in args.scope:
                name, separator, path = value.partition("=")
                if not separator or not path or name in scopes:
                    raise ValueError("invalid or duplicate scope")
                scopes[name] = path
            data = snapshot(scopes, args.output)
        else:
            before, after = load_snapshot(args.before), load_snapshot(args.after)
            if any(output_in_scope(Path(scope["root"]), args.output) for scope in before["scopes"].values()):
                raise ValueError("diff output must be outside watched scopes")
            data = compare(before, after, args.allow_change)
        write_new(args.output, data)
        print(json.dumps({"output": str(args.output), "binding_verified": False,
                          "concurrency_accepted": False, "observation_check": data.get("observation_check")}, ensure_ascii=False))
        return 2 if data.get("observation_check") == "blocked" else 0
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f"OBSERVATION_FAILED: {type(error).__name__}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
