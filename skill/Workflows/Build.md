# Build

Compile the op-bridge binary for development or release.

## Steps

### 1. Navigate to source

```bash
cd ~/op-bridge
```

### 2. Determine build target

| User intent | Command |
|-------------|---------|
| Development / default | `cargo build` |
| Release (optimized) | `cargo build --release` |
| Linux amd64 static | `cargo build --release --target x86_64-unknown-linux-musl` |
| Linux arm64 static | `cargo build --release --target aarch64-unknown-linux-musl` |

For cross-compilation targets, ensure the target is installed:

```bash
rustup target add x86_64-unknown-linux-musl    # or aarch64-unknown-linux-musl
```

### 3. Run CI gates before release builds

For release builds, run the full CI gate:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
cargo audit
```

If any gate fails, fix the issue before proceeding.

### 4. Build

Run the appropriate cargo command from step 2.

### 5. Report result

Report:
- Binary path and size
- Target architecture
- Build profile (debug/release)

```bash
ls -lh target/debug/op-bridge 2>/dev/null || ls -lh target/release/op-bridge 2>/dev/null
```
