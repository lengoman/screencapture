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

**Start your Server first (HTTP on :8080, gRPC on :50051):**
```bash
screencapture serve --grpc-port 50051 --http-port 8080
```

**Connect your computer as an Agent:**
```bash
screencapture agent --id "laptop-1" --server http://127.0.0.1:50051
```

**Trigger a remote screenshot:**
Navigate your web browser or send a GET request precisely to:
```bash
http://localhost:8080/api/v1/capture/laptop-1
```
