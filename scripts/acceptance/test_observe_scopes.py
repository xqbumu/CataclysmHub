import json
from pathlib import Path
import tempfile
import unittest
from observe_scopes import compare, load_snapshot, snapshot, write_new

class ObservationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ("a", "b"):
            (self.root / name).mkdir()
        self.scopes = {name: str(self.root / name) for name in ("a", "b")}

    def capture(self):
        return snapshot(self.scopes, self.root / "outside.json")

    def test_same_size_change_and_other_instance_write_are_detected(self):
        (self.root / "a/save").write_bytes(b"old")
        before = self.capture()
        (self.root / "a/save").write_bytes(b"new")
        (self.root / "b/unexpected").write_bytes(b"unexpected")
        diff = compare(before, self.capture(), ["a"])
        self.assertEqual(diff["unexpected_scopes"], ["b"])
        self.assertEqual(diff["observation_check"], "blocked")
        self.assertEqual(len(diff["changes"]), 2)
        self.assertFalse(diff["binding_verified"])
        self.assertFalse(diff["concurrency_accepted"])

    def test_allowed_write_is_only_an_observation_and_does_not_approve_binding(self):
        before = self.capture()
        (self.root / "a/save").mkdir()
        (self.root / "a/save/world").write_bytes(b"opaque")
        diff = compare(before, self.capture(), ["a"])
        self.assertEqual(diff["observation_check"], "within_declared_allowance")
        self.assertFalse(diff["binding_verified"])
        self.assertFalse(diff["concurrency_accepted"])
        self.assertEqual((self.root / "a/save/world").read_bytes(), b"opaque")

    def test_scope_root_replacement_is_blocked_even_if_empty(self):
        before = self.capture()
        (self.root / "a").rename(self.root / "old-a")
        (self.root / "a").mkdir()
        self.assertEqual(compare(before, self.capture(), ["a"])["replaced_roots"], ["a"])

    def test_partial_or_unknown_scope_is_rejected(self):
        before = self.capture()
        after = self.capture()
        del after["scopes"]["b"]
        with self.assertRaises(ValueError): compare(before, after, ["a"])
        with self.assertRaises(ValueError): compare(before, before, ["invented"])

    def test_nested_scopes_and_output_inside_scope_are_rejected(self):
        (self.root / "a/child").mkdir()
        with self.assertRaises(ValueError): snapshot({"a": str(self.root / "a"), "child": str(self.root / "a/child")}, self.root / "out")
        with self.assertRaises(ValueError): snapshot(self.scopes, self.root / "a/out")

    def test_links_are_recorded_without_reading_their_targets(self):
        target = self.root / "secret"
        target.write_bytes(b"outside")
        try: (self.root / "a/link").symlink_to(target)
        except OSError: self.skipTest("symlink creation unavailable")
        data = self.capture()
        item = data["scopes"]["a"]["entries"]["link"]
        self.assertEqual(item["kind"], "symlink")
        self.assertNotIn("sha256", item)
        self.assertTrue(compare(data, data, ["a"])["unsafe_entries"])
        self.assertEqual(target.read_bytes(), b"outside")

    def test_internal_framework_style_links_do_not_follow_or_block_observation(self):
        (self.root / "a/real").write_bytes(b"inside")
        try: (self.root / "a/link").symlink_to("real")
        except OSError: self.skipTest("symlink creation unavailable")
        data = self.capture()
        self.assertTrue(data["scopes"]["a"]["entries"]["link"]["within_scope"])
        self.assertFalse(compare(data, data, ["a"])["unsafe_entries"])

    def test_output_never_overwrites_and_claims_cannot_be_promoted(self):
        output = self.root / "observation.json"
        data = self.capture()
        write_new(output, data)
        self.assertEqual(load_snapshot(output), data)
        with self.assertRaises(FileExistsError): write_new(output, data)
        data["binding_verified"] = True
        output.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaises(ValueError): load_snapshot(output)

    def test_non_object_documents_are_rejected(self):
        output = self.root / "invalid.json"
        output.write_text("[]", encoding="utf-8")
        with self.assertRaises(ValueError): load_snapshot(output)

    def test_duplicate_json_fields_are_rejected(self):
        output = self.root / "duplicate.json"
        output.write_text('{"format":"x","format":"y"}', encoding="utf-8")
        with self.assertRaises(ValueError): load_snapshot(output)

if __name__ == "__main__": unittest.main()
