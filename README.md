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

Requires Rust 1.99+, Python 3.10+ and a C compiler.

```sh
cargo check --workspace --locked    # Check all packages for compile errors
cargo test --workspace --locked     # Test Rust functionality in all packages
cargo run -p plainapp -- run --help  # Build plainapp and show run-command help
```

## Deploy the CLI

`plainapp` is a single executable with the web UI compiled into it, and a Linux host runs it as the systemd service `plainapp`.

### One-line install (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/plainhub/plain-core/main/install.sh | bash
```

It detects the architecture, downloads the matching release, installs the dependencies and the systemd unit, starts the service, and prints the addresses to open.

```bash
sudo plainapp passwd   # admin password, prompted twice
```

- `http://<host>:8080` — HTTP
- `https://<host>:8443` — HTTPS with a self-signed certificate created on first start for `plainapp.local` and `localhost`

### Install manually

```sh
cargo build --release --locked -p plainapp
install -m 0755 target/release/plainapp /usr/local/bin/plainapp
sudo plainapp install                     # ffmpeg, Samba, Avahi
sudo plainapp install --with-libreoffice  # add DOC/DOCX preview
sudo systemctl enable --now plainapp
```

`plainapp install` writes `/etc/systemd/system/plainapp.service`, whose `ExecStart` is `/usr/local/bin/plainapp run`, so that path is required. It does not copy the binary, and it neither enables nor starts the service.

A checkout without `plain-cli/src/web_dist` builds a `plainapp` that serves the API only.

### Configuration

`run --web-root <directory>` serves a frontend bundle from disk instead of the compiled one, which is how a local frontend build is tried without rebuilding:

```ini
[Service]
ExecStart=/usr/local/bin/plainapp run --web-root /opt/plainapp/web
```

Ports come from user settings and default to `8080` and `8443`. Data and cache live in `$HOME/.plainapp/data` and `$HOME/.plainapp/cache`; the unit sets no `User=`, so it runs as root and uses `/root/.plainapp`. Override with `--data-dir`, `--cache-dir`, `--tls-cert`, `--tls-key` or the `PLAINAPP_DATA_DIR`, `PLAINAPP_CACHE_DIR`, `PLAINAPP_TLS_CERT` and `PLAINAPP_TLS_KEY` environment variables.

### Upgrade and removal

`plainapp update` only reports that self-update is not implemented, so an upgrade replaces the executable and restarts the service:

```sh
install -m 0755 new/plainapp /usr/local/bin/plainapp
sudo systemctl restart plainapp
```

`sudo plainapp uninstall` removes the unit, the certificate and the data directory. Pass `--keep-config` and `--keep-data` to retain them.

### Releases

A `v*` tag builds the frontend from plain-desktop, compiles it into `plainapp` for Linux amd64 and arm64, and attaches the archives to the GitHub release.

## Work on desktop or app

Keep plain-core next to plain-desktop or plain-app. Run these commands in the desktop or app repository; its `scripts/core` calls `plain-core/scripts/core-dependencies.py`.

```bash
./scripts/core local        # Use local plain-core
./scripts/core local --off  # Stop using local paths
./scripts/core update       # Lock this project to the latest remote commit
```
