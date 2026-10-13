import argparse
import subprocess
import sys
from pathlib import Path

from core_dependency_state import configure
from core_dependency_update import update_current


def consumer(root):
    if (root / "src-tauri/Cargo.toml").is_file():
        return root / "Cargo.toml"
    if (root / "gradlew").is_file() and (root / "plain-rs/Cargo.toml").is_file():
        return root / "plain-rs/Cargo.toml"
    raise ValueError("Run ./scripts/core from the desktop or app repository root")


def main():
    parser = argparse.ArgumentParser(prog="./scripts/core")
    parser.add_argument("command", choices=["local", "update"])
    parser.add_argument("--off", action="store_true", help="Stop using the local plain-core checkout")
    args = parser.parse_args()
    if args.off and args.command != "local":
        parser.error("--off is only used with local")
    root = Path.cwd().resolve()
    core = Path(__file__).resolve().parents[1]
    manifest = consumer(root)
    if args.command == "local":
        configure(root, manifest, core, "pinned" if args.off else "local")
        print("Using locked plain-core" if args.off else "Using local plain-core")
    else:
        commit = update_current(root, manifest, core)
        print(f"plain-core updated to {commit[:12]}")


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        print(f"core: {error}", file=sys.stderr)
        sys.exit(1)
    except subprocess.CalledProcessError as error:
        print(f"core: {error.cmd[0]} failed ({error.returncode})", file=sys.stderr)
        sys.exit(1)
