import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[2] / 'scripts/core-dependencies.py'
spec = importlib.util.spec_from_file_location('core_dependencies', SCRIPT)
dependencies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dependencies)


class CoreDependenciesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.core = self.base / 'plain-core'
        for path in dependencies.PACKAGES.values():
            folder = self.core / path
            folder.mkdir(parents=True)
            (folder / 'Cargo.toml').write_text('[package]\n')
        self.root = self.base / 'plain-desktop'
        self.root.mkdir()
        self.manifest = self.root / 'Cargo.toml'
        self.manifest.write_text('[workspace]\n')
        self.lock = self.root / 'Cargo.lock'
        self.lock.write_bytes(b'pinned git dependency\n')
        self.config = self.root / '.cargo/config.toml'
        self.config.parent.mkdir()
        self.original_config = b'[net]\ngit-fetch-with-cli = true\n'
        self.config.write_bytes(self.original_config)
        self.mock = patch.object(dependencies, 'cargo').start()
        self.addCleanup(patch.stopall)

    def local(self):
        dependencies.configure(self.root, self.manifest, self.core, 'local')

    def pinned(self):
        dependencies.configure(self.root, self.manifest, self.core, 'pinned')

    def test_switch_restores_exact_pinned_lock_and_machine_config(self):
        self.local()
        self.lock.write_bytes(b'local path dependency\n')
        self.pinned()
        self.assertEqual(self.lock.read_bytes(), b'pinned git dependency\n')
        self.assertEqual(self.config.read_bytes(), self.original_config)
        self.assertFalse((self.root / '.cargo/core-dependency-state').exists())
        self.assertIn('--locked', self.mock.call_args.args)

    def test_failed_resolution_restores_previous_files(self):
        def fail(*args):
            self.lock.write_bytes(b'partial update\n')
            raise subprocess.CalledProcessError(1, 'cargo')
        self.mock.side_effect = fail
        with self.assertRaises(subprocess.CalledProcessError):
            self.local()
        self.assertEqual(self.lock.read_bytes(), b'pinned git dependency\n')
        self.assertEqual(self.config.read_bytes(), self.original_config)
        self.assertFalse((self.root / '.cargo/core-dependency-state').exists())

    def test_configuration_edits_are_not_overwritten(self):
        self.local()
        edited = self.config.read_bytes() + b'\n[build]\njobs = 2\n'
        self.config.write_bytes(edited)
        with self.assertRaisesRegex(ValueError, 'configuration changed'):
            self.pinned()
        self.assertEqual(self.config.read_bytes(), edited)

    def test_manifest_edits_prevent_stale_lock_restore(self):
        self.local()
        self.manifest.write_text('[workspace]\nresolver = "3"\n')
        self.lock.write_bytes(b'new dependencies\n')
        with self.assertRaisesRegex(ValueError, 'manifests changed'):
            self.pinned()
        self.assertEqual(self.lock.read_bytes(), b'new dependencies\n')

    def test_repeated_local_switch_does_not_replace_saved_pin(self):
        self.local()
        self.lock.write_bytes(b'local dependencies\n')
        self.local()
        self.pinned()
        self.assertEqual(self.lock.read_bytes(), b'pinned git dependency\n')

    def test_custom_named_mobile_checkout_uses_bridge_lock(self):
        root = self.base / 'custom-mobile'
        bridge = root / 'plain-rs'
        bridge.mkdir(parents=True)
        manifest = bridge / 'Cargo.toml'
        manifest.write_text('[package]\n')
        lock = bridge / 'Cargo.lock'
        lock.write_bytes(b'mobile pinned\n')
        dependencies.configure(root, manifest, self.core, 'local')
        lock.write_bytes(b'mobile local\n')
        dependencies.configure(root, manifest, self.core, 'pinned')
        self.assertEqual(lock.read_bytes(), b'mobile pinned\n')
        self.assertFalse((root / '.cargo/config.toml').exists())

    def test_update_rolls_back_both_consumers_on_second_failure(self):
        app = self.base / 'plain-app/plain-rs'
        app.mkdir(parents=True)
        manifest = app / 'Cargo.toml'
        manifest.write_text('[package]\n')
        lock = app / 'Cargo.lock'
        lock.write_bytes(b'app original pin\n')
        def update(root, *args):
            self.lock.write_bytes(b'new desktop pin\n')
            lock.write_bytes(b'partial app pin\n')
            if root.name == 'plain-app':
                raise subprocess.CalledProcessError(1, 'cargo')
        self.mock.side_effect = update
        with self.assertRaises(subprocess.CalledProcessError):
            dependencies.update([(self.root, self.manifest), (app.parent, manifest)], 'a' * 40)
        self.assertEqual(self.lock.read_bytes(), b'pinned git dependency\n')
        self.assertEqual(lock.read_bytes(), b'app original pin\n')


if __name__ == '__main__':
    unittest.main()
