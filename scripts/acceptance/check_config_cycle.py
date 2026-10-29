#!/usr/bin/env python3
"""Check a stopped-game single-file apply/rollback cycle from existing scope snapshots."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import sys

from observe_scopes import compare, load_snapshot, output_in_scope, write_new

FORMAT = "cataclysm-hub.config-cycle-observation/v1"
CONFIG_PATHS = {"config/options.json", "config/user-default-mods.json"}


def canonical_digest(snapshot):
    encoded = json.dumps(snapshot, ensure_ascii=False, sort_keys=True,
                         separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def validate_observation(snapshot):
    """Require the identity/mode fields needed for strict change comparisons."""
    captured = snapshot.get("captured_at")
    if not isinstance(captured, str):
        raise ValueError("missing capture time")
    at = datetime.fromisoformat(captured.replace("Z", "+00:00"))
    if at.utcoffset() is None:
        raise ValueError("capture time must include a timezone")
    for scope in snapshot["scopes"].values():
        for item in scope["entries"].values():
            identity = item.get("identity")
            if not isinstance(identity, list) or len(identity) != 2 or not all(
                    isinstance(value, str) and value.isdecimal() for value in identity):
                raise ValueError("missing or invalid entry identity")
            mode = item.get("mode")
            if type(mode) is not int or not 0 <= mode <= 0o7777:
                raise ValueError("missing or invalid entry mode")
    return at


def check_cycle(before, applied, rolled_back, scope, path, expected_sha256):
    if path not in CONFIG_PATHS:
        raise ValueError("only one supported configuration file may be checked")
    if not re.fullmatch(r"[0-9a-f]{64}", expected_sha256):
        raise ValueError("expected applied SHA-256 must be lowercase hexadecimal")
    times = [validate_observation(item) for item in (before, applied, rolled_back)]
    if times != sorted(times):
        raise ValueError("capture times are not in phase order")
    if scope not in before["scopes"] or len(before["scopes"]) < 2:
        raise ValueError("declare the target instance and at least one independent control scope")
    phases = [compare(before, applied, []), compare(applied, rolled_back, [])]
    issues = []
    for label, diff in zip(("apply", "rollback"), phases):
        for replaced in diff["replaced_roots"]:
            issues.append({"phase": label, "reason": "root_replaced", "scope": replaced})
        for unsafe in diff["unsafe_entries"]:
            issues.append({"phase": label, "reason": "unsafe_entry", **unsafe})
        for change in diff["changes"]:
            if (change["scope"], change["path"]) != (scope, path):
                issues.append({"phase": label, "reason": "unexpected_change",
                               "scope": change["scope"], "path": change["path"],
                               "change": change["change"]})
    targets = [item["scopes"][scope]["entries"].get(path)
               for item in (before, applied, rolled_back)]
    if any(item is None or item["kind"] != "file" for item in targets):
        issues.append({"reason": "target_missing_or_not_regular"})
    else:
        old, new, restored = targets
        if old["sha256"] == expected_sha256:
            issues.append({"reason": "no_byte_change_requested"})
        if new["sha256"] != expected_sha256:
            issues.append({"reason": "applied_bytes_do_not_match_plan"})
        if (old["sha256"], old["size"]) != (restored["sha256"], restored["size"]):
            issues.append({"reason": "rollback_does_not_match_original_bytes"})
        if len({item["mode"] for item in targets}) != 1:
            issues.append({"reason": "target_mode_changed"})
    return {
        "format": FORMAT, "coverage": "declared_scopes_only",
        "target_scope": scope, "target_path": path,
        "expected_applied_sha256": expected_sha256,
        "snapshot_canonical_sha256": {
            label: canonical_digest(item) for label, item in
            zip(("before", "applied", "rolled_back"), (before, applied, rolled_back))
        },
        "phase_changes": {label: diff["changes"] for label, diff in
                          zip(("apply", "rollback"), phases)},
        "issues": issues,
        "file_cycle_check": "blocked" if issues else "matched",
        "binding_verified": False, "concurrency_accepted": False,
        "game_loading_verified": False, "audio_verified": False,
        "new_world_defaults_verified": False, "evidence_authenticity_verified": False,
        "limitations": [
            "Checks supplied snapshots, not their authenticity or which process wrote files",
            "Requires a separate stopped-game apply/rollback run; does not verify process exit",
            "Target inode replacement is expected; other declared changes are blocked",
            "Does not observe undeclared paths or temporary writes removed between captures",
            "Does not certify game settings, audible sound, new-world defaults or save compatibility",
            "Game serialization after launch may invalidate a rollback plan; do not force overwrite",
        ],
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--applied", type=Path, required=True)
    parser.add_argument("--rolled-back", type=Path, required=True)
    parser.add_argument("--scope", required=True)
    parser.add_argument("--path", choices=sorted(CONFIG_PATHS), required=True)
    parser.add_argument("--expected-applied-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        observations = [load_snapshot(path) for path in
                        (args.before, args.applied, args.rolled_back)]
        for observation in observations:
            if any(output_in_scope(Path(scope["root"]), args.output)
                   for scope in observation["scopes"].values()):
                raise ValueError("evidence output must be outside every observed scope")
        result = check_cycle(*observations, args.scope, args.path,
                             args.expected_applied_sha256)
        write_new(args.output, result)
        print(json.dumps({"output": str(args.output),
                          "file_cycle_check": result["file_cycle_check"],
                          "game_loading_verified": False}, ensure_ascii=False))
        return 2 if result["file_cycle_check"] == "blocked" else 0
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f"CONFIG_CYCLE_FAILED: {type(error).__name__}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
