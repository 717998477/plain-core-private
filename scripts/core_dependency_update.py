import shutil
import subprocess
import tempfile
from pathlib import Path

from core_dependency_state import URL, cargo, configure


def latest_commit():
    refs = subprocess.check_output(["git", "ls-remote", URL, "refs/heads/main"], text=True).split()
    if not refs:
        raise ValueError("plain-core/main was not found")
    return refs[0]


def update_current(root, manifest, core):
    commit = latest_commit()
    config = root / ".cargo/config.toml"
    state = root / ".cargo/core-dependency-state"
    lock = manifest.parent / "Cargo.lock"
    before_config = config.read_bytes() if config.exists() else None
    before_lock = lock.read_bytes()
    with tempfile.TemporaryDirectory() as directory:
        backup = Path(directory) / "state"
        if state.exists():
            shutil.copytree(state, backup)
        try:
            configure(root, manifest, core, "pinned")
            cargo(root, manifest, "update", "-p", "plain-server", "--precise", commit)
        except BaseException:
            if before_config is None:
                config.unlink(missing_ok=True)
            else:
                config.write_bytes(before_config)
            lock.write_bytes(before_lock)
            if backup.exists():
                if state.exists():
                    shutil.rmtree(state)
                shutil.copytree(backup, state)
            raise
    return commit
