import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from check_config_cycle import check_cycle, main
from observe_scopes import snapshot, write_new


class ConfigCycleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for path in ("instance/config", "control"):
            (self.root / path).mkdir(parents=True)
        self.target = self.root / "instance/config/options.json"
        self.scopes = {name: str(self.root / name) for name in ("instance", "control")}
        self.original = b'[ {"name":"SOUND_ENABLED", "value":"false"} ]\n'
        self.proposed = b'[{"name":"SOUND_ENABLED","value":"true"}]\n'
        self.expected = hashlib.sha256(self.proposed).hexdigest()
        self.target.write_bytes(self.original)
        self.before = self.capture()
        self.replace(self.proposed)
        self.applied = self.capture()
        self.replace(self.original)
        self.rolled_back = self.capture()

    def capture(self):
        return snapshot(self.scopes, self.root / "outside.json")

    def replace(self, content):
        temp = self.target.with_suffix(".tmp")
        temp.write_bytes(content)
        temp.replace(self.target)

    def check(self, before=None, applied=None, rolled_back=None):
        return check_cycle(before or self.before, applied or self.applied,
                           rolled_back or self.rolled_back, "instance",
                           "config/options.json", self.expected)

    def test_exact_byte_cycle_allows_target_inode_replacement_without_certifying_game(self):
        result = self.check()
        self.assertEqual(result["file_cycle_check"], "matched")
        self.assertEqual(len(result["phase_changes"]["apply"]), 1)
        for key in ("binding_verified", "concurrency_accepted", "game_loading_verified",
                    "audio_verified", "new_world_defaults_verified", "evidence_authenticity_verified"):
            self.assertIs(result[key], False)
        self.assertEqual(self.target.read_bytes(), self.original)

    def test_unrelated_change_inside_target_instance_is_not_blanket_allowed(self):
        (self.root / "instance/unexpected").write_bytes(b"side effect")
        result = self.check(rolled_back=self.capture())
        self.assertEqual(result["file_cycle_check"], "blocked")
        self.assertTrue(any(i["reason"] == "unexpected_change" for i in result["issues"]))

    def test_control_changes_root_replacement_and_unsafe_entries_are_blocked(self):
        (self.root / "control/changed").write_bytes(b"side effect")
        result = self.check(rolled_back=self.capture())
        self.assertEqual(result["file_cycle_check"], "blocked")
        altered = copy.deepcopy(self.rolled_back)
        altered["scopes"]["instance"]["identity"] = ["999", "999"]
        self.assertTrue(any(i["reason"] == "root_replaced" for i in self.check(rolled_back=altered)["issues"]))
        altered["scopes"]["control"]["entries"]["special"] = {
            "kind": "special", "identity": ["1", "1"], "mode": 0o600}
        self.assertTrue(any(i["reason"] == "unsafe_entry" for i in self.check(rolled_back=altered)["issues"]))

    def test_wrong_apply_rollback_mode_and_missing_file_are_blocked(self):
        for modification, expected in (
            ({"sha256": "a" * 64}, "rollback_does_not_match_original_bytes"),
            ({"mode": 0o777}, "target_mode_changed"),
        ):
            altered = copy.deepcopy(self.rolled_back)
            altered["scopes"]["instance"]["entries"]["config/options.json"].update(modification)
            self.assertTrue(any(i["reason"] == expected for i in self.check(rolled_back=altered)["issues"]))
        altered = copy.deepcopy(self.applied)
        altered["scopes"]["instance"]["entries"]["config/options.json"]["sha256"] = "a" * 64
        self.assertEqual(self.check(applied=altered)["file_cycle_check"], "blocked")
        missing = copy.deepcopy(self.rolled_back)
        del missing["scopes"]["instance"]["entries"]["config/options.json"]
        self.assertEqual(self.check(rolled_back=missing)["file_cycle_check"], "blocked")

    def test_partial_scope_unknown_time_or_missing_identity_is_rejected(self):
        for edit in (
            lambda data: data["scopes"].pop("control"),
            lambda data: data.update(captured_at="invalid"),
            lambda data: data["scopes"]["instance"]["entries"]["config/options.json"].pop("identity"),
        ):
            altered = copy.deepcopy(self.rolled_back)
            edit(altered)
            with self.assertRaises(ValueError):
                self.check(rolled_back=altered)
        with self.assertRaises(ValueError):
            self.check(before=self.rolled_back, rolled_back=self.before)
        with self.assertRaises(ValueError):
            check_cycle(self.before, self.applied, self.rolled_back,
                        "instance", "save/world/mods.json", self.expected)

    def test_cli_new_external_output_only_and_blocked_exit_code(self):
        for label, item in (("before", self.before), ("applied", self.applied),
                            ("rolled_back", self.rolled_back)):
            write_new(self.root / f"{label}.json", item)
        arguments = ["--before", str(self.root / "before.json"),
                     "--applied", str(self.root / "applied.json"),
                     "--rolled-back", str(self.root / "rolled_back.json"),
                     "--scope", "instance", "--path", "config/options.json",
                     "--expected-applied-sha256", self.expected]
        output = self.root / "result.json"
        self.assertEqual(main(arguments + ["--output", str(output)]), 0)
        self.assertEqual(json.loads(output.read_text())["file_cycle_check"], "matched")
        original = output.read_bytes()
        self.assertEqual(main(arguments + ["--output", str(output)]), 1)
        self.assertEqual(output.read_bytes(), original)
        self.assertEqual(main(arguments + ["--output", str(self.root / "control/result.json")]), 1)
        arguments[-1] = "b" * 64
        self.assertEqual(main(arguments + ["--output", str(self.root / "blocked.json")]), 2)


if __name__ == "__main__":
    unittest.main()
