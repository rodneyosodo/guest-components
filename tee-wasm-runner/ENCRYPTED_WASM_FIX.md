# Fix for Encrypted WASM Images

## Problem

When running with an encrypted WASM image, the runner would:
1. ✅ Detect it's a WASM image
2. ✅ Pull the encrypted blob directly
3. ✅ Write encrypted bytes to disk
4. ✅ Try to execute with wasmtime
5. ❌ **FAIL** because bytes are still encrypted (wasuntime: "input bytes aren't valid utf-8")

```
[INFO] WASM layer: ... (application/vnd.wasm.content.layer.v1+wasm+encrypted)
[INFO] Writing WASM to: "/tmp/tee-wasm-runner/layers/..."
[WARN] Error: failed to parse `...wasm`: input bytes aren't valid utf-8
```

## Root Cause

The code had a logic flaw:

```rust
if is_wasm_image {
    // Pull blob directly
    // Return early ← Never reaches decryption code!
}
```

For **encrypted** WASM images, the direct blob download path would:
- Download encrypted bytes ✅
- Skip decryption ❌
- Return before calling `async_pull_layers` ❌
- Execute encrypted WASM → wasuntime fails ❌

## Solution

Updated the logic to check if WASM image is **encrypted**:

```rust
// Check if this is a WASM image
let is_wasm_image = manifest.config.media_type.contains("wasm")
    || manifest.layers.iter().any(|l| l.media_type.contains("wasm"));

// Check if image is encrypted
let is_encrypted = manifest.layers.iter().any(|l| l.media_type.contains("encrypted"));

log::info!("Image type: {}", if is_wasm_image { "WASM" } else { "OCI" });
log::info!("Image encrypted: {}", is_encrypted);

// For WASM images, download blob directly instead of using layer decompression
// BUT: If encrypted, use standard OCI path to handle decryption
if is_wasm_image && !is_encrypted {
    // Direct blob download for unencrypted WASM
} else {
    // Standard OCI image processing (handles decryption for encrypted images)
    // This includes encrypted WASM images and standard OCI images
}
```

## Updated Behavior

### Unencrypted WASM Image
```
[INFO] Image type: WASM
[INFO] Image encrypted: false
[INFO] Pulling WASM blob directly
[INFO] Successfully pulled WASM to: /tmp/.../...wasm
[INFO] Running WASM with wasmtime runtime
→ Executes successfully ✅
```

### Encrypted WASM Image
```
[INFO] Image type: WASM
[INFO] Image encrypted: true
[INFO] Processing with OCI layer handler (for decryption if encrypted)
[INFO] Detected encrypted layers, setting up KBS client
[INFO] Successfully obtained decryption key from KBS
[INFO] Successfully pulled and decrypted WASM to: /tmp/.../...wasm
[INFO] Running WASM with wasmtime runtime
→ Executes successfully ✅
```

## How to Use

### Build the Fixed Version

```bash
cd /path/to/guest-components
cargo build --release --package tee-wasm-runner
```

### Run with Encrypted WASM Image

```bash
# First, ensure KBS is running and has your encryption key
cd ~/trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file /path/to/encryption-key.pem \
  --path default/key/mykey

# Then run with encrypted WASM image
cd /path/to/guest-components
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

### Expected Output

```
[INFO] Starting TEE WASM Runner...
[INFO] TEE evidence obtained: 44 bytes
[INFO] Pulling image: docker.io/rodneydav/wasm-addition:encrypted
[INFO] Successfully pulled manifest
[INFO] Image type: WASM
[INFO] Image encrypted: true
[INFO] Processing with OCI layer handler (for decryption if encrypted)
[INFO] Detected encrypted layers, setting up KBS client
[INFO] Successfully obtained decryption key from KBS
[INFO] Successfully pulled and decrypted WASM to: /tmp/.../...wasm
[INFO] Running WASM with wasmtime runtime
[INFO] Invoking function: add
[INFO] Executing command: "wasmtime" "--invoke" "add" ...
INFO] WASM stdout:
    15
[INFO] WASM execution completed successfully
```

## Troubleshooting

### Issue: Still getting "input bytes aren't valid utf-8"

**Check 1: Verify you built the latest version**
```bash
cd /path/to/guest-components
git pull
cargo build --release --package tee-wasm-runner
```

**Check 2: Verify image is actually encrypted**
```bash
skopeo inspect docker://docker.io/user/wasm:encrypted
# Look for: "mediaType": "application/vnd.wasm.content.layer.v1+wasm+encrypted"
```

**Check 3: Verify logs show "Image encrypted: true"**
```bash
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/user/wasm:encrypted \
  --kbs-uri http://10.0.2.2:8082
# Should see: [INFO] Image encrypted: true
```

**Check 4: Verify KBS has the decryption key**
```bash
cd ~/trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  get-resource \
  --path default/key/mykey
# Should return the private key content
```

### Issue: "Detected encrypted layers, setting up KBS client" but then fails

**Solution**: KBS client may not be configured correctly. Check:

```bash
# Check aa-config.toml
cat aa-config.toml

# Should have:
# [token_configs]
# [token_configs.coco_kbs]
# url = "http://10.0.2.2:8082"
```

### Issue: "Failed to get decryption key from KBS"

**Solution**:
1. Verify KBS is running on the host
2. Verify port is correct
3. Check [KBS_AUTH_FIX.md](./KBS_AUTH_FIX.md) for authentication issues

## TEE Platform Detection

If you're seeing "Sample Attester will be used" but you're in a TEE:

### Check 1: Verify TDX device exists
```bash
ls -l /dev/tdx_guest
```

### Check 2: Build with TDX support
```bash
cd /path/to/guest-components

# Build with TDX attester
cargo build --release --package tee-wasm-runner --features "tdx-attester"

# Or with SNP support
cargo build --release --package tee-wasm-runner --features "snp-attester"
```

### Check 3: Configure attestation to use real platform

See the attestation-agent documentation for platform-specific configuration.

## Technical Details

{
  "key-providers": {
    "attestation-agent": {
      "grpc": "127.0.0.1:50000"
    }
  }
}

### Image Type Detection Logic

The runner now uses a 2x2 matrix to determine how to handle images:

| Image Type | Encrypted | Path |
|-------------|-----------|------|
| WASM | No | Direct blob download (fast) |
| WASM | Yes | OCI layer handler (with decryption) |
| OCI | No | OCI layer handler |
| OCI | Yes | OCI layer handler (with decryption) |

### Media Type Examples

| Media Type | Encryption | Handler |
|------------|-----------|----------|
| `application/vnd.wasm.content.layer.v1+wasm` | No | Direct download |
| `application/vnd.wasm.content.layer.v1+wasm+encrypted` | Yes | OCI with decryption |
| `application/vnd.oci.image.layer.v1.tar+gzip` | No | OCI layer handler |
| `application/vnd.oci.image.layer.v1.tar+gzip+encrypted` | Yes | OCI with decryption |

## Verification

To verify the fix is working:

```bash
# 1. Test unencrypted WASM (should use fast path)
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/test-unencrypted

# Should see: "Image encrypted: false"
# Should see: "Pulling WASM blob directly"

# 2. Test encrypted WASM (should use decryption path)
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/test-encrypted \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml

# Should see: "Image encrypted: true"
# Should see: "Processing with OCI layer handler"
# Should see: "Successfully pulled and decrypted WASM"
```

## References

- [Issue description](#problem)
- [Root cause](#root-cause)
- [Solution code](#solution)
- [Usage](#how-to-use)
- [Troubleshooting](#troubleshooting)
- [KBS_AUTH_FIX.md](./KBS_AUTH_FIX.md) - KBS authentication issues
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues
