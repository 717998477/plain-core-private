import argparse
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

URL = "https://github.com/plainhub/plain-core.git"
PACKAGES = {
    "plain-rs": "plain-rs",
    "plain-server": "crates/plain-server",
    "plain-media": "crates/plain-media",
    "plain-inference": "crates/plain-inference",
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
            if hashlib.sha256(before_config or b"").hexdigest() != record["config_hash"]:
                raise ValueError("Local Cargo configuration changed; reconcile it before switching")
            if record["core"] != str(core):
                raise ValueError("Switch to pinned mode before changing the core checkout")
            cargo(root, manifest, "metadata", "--offline", "--format-version", "1")
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
        record["config_hash"] = hashlib.sha256(config.read_bytes()).hexdigest()
        (state / "state.json").write_text(json.dumps(record))
    else:
        if not state.exists():
            cargo(root, manifest, "metadata", "--offline", "--locked", "--format-version", "1")
            return
        record = json.loads((state / "state.json").read_text())
        if hashlib.sha256(before_config or b"").hexdigest() != record["config_hash"]:
            raise ValueError("Local Cargo configuration changed; reconcile it before switching")
        if record["manifests"] != manifests(root, manifest):
            raise ValueError("Consumer manifests changed; reconcile the pinned lock before switching")
        if record["config_exists"]:
            config.write_bytes((state / "config.original").read_bytes())
        else:
            config.unlink()
        lock.write_bytes((state / "Cargo.lock.pinned").read_bytes())
    try:
        args = ["metadata", "--offline", "--format-version", "1"]
        if mode == "pinned":
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
    print(f"{root.name}: {mode}")


def update(projects, commit):
    for root, _ in projects:
        if (root / ".cargo/core-dependency-state").exists():
            raise ValueError("Switch all consumers to pinned mode before updating")
    backups = [manifest.parent / "Cargo.lock" for _, manifest in projects]
    contents = [path.read_bytes() for path in backups]
    try:
        for root, manifest in projects:
            cargo(root, manifest, "update", "-p", "plain-server", "--precise", commit)
    except BaseException:
        for path, content in zip(backups, contents):
            path.write_bytes(content)
        raise


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["local", "pinned", "update"])
    parser.add_argument("--project", action="append", choices=["desktop", "app"])
    parser.add_argument("--commit")
    parser.add_argument("--core", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--desktop", type=Path)
    parser.add_argument("--app", type=Path)
    args = parser.parse_args()
    core = args.core.resolve()
    choices = {
        "desktop": (args.desktop or core.parent / "plain-desktop").resolve(),
        "app": (args.app or core.parent / "plain-app").resolve(),
    }
    projects = [(choices[name], choices[name] / ("plain-rs/Cargo.toml" if name == "app" else "Cargo.toml"))
                for name in args.project or choices]
    if args.mode == "update":
        commit = args.commit or subprocess.check_output(
            ["git", "ls-remote", URL, "refs/heads/main"], text=True).split()[0]
        update(projects, commit)
    else:
        for root, manifest in projects:
            configure(root, manifest, core, args.mode)


if __name__ == "__main__":
    main()
