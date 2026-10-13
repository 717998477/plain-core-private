# plain-core

Shared Rust implementation for PlainApp desktop, mobile and headless hosts.

| Package | Responsibility |
| --- | --- |
| `plain-rs` | Domain services, storage, crypto, protocols and platform interfaces |
| `plain-server` | Server assembly, GraphQL, transports and host coordination |
| `plain-media` | Media catalog and processing |
| `plain-inference` | Inference models and ONNX Runtime integration |
| `plainapp` | Headless executable in `crates/plain-cli` |

Tauri shell code lives in plain-desktop. Mobile JNI/C ABI host glue lives in plain-app. Existing Rust package names and Cargo features remain unchanged.

## Build

Use Rust 1.96 or newer. The inference build also requires Python 3.10 or newer and a C compiler.

```sh
cargo check --workspace --locked
cargo test --workspace --locked
cargo run -p plainapp -- run --help
```

Pass `--web-root` to the headless `run` command when serving a separately built web frontend.

## Consumer dependencies

Consumers depend on `https://github.com/plainhub/plain-core.git`, branch `main`. Their committed `Cargo.lock` files pin the exact Git commit and transitive dependencies. Normal builds do not automatically follow new commits.

Check out plain-core, plain-desktop and plain-app as sibling directories. Run the following commands from plain-core:

```sh
python3 scripts/core-dependencies.py local
python3 scripts/core-dependencies.py pinned
python3 scripts/core-dependencies.py update
```

`local` saves each consumer's pinned lock and machine configuration, then resolves all shared packages from this working directory. Uncommitted core changes can be compiled by normal desktop and mobile build commands. Gradle invokes Cargo for native builds so Cargo can detect changes in external Rust sources.

`pinned` restores the saved configuration and exact pinned lock. Changes to consumer manifests or machine configuration require reconciliation before switching, so the tool does not overwrite edits silently. Do not commit consumer locks while local mode is active.

`update` resolves the current remote main commit once and updates both consumer locks to it. It requires pinned mode. To select a specific published commit:

```sh
python3 scripts/core-dependencies.py update --commit <commit-sha>
```

Use `--project desktop` or `--project app` to operate on one consumer. Use `--desktop`, `--app` and `--core` to provide alternate checkout paths. Review and commit the updated consumer locks after validation.

The core commit referenced by a consumer lock must be available on the remote repository before another machine can build it.

## Development-tool tests

```sh
python3 -m unittest discover -s tests/scripts
```
