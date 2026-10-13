import hashlib
import json
import shutil
import subprocess
from pathlib import Path

URL = "https://github.com/plainhub/plain-core.git"
PACKAGES = {
    "plain-rs": "plain-rs",
    "plain-server": "plain-server",
    "plain-media": "plain-media",
    "plain-inference": "plain-inference",
}


def cargo(root, manifest, *args):
    if args[0] == "metadata":
        version = subprocess.check_output(["rustc", "-vV"], text=True)
        host = next(line[6:] for line in version.splitlines() if line.startswith("host: "))
        args = (*args, "--filter-platform", host)
    subprocess.run(["cargo", *args, "--manifest-path", str(manifest)], cwd=root, check=True, stdout=subprocess.DEVNULL)


def manifests(root, manifest):
    paths = {root / "Cargo.toml", root / "src-tauri/Cargo.toml", manifest}
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in paths if p.exists()}


def configure(root, manifest, core, mode):
    lock = manifest.parent / "Cargo.lock"
    config = root / ".cargo/config.toml"
    state = root / ".cargo/core-dependency-state"
    config.parent.mkdir(parents=True, exist_ok=True)
    before_config = config.read_bytes() if config.exists() else None
    before_lock = lock.read_bytes()
    if mode == "local":
        if state.exists():
            record = json.loads((state / "state.json").read_text())
            if (before_config or b"").count(record["patch"].encode()) != 1:
                raise ValueError("The local core override was edited; restore it before switching")
            if record["core"] != str(core):
                raise ValueError("Switch to pinned mode before changing the core checkout")
            cargo(root, manifest, "metadata", "--format-version", "1")
            return
        if f'[patch."{URL}"]' in (before_config or b"").decode():
            raise ValueError("Existing plain-core patch must be reviewed before switching")
        for path in PACKAGES.values():
            if not (core / path / "Cargo.toml").is_file():
                raise ValueError(f"Missing core package: {path}")
        state.mkdir()
        (state / "Cargo.lock.pinned").write_bytes(before_lock)
        (state / "state.json").write_text(json.dumps({
            "core": str(core), "config_exists": before_config is not None,
            "manifests": manifests(root, manifest),
        }))
        (state / "config.original").write_bytes(before_config or b"")
        patch = f'\n[patch."{URL}"]\n' + "".join(
            f"{name} = {{ path = {json.dumps(str(core / path))} }}\n"
            for name, path in PACKAGES.items())
        config.write_bytes((before_config or b"") + patch.encode())
        record = json.loads((state / "state.json").read_text())
        record["patch"] = patch
        (state / "state.json").write_text(json.dumps(record))
    else:
        if not state.exists():
            cargo(root, manifest, "metadata", "--locked", "--format-version", "1")
            return
        record = json.loads((state / "state.json").read_text())
        patch = record["patch"].encode()
        if (before_config or b"").count(patch) != 1:
            raise ValueError("The local core override was edited; restore it before switching")
        remaining = before_config.replace(patch, b"", 1)
        if remaining or record["config_exists"]:
            config.write_bytes(remaining)
        else:
            config.unlink()
        lock.write_bytes((state / "Cargo.lock.pinned").read_bytes())
    try:
        args = ["metadata", "--format-version", "1"]
        if mode == "pinned" and record["manifests"] == manifests(root, manifest):
            args.append("--locked")
        cargo(root, manifest, *args)
    except BaseException:
        if before_config is None:
            config.unlink(missing_ok=True)
        else:
            config.write_bytes(before_config)
        lock.write_bytes(before_lock)
        if mode == "local":
            shutil.rmtree(state)
        raise
    if mode == "pinned":
        shutil.rmtree(state)
