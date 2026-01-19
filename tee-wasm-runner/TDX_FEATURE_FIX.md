# Correct TDX Feature Build Commands

## Problem

```bash
$ cargo build --release --package tee-wasm-runner --features "tdx-attester"
error: package 'tee-wasm-runner' does not contain this feature: tdx-attester
help: packages with to check which features are available
```

## Root Cause

The `tdx-attester` feature in `attestation-agent` is actually a **composite feature** made of sub-features. To use it, we need to enable all its dependencies.

Looking at attestation-agent's Cargo.toml:

```toml
tdx-attester = [
    "kbs_protocol?/tdx-attester",  # ← This is in kbs_protocol crate
    "attester/tdx-attester"           # ← This is in attester crate
    "attester/tdx-attest-dcap-ioctls"  # ← This is in attester crate
]
```

All these features are **optional** and need to be explicitly enabled.

## Solution 1: Enable All TDX Features (Most Reliable)

Enable all TDX-related features:

```bash
cd /path/to/guest-components

# Enable all TDX features for all involved crates
cargo build --release --package tee-wasm-runner --features "tdx-attester,kbs_protocol/tdx-attester,attester/tdx-attester"
```

### Explanation

- `tdx-attester` - Main composite feature (activates all TDX features)
- `kbs_protocol/tdx-attester` - KBS protocol with TDX support
- `attester/tdx-attester` - TDX attester implementation

### Alternative: Using Workspace Feature Pattern

Some features use `?/` pattern which is workspace-relative. Try:

```bash
# Using workspace-relative paths
cargo build --release --package tee-wasm-runner \
  --features "kbs_protocol?/tdx-attester"
```

## Solution 2: Check Available Features

List all available features:

```bash
cd /path/to/guest-components/attestation-agent
cargo tree --features attestation-agent

# Or
cargo tree --features kbs_protocol

# Or
cargo tree --features attester
```

Look for features related to your TEE platform.

## Solution 3: Try Individual Features

If composite feature doesn't work, try enabling sub-features individually:

```bash
# Try just the kbs_protocol TDX feature
cargo build --release --package tee-wasm-runner \
  --features "kbs_protocol?/tdx-attester"

# Or try the attester feature
cargo build --release --package tee-wasm-runner \
  --features "attester/tdx-attester"
```

## Solution 4: Use Feature Aliases (If Defined)

Check if there are any feature aliases defined:

```bash
cd /path/to/guest-components/attestation-agent

# Check package manifest
cat Cargo.toml | grep -A 10 "^\[package\]"

# Check if there are [features.*] sections
cat Cargo.toml | grep "\[features\.tee-wasm-runner\]"
```

If aliases exist, you might be able to use a shorter name like:

```bash
cargo build --release --package tee-wasm-runner --features "tee-wasm-runner-tdx"
```

## Solution 5: Add Feature Alias (If Needed)

If there's no convenient feature alias, add one to tee-wasm-runner's Cargo.toml:

```toml
# In tee-wasm-runner/Cargo.toml
[features]
default = []
tee-wasm-runner-tdx = [
    "tdx-attester",
    "kbs_protocol?/tdx-attester",
    "attester/tdx-attester"
]
```

Then build with:
```bash
cargo build --release --package tee-wasm-runner --features "tee-wasm-runner-tdx"
```

## Solution 6: Enable in Dependency Directly

If all else fails, you may need to modify the attestation-agent dependency to enable features by default:

```toml
# In tee-wasm-runner/Cargo.toml
[dependencies]
# Try enabling features in attestation-agent directly
attestation-agent = {
    path = "../attestation-agent/attestation-agent",
    default-features = false,
    # Force-enable TDX features (may not work if they're optional)
    # features = ["rust-crypto", "kbs", "ttrpc", "grpc", "tdx-attester"]
}
```

⚠️ **Warning**: This approach may not work if features are truly optional.

## Platform-Specific Feature Lists

Based on the attestation-agent features, here are the available TEE features:

### Intel TDX
```bash
# Composite feature (try first)
cargo build --release --package tee-wasm-runner --features "tdx-attester"

# Or all sub-features
cargo build --release --package tee-wasm-runner \
  --features "kbs_protocol?/tdx-attester,attester/tdx-attester"
```

### AMD SEV-SNP
```bash
cargo build --release --package tee-wasm-runner --features "snp-attester"
cargo build --release --package tee-wasm-runner \
  --features "kbs_protocol?snp-attester,attester/snp-attester"
```

### Intel SGX
```bash
cargo build --release --package tee-wasm-runner --features "sgx-attester"
cargo build --release --package tee-wasm-runner \
  --features "kbs_protocol?sgx-attester,attester/sgx-attester"
```

### Azure SNP vTPM
```bash
cargo build --release --package tee-wasm-runner --features "az-snp-vtpm-attester"
```

### Azure TDX vTPM
```bash
cargo build --release --package tee-wasm-runner --features "az-tdx-vtpm-attester"
```

## Verification

After building, verify the feature was actually enabled:

```bash
# Check if TDX attester was included
strings target/release/tee-wasm-runner | grep -i "tdx\|intel.*tdx"

# Or check build output
cargo build --release --package tee-wasm-runner --features "tdx-attester" 2>&1 | grep -i "compiling.*tdx\|activating.*tdx"
```

And test if TEE is now detected:

```bash
# Run with encrypted image
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10

# Should see: [INFO] TEE Platform: Tdx (not Sample!)
```

## Recommended First Attempt

```bash
# Try the composite feature first
cd ~/guest-components
cargo clean --package tee-wasm-runner
cargo build --release --package tee-wasm-runner --features "tdx-attester" 2>&1

# If it complains about missing features, note which features it mentions
# Then enable those specific sub-features
```

## Troubleshooting

### Feature Not Found

If you see `error: package does not contain this feature`:

1. Check if feature exists:
```bash
cd /path/to/guest-components/attestation-agent
cargo tree --features attestation-agent | grep -i "tdx"
```

2. Check if feature is optional:
```bash
grep -A 5 "tdx-attester" attestation-agent/Cargo.toml | grep optional
```

3. Try enabling in kbs_protocol directly:
```bash
cd /path/to/guest-components/kbs_protocol
cargo tree --features | grep tdx
```

### Build Fails with Sub-features

```bash
# Try simpler approach - just kbs_protocol feature
cargo build --release --package tee-wasm-runner \
  --features "kbs_protocol/tdx-attester"
```

### Still Shows "Sample Attester"

Even after building with TDX features, if you still see "Sample Attester":

1. **Verify `/dev/tdx_guest` exists**:
```bash
ls -l /dev/tdx_guest
```

2. **Check attestation-agent logs**:
```bash
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10 2>&1 | grep -i "attest\|tdx\|platform"
```

3. **Check what attester was actually compiled in**:
```bash
# Check dependency was built with features
cargo tree --features attestation-agent

# Check if tdx-attester was enabled
cargo build --release --package tee-wasm-runner --features "tdx-attester" --verbose
```

## Documentation Links

- [attestation-agent/Cargo.toml](https://github.com/confidential-containers/attestation-agent/blob/main/attestation-agent/Cargo.toml) - Official feature definitions
- [kbs_protocol/Cargo.toml](https://github.com/confidential-containers/attestation-agent/blob/main/kbs_protocol/Cargo.toml) - KBS protocol features
- [attester/Cargo.toml](https://github.com/confidential-containers/attestation-agent/blob/main/attester/Cargo.toml) - Attester features

## Summary

The correct build command for TDX support is:

```bash
cargo build --release --package tee-wasm-runner \
  --features "tdx-attester,kbs_protocol/tdx-attester,attester/tdx-attester"
```

Or try the simpler approach first:
```bash
cargo build --release --package tee-wasm-runner --features "kbs_protocol/tdx-attester"
```

Both should work! If one fails, try the other. ✅
