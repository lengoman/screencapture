# Screencapture & gRPC Tool

A streamlined Rust utility designed to natively capture monitor regions and stream them automatically to an asynchronous gRPC server backend. The workspace includes two native binaries:
- `screencapture`: The core CLI program which binds into your OS shortcut mechanics.
- `screencapture-server`: The localized gRPC listener to automatically decode and stash screenshots into a unified inbox directory.

## Prerequisites
- [Git](https://git-scm.com/) installed
- [Rust & Cargo](https://rustup.rs/) installed (`rustc`, `cargo` commands working correctly)

## Quick Install (macOS / Linux)

If you have Rust installed you can easily build and stash the native executables right to your local `~/.local/bin` using the automated build script:

```bash
curl -sSL https://raw.githubusercontent.com/lengoman/screencapture/main/install.sh | bash
```

*Don't hesitate to inspect the script manually before executing it blindly!*

### Manual Installation
If you prefer not to use `curl`, simply build via Cargo:
1. `git clone https://github.com/lengoman/screencapture.git`
2. `cd screencapture`
3. `cargo build --release`

## Usage Examples

**Start your gRPC Server first:**
```bash
screencapture-server
```

**Capture a fullscreen frame using your shortcut keys globally:**
```bash
screencapture --wait-for-keys --grpc-url http://127.0.0.1:50051 capture-full --monitor 0 --output target.png
```

## Note about `.keys` files
When deploying with `--wait-for-keys`, the `screencapture` binary looks for a `.keys` file in your *current working directory* specifying the activation shortcut. Standard key combinations look like: `CMD+SHIFT+M` or `CTRL+ALT+S`. Ensure it is configured if utilizing the wait flags.
