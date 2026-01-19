# Building TEE WASM Runner with TDX Support

## Problem

Even though you're running in a TDX TEE (have `/dev/tdx_guest` device), the runner shows:
```
WARN attester] No TEE platform detected. Sample Attester will be used.
```

This happens because the runner was built without TDX support features.

## Solution

### Option 1: Build with TDX Support (Recommended)

Rebuild the runner with the TDX attester feature:

```bash
cd /path/to/guest-components

# Clean previous build
cargo clean --package tee-wasm-runner

# Build with TDX support
cargo build --release --package tee-wasm-runner --features "tdx-attester"

# Verify it worked
./target/release/tee-wasm-runner --version
```

### Option 2: Add to Workspace Features

To make TDX the default for all builds, update workspace Cargo.toml:

```bash
cd /path/to/guest-components

# Edit root Cargo.toml
cat >> Cargo.toml << 'EOF'

[workspace.metadata.default-features]
tee-wasm-runner = ["tdx-attester"]
EOF

# Then build
cargo build --release --package tee-wasm-runner
```

### Option 3: Use Environment Variable

Set default features via environment:

```bash
export CARGO_FEATURES_TEE_WASM_RUNNER="tdx-attester"
cargo build --release --package tee-wasm-runner
```

## Available TEE Features

The attestation-agent supports these platform-specific attesters:

| Platform | Feature | Device Check |
|----------|---------|---------------|
| Intel TDX | `tdx-attester` | `/dev/tdx_guest` |
| AMD SEV-SNP | `snp-attester` | `/dev/sev` |
| Intel SGX | `sgx-attester` | `/dev/sgx_enclave` or `/dev/sgx_provision` |
| Azure SNP vTPM | `az-snp-vtpm-attester` | `/dev/tpm0` (vTPM) |
| Azure TDX vTPM | `az-tdx-vtpm-attester` | `/dev/tdx_guest` (Azure) |
| IBM SE | `cca-attester` | `/dev/s390` + SE enabled |
| AMD SEV-SNP | `se-snp-attester` | AMD SNP variant |

### Building for Different Platforms

#### For Intel TDX
```bash
cargo build --release --package tee-wasm-runner --features "tdx-attester"
```

#### For AMD SEV-SNP
```bash
cargo build --release --package tee-wasm-runner --features "snp-attester"
```

#### For Intel SGX
```bash
cargo build --release --package tee-wasm-runner --features "sgx-attester"
```

#### For Multiple Platforms (for testing)
```bash
cargo build --release --package tee-wasm-runner --features "tdx-attester,snp-attester,sgx-attester"
```

## Verifying TEE Detection

### Check if TDX Device Exists

```bash
# Check for TDX guest device
ls -l /dev/tdx_guest

# Should show: crw-rw---- 1 root root 244, 0 ...
```

### Check Attester Features

```bash
cd /path/to/guest-components/attestation-agent

# View available features
grep -A 20 "\[features\]" attestation-agent/Cargo.toml

# Check what's enabled
cargo tree --features tee-wasm-runner
```

### Test TEE Detection

After building with TDX support, verify:

```bash
# Run with debug logging
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/test

# Should see:
# [INFO] TEE Platform: Tdx (NOT Sample!)
# [DEBUG] Found TDX guest device at /dev/tdx_guest
```

## Makefile Integration

Update the Makefile to support TEE builds:

```makefile
# Build for development/testing (sample attester)
tee-wasm-runner:
	cargo build --release --package tee-wasm-runner

# Build for TDX environments
tee-wasm-runner-tdx:
	cargo build --release --package tee-wasm-runner --features "tdx-attester"

# Build for SNP environments
tee-wasm-runner-snp:
	cargo build --release --package tee-wasm-runner --features "snp-attester"

# Build for SGX environments
tee-wasm-runner-sgx:
	cargo build --release --package tee-wasm-runner --features "sgx-attester"

# Detect platform and build appropriately (experimental)
tee-wasm-runner-auto:
	@echo "Detecting TEE platform..."
	@if [ -e /dev/tdx_guest ]; then \
		cargo build --release --package tee-wasm-runner --features "tdx-attester"; \
	elif [ -e /dev/sev ]; then \
		cargo build --release --package tee-wasm-runner --features "snp-attester"; \
	elif [ -e /dev/sgx_enclave ]; then \
		cargo build --release --package tee-wasm-runner --features "sgx-attester"; \
	else \
		cargo build --release --package tee-wasm-runner; \
	fi
```

## Deployment Considerations

### For Production/Development Teams

#### Create Platform-Specific Binaries

Different TEE platforms may require different builds:

```bash
# Create TDX build for production
cargo build --release --package tee-wasm-runner --features "tdx-attester"
cp target/release/tee-wasm-runner target/release/tee-wasm-runner-tdx

# Create SNP build for testing
cargo build --release --package tee-wasm-runner --features "snp-attester"
cp target/release/tee-wasm-runner target/release/tee-wasm-runner-snp

# Deploy appropriate binary to each environment
```

#### Use Docker Multi-Stage Builds

```dockerfile
# Build stage with multiple platform features
FROM rust:1.85 as builder

# Build with TDX support
RUN cargo build --release --package tee-wasm-runner --features "tdx-attester"

# Create minimal runtime image
FROM alpine:3.19
COPY --from=builder /workspace/target/release/tee-wasm-runner /usr/local/bin/
```

### For CI/CD

```yaml
# .github/workflows/build.yml
name: Build TEE WASM Runner

on: [push, pull_request]

jobs:
  build:
    strategy:
      matrix:
        platform: [tdx, snp, sgx, none]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1

      - name: Build
        run: |
          if [ "${{ matrix.platform }}" = "tdx" ]; then
            cargo build --release --package tee-wasm-runner --features "tdx-attester"
          elif [ "${{ matrix.platform }}" = "snp" ]; then
            cargo build --release --package tee-wasm-runner --features "snp-attester"
          elif [ "${{ matrix.platform }}" = "sgx" ]; then
            cargo build --release --package tee-wasm-runner --features "sgx-attester"
          else
            cargo build --release --package tee-wasm-runner

      - name: Upload artifacts
        uses: actions/upload-artifact@v3
        with:
          name: tee-wasm-runner-${{ matrix.platform }}
          path: target/release/tee-wasm-runner
```

## Testing in TEE

### Verify Attestation Works

After building with TDX support:

```bash
# Run with encrypted image (requires real attestation)
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/user/wasm:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10

# Should see:
# [DEBUG] TDX attester initialized
# [DEBUG] Collecting TDX report from /dev/tdx_guest
# [DEBUG] Evidence obtained: 1500+ bytes (not 44 bytes for sample)
# [INFO] TEE Platform: Tdx
```

### Compare Sample vs Real Attester

| Attester Type | Evidence Size | TEE Platform | Use Case |
|--------------|---------------|---------------|-----------|
| Sample | ~44 bytes | Sample | Development, testing |
| TDX | 1500+ bytes | Tdx | Production TDX |
| SNP | 2000+ bytes | Snp | Production SNP |
| SGX | 800+ bytes | Sgx | Production SGX |

## Troubleshooting

### Still Shows "Sample Attester"

If you built with `--features "tdx-attester"` but still see sample:

**Check 1: Verify build includes feature**
```bash
cargo tree --features tee-wasm-runner
# Should show: tdx-attester in features list
```

**Check 2: Verify binary is actually rebuilt**
```bash
ls -lh target/release/tee-wasm-runner
# Check timestamp
```

**Check 3: Check attestation-agent features**
```bash
cd /path/to/guest-components/attestation-agent
cargo tree --features attestation-agent | grep tdx
```

**Check 4: Check device permissions**
```bash
ls -l /dev/tdx_guest
# Should have proper permissions
# If missing, check TDX kernel module is loaded
lsmod | grep tdx
```

### Build Fails with Unknown Feature

```bash
error: optional feature `tdx-attester` not found
```

**Solution**: Ensure attestation-agent version includes the feature:

```bash
cd /path/to/guest-components/attestation-agent
git pull
cargo tree --features attestation-agent | grep tdx
```

## Recommended Build Strategy

For development in a known TEE environment (like propeller-cvm):

### Per-Developer Setup

Each developer builds with their specific platform:

```bash
# On TDX VM
cargo build --release --package tee-wasm-runner --features "tdx-attester"

# On SNP VM
cargo build --release --package tee-wasm-runner --features "snp-attester"
```

### Deployment Script

Create deployment scripts for different platforms:

```bash
# deploy.sh
#!/bin/bash

PLATFORM=${1:-"auto"}

detect_platform() {
    if [ -e /dev/tdx_guest ]; then
        echo "tdx"
    elif [ -e /dev/sev ]; then
        echo "snp"
    elif [ -e /dev/sgx_enclave ]; then
        echo "sgx"
    else
        echo "none"
    fi
}

if [ "$PLATFORM" = "auto" ]; then
    PLATFORM=$(detect_platform)
fi

echo "Building for platform: $PLATFORM"

case "$PLATFORM" in
    tdx)
        cargo build --release --package tee-wasm-runner --features "tdx-attester"
        ;;
    snp)
        cargo build --release --package tee-wasm-runner --features "snp-attester"
        ;;
    sgx)
        cargo build --release --package tee-wasm-runner --features "sgx-attester"
        ;;
    none)
        cargo build --release --package tee-wasm-runner
        ;;
    *)
        echo "Unknown platform: $PLATFORM"
        exit 1
        ;;
esac

echo "Build complete: target/release/tee-wasm-runner"
```

## Quick Reference

```bash
# Build for TDX (your environment)
cargo build --release --package tee-wasm-runner --features "tdx-attester"

# Run with encrypted image (requires TDX attestation)
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10
```

## Documentation Links

- [WASM_CONFIG_FIX.md](./WASM_CONFIG_FIX.md) - Handle minimal WASM configs
- [ENCRYPTED_WASM_FIX.md](./ENCRYPTED_WASM_FIX.md) - Encrypted WASM handling
- [KBS_SETUP.md](./KBS_SETUP.md) - KBS configuration
- [README.md](./README.md) - Main documentation
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues

## Summary

To fix "No TEE platform detected" warning:

1. ✅ Build with TDX feature: `--features "tdx-attester"`
2. ✅ Verify `/dev/tdx_guest` exists
3. ✅ Verify logs show "TEE Platform: Tdx"
4. ✅ Test with encrypted image to ensure attestation works

**Now your runner will properly detect TDX and use real attestation!** 🎉
