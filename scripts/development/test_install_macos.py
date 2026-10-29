import os
import plistlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import install_macos as installer


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.make_bundle(self.root / "source.app", b"new")
        self.destination = self.root / "Applications with spaces"
        self.destination.mkdir()
        self.old = self.make_bundle(self.destination / "Cataclysm Hub.app", b"old")
        for mocker in (
            patch.object(installer.sys, "platform", "darwin"),
            patch.object(installer, "assert_stopped"),
            patch.object(installer, "copy_bundle", side_effect=lambda src, dst: shutil.copytree(src, dst, symlinks=True)),
        ):
            mocker.start()
            self.addCleanup(mocker.stop)

    def make_bundle(self, path, data):
        binary = path / "Contents/MacOS/cataclysm-hub"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(data)
        binary.chmod(0o755)
        with (path / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump({"CFBundleIdentifier": "com.cataclysmhub.desktop",
                          "CFBundleExecutable": "cataclysm-hub"}, stream)
        return path

    def test_replace_preserves_old_bundle_and_cleans_stage(self):
        target, backup = installer.install(self.source, self.destination)
        self.assertEqual((target / "Contents/MacOS/cataclysm-hub").read_bytes(), b"new")
        self.assertEqual((backup / "Contents/MacOS/cataclysm-hub").read_bytes(), b"old")
        self.assertFalse(list(self.destination.glob(".cataclysm-hub-install-*")))

    def test_first_install(self):
        target, backup = installer.install(self.source, self.root / "new destination")
        self.assertTrue(target.is_dir())
        self.assertIsNone(backup)

    def test_copy_failure_keeps_old_bundle(self):
        with patch.object(installer, "copy_bundle", side_effect=OSError("copy failed")):
            with self.assertRaises(OSError):
                installer.install(self.source, self.destination)
        self.assertEqual((self.old / "Contents/MacOS/cataclysm-hub").read_bytes(), b"old")
        self.assertFalse(list(self.destination.glob(".cataclysm-hub-install-*")))

    def test_failed_publish_restores_old_path(self):
        rename = Path.rename
        def fail_publish(path, target):
            if path.parent.name.startswith(".cataclysm-hub-install-"):
                raise OSError("publish failed")
            return rename(path, target)
        with patch.object(Path, "rename", fail_publish):
            with self.assertRaises(OSError):
                installer.install(self.source, self.destination)
        self.assertEqual((self.old / "Contents/MacOS/cataclysm-hub").read_bytes(), b"old")

    def test_running_application_stops_before_copy(self):
        with patch.object(installer, "assert_stopped", side_effect=RuntimeError("running")):
            with self.assertRaises(RuntimeError):
                installer.install(self.source, self.destination)
        installer.copy_bundle.assert_not_called()

    def test_process_started_during_copy_keeps_old_bundle(self):
        with patch.object(installer, "assert_stopped", side_effect=[None, RuntimeError("running")]):
            with self.assertRaises(RuntimeError):
                installer.install(self.source, self.destination)
        self.assertEqual((self.old / "Contents/MacOS/cataclysm-hub").read_bytes(), b"old")

    def test_same_or_nested_paths_rejected(self):
        for source, destination in [(self.old, self.destination), (self.source, self.source / "nested")]:
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                installer.install(source, destination)

    def test_wrong_bundle_id_is_not_replaced(self):
        with (self.old / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump({"CFBundleIdentifier": "other.app"}, stream)
        with self.assertRaises(ValueError):
            installer.install(self.source, self.destination)
        installer.copy_bundle.assert_not_called()

    @unittest.skipIf(os.name == "nt", "Windows symlink privileges are not assumed")
    def test_symlink_target_rejected(self):
        other = self.root / "links"
        other.mkdir()
        (other / "Cataclysm Hub.app").symlink_to(self.old, target_is_directory=True)
        with self.assertRaises(ValueError):
            installer.install(self.source, other)

    @unittest.skipIf(os.name == "nt", "Windows symlink privileges are not assumed")
    def test_symlink_contents_rejected(self):
        linked = self.root / "linked.app"
        linked.mkdir()
        (linked / "Contents").symlink_to(self.source / "Contents", target_is_directory=True)
        with self.assertRaises(ValueError):
            installer.install(linked, self.destination)


class ProcessTests(unittest.TestCase):
    def test_probe_fails_closed(self):
        for code in (0, 2):
            with self.subTest(code=code), patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], code, "", "probe")), self.assertRaises(RuntimeError):
                installer.assert_stopped()

    def test_no_process_allows_installation(self):
        with patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "")):
            installer.assert_stopped()


if __name__ == "__main__":
    unittest.main()
