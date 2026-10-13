# plain-core

Shared Rust code for PlainApp desktop, mobile and command-line hosts.

```text
plain-core/
├── Cargo.toml                Builds and tests the five packages together
├── plain-rs/                 Domain services, storage, crypto and protocols
├── plain-server/             Server, GraphQL and host communication
├── plain-media/              Media catalog, scanning and processing
├── plain-inference/          Model inference
├── plain-cli/                The plainapp command-line executable
├── schema/                   GraphQL and SQLite definitions
└── scripts/                  Development tools
```

Each `plain-*` directory is a Rust package managed by the root `Cargo.toml`.
Desktop shell and mobile JNI/C ABI code live in plain-desktop and plain-app, respectively.

## Build

Requires Rust 1.96+, Python 3.10+ and a C compiler.

```sh
cargo check --workspace --locked    # Check all packages for compile errors
cargo test --workspace --locked     # Test Rust functionality in all packages
cargo run -p plainapp -- run --help  # Build plainapp and show run-command help
```

Use `run --web-root <directory>` to serve a built web frontend.

## Work on desktop or app

Keep plain-core next to plain-desktop or plain-app. Run these commands in the desktop or app repository; its `scripts/core` calls `plain-core/scripts/core-dependencies.py`.

```bash
./scripts/core local        # Use local plain-core
./scripts/core local --off  # Stop using local paths
./scripts/core update       # Lock this project to the latest remote commit
```
