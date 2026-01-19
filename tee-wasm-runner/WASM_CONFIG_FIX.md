# WASM Image Config Fix

## Problem

When processing WASM images through the OCI path, the runner fails with:
```
ERROR tee_wasm_runner] Error running TEE WASM runner: Failed to parse image config
    Caused by:
        0: serde failed
        1: missing field `architecture` at line 1 column 2
```

## Root Cause

WASM images created by `wasm-to-oci` often have minimal/empty configs that don't conform to the OCI Image Configuration specification. For example:

```json
{}
```

This is valid for `wasm-to-oci` but causes `ImageConfiguration::from_reader()` to fail because it expects required fields like `architecture`, `os`, etc.

## Solution

Updated the code to handle config parsing failures gracefully by using `unwrap_or_else()`:

```rust
// Use default config if parsing fails (common for WASM images)
let image_config = ImageConfiguration::from_reader(config.as_bytes())
    .unwrap_or_else(|e| {
        log::warn!("Failed to parse image config (may be minimal WASM config): {}", e);
        log::info!("Using default OCI configuration for WASM image");
        ImageConfiguration::default()
    });
```

This allows WASM images with invalid configs to proceed through the OCI layer processing path.

## When This Happens

The runner will:
1. Log a warning: `"Failed to parse image config (may be minimal WASM config)"`
2. Log info: `"Using default OCI configuration for WASM image"`
3. Continue with OCI layer processing (which includes decryption support for encrypted images)

## Expected Behavior

### Unencrypted WASM Image (with minimal config)
```
[INFO] Processing with OCI layer handler (for decryption if encrypted)
[WARN] Failed to parse image config (may be minimal WASM config): ...
[INFO] Using default OCI configuration for WASM image
[INFO] Successfully pulled and decrypted WASM to: /tmp/.../...wasm
[INFO] Running WASM with wasmtime runtime
→ Executes successfully ✅
```

### Encrypted WASM Image (with minimal config)
```
[INFO] Processing with OCI layer handler (for decryption if encrypted)
[WARN] Failed to parse image config (may be minimal WASM config): ...
[INFO] Using default OCI configuration for WASM image
[INFO] Detected encrypted layers, setting up KBS client
[INFO] Successfully obtained decryption key from KBS
[INFO] Successfully pulled and decrypted WASM to: /tmp/.../...wasm
[INFO] Running WASM with wasmtime runtime
→ Executes successfully ✅
```

### Standard OCI Image (with valid config)
```
[INFO] Processing with OCI layer handler (for decryption if encrypted)
[INFO] Successfully pulled and decrypted WASM to: /tmp/.../...wasm
→ No warning (config parses successfully)
```

## Using the Fix

### 1. Build the Updated Runner

```bash
cd /path/to/guest-components
cargo build --release --package tee-wasm-runner
```

### 2. Run with Encrypted WASM Image

```bash
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

### 3. Verify in Your TEE

You should now see:
```
[INFO] Image encrypted: true
[INFO] Processing with OCI layer handler (for decryption if encrypted)
[WARN] Failed to parse image config (may be minimal WASM config): ...
[INFO] Using default OCI configuration for WASM image
[INFO] Detected encrypted layers, setting up KBS client
[INFO] Successfully obtained decryption key from KBS
[INFO] Successfully pulled and decrypted WASM to: /tmp/.../...wasm
[INFO] Running WASM with wasmtime runtime
[INFO] WASM stdout:
    15
[INFO] WASM execution completed successfully
```

## Technical Details

### ImageConfiguration Specification

The OCI Image Configuration specification requires certain fields:

```json
{
  "architecture": "amd64",     // Required
  "os": "linux",               // Required
  "config": {
    "Env": [...],
    "Cmd": [...],
    "Entrypoint": [...]
  },
  "rootfs": {
    "type": "layers",
    "diff_ids": [...]
  }
}
```

WASM images from `wasm-to-oci` often have just `{}` which causes `ImageConfiguration::from_reader()` to fail.

### Our Fix Approach

Instead of failing when config is invalid:
- Try to parse config with `ImageConfiguration::from_reader()`
- If it fails, log a warning and use `ImageConfiguration::default()`
- Continue processing because the runner can handle layers without a perfect config

This is acceptable because:
1. The config is primarily needed for standard OCI images
2. WASM images don't typically rely on config fields like `Entrypoint`
3. The layer processing (pulling, decryption, execution) works fine
4. For encrypted images, the decryption process doesn't depend on config

## Why This Works

### For Unencrypted WASM
- Config is invalid → Use default → Pull layers → Execute ✅

### For Encrypted WASM
- Config is invalid → Use default → Pull encrypted layers → Decrypt → Execute ✅

### For Standard OCI
- Config is valid → Parse successfully → Pull layers → Execute ✅

## Alternative Solutions

### Option 1: Create WASM Images with Valid Config

Use tools that create proper OCI configs instead of `wasm-to-oci`:

```bash
# Using oci-cli (experimental)
oci-cli push addition.wasm docker.io/user/wasm:latest

# Or manually create manifest with valid config
```

### Option 2: Patch wasm-to-oci

Fork `wasm-to-oci` and add proper OCI config generation.

### Option 3: Accept Minimal WASM Configs (Current Solution)

Our current fix is the simplest and least intrusive - it just handles the error gracefully without breaking functionality.

## Related Documentation

- [ENCRYPTED_WASM_FIX.md](./ENCRYPTED_WASM_FIX.md) - Fixed encrypted WASM handling
- [KBS_SETUP.md](./KBS_SETUP.md) - KBS setup guide
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues

## Verification

To verify the fix works:

```bash
# Test with unencrypted WASM (should handle minimal config)
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/test1

# Test with encrypted WASM (should handle minimal config + decrypt)
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/test2 \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10
```

Both should now execute successfully! ✅
