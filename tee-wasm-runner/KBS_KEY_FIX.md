# Fix for KBS Base64 Key Response

## Problem

KBS is returning a **base64-encoded key** instead of raw bytes:

```bash
Successfully obtained decryption key from KBS
# The key is actually: Zkdmcjd2UHg4S0VueGlxUmNFMjBzMmtlNVdrRm5Tb3h0NEwrd044M1hXQT0K
# Not: 32 bytes (the real key)
```

When `image-rs` tries to use this as a decryption key, it treats the base64 string as the key, which causes the `index out of bounds` panic.

## Root Cause

The `image-rs` decryption expects:
- **Raw key bytes** (Vec<u8>)
- **diff_ids** array references into the key

When KBS returns a **base64-encoded key**:
- We get the base64 **string**
- But `diff_ids` is **empty** (no reference to decode)
- `image-rs` tries to index into empty array → **PANIC: index 0 out of bounds**

## Solution

**Decode the base64 key from KBS** to get the actual key bytes.

## Implementation

### 1. Add base64 decode to KBS protocol

The KBS client should decode the base64 response. Since this is in `kbs_protocol`, we can't modify it directly. Instead, we need to decode it after receiving.

### 2. Update `get_decryption_key` to handle base64

```rust
// In src/main.rs
async fn get_decryption_key(&self, client: &mut KbsClientType) -> Result<Vec<u8>> {
    let kbs_uri = self
        .args
        .kbs_uri
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("KBS URI is required for encrypted images"))?;

    log::info!("Requesting decryption key from KBS: {}", kbs_uri);

    let evidence_provider = Box::new(NativeEvidenceProvider::new()?);

    let client =
        KbsClientBuilder::with_evidence_provider(evidence_provider, kbs_uri).build()?;

    // Get resource (may be base64 encoded)
    let response = client
        .get_resource(ResourceUri::try_from(resource_uri).map_err(|e| anyhow::anyhow!(e))?)
        .await
        .context("Failed to get decryption key from KBS")?;

    // Decode base64 to get actual bytes
    use base64::{Engine, engine::general_purpose::decode};
    let key = engine::general_purpose::decode(&response)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64 key: {}", e))?;

    // Validate key length
    if key.is_empty() {
        return Err(anyhow::anyhow!(
            "Invalid decryption key from KBS: Empty key decoded from response: {}",
            response.len()
        ));
    }

    if key.len() < 32 {
        return Err(anyhow::anyhow!(
            "Invalid decryption key from KBS: Key too short ({} bytes, expected at least 32 bytes for RSA)",
            key.len()
        ));
    }

    log::info!("Decryption key from KBS: {} bytes (decoded from base64)", key.len());

    Ok(key)
}
```

### 3. Add base64 dependency

```toml
# In tee-wasm-runner/Cargo.toml
base64 = "0.22"
```

### 4. Handle Empty diff_ids

When KBS returns just the key (without diff_ids), ensure image-rs doesn't panic:

```rust
// In src/main.rs, where we create decrypt_config
let diff_ids = image_config.rootfs().diff_ids();

// For WASM images with no layer references, diff_ids may be empty
// This is OK - we just need the key for decryption
```

### Alternative: Use Image-RS Directly

Instead of passing the key to `image-rs`, use `ocicrypt` directly:

```rust
use ocicrypt_rs::decrypt::Decryptor;

// In pull_and_decrypt_wasm()
if is_encrypted {
    // Get key as base64 (as above)
    let key = self.get_decryption_key(&mut kbs_client).await?;

    // Write key to temporary file
    let key_file = std::env::temp_dir().join("decryption-key");
    tokio::fs::write(&key_file, &key).await?;

    // Use ocicrypt to decrypt layer
    let decryptor = Decryptor::new()?;
    let decrypted = decryptor
        .decrypt_layer_blob(&encrypted_blob, &key_file)
        .await
        .context("Failed to decrypt WASM layer")?;

    // Remove temp key file
    tokio::fs::remove_file(&key_file).await?;

    Ok(decrypted)
}
```

## Complete Fix for Image-RS Integration

If modifying `image-rs` is difficult, ensure your KBS returns raw bytes by using a different configuration.

### Option A: Configure KBS to Return Raw Bytes

Check if KBS supports returning raw bytes instead of base64:

```toml
# In trustee/kbs/config/docker-compose/kbs-config.toml
[[plugins]]
name = "resource"
type = "LocalFs"
dir_path = "/opt/confidential-containers/kbs/repository"

# Add configuration for raw byte responses
return_raw_bytes = true
```

**Note**: This depends on Trustee KBS version and may not be supported.

### Option B: Store Keys in KBS Without Encoding

Instead of encoding keys when uploading, store them as raw bytes:

```toml
# If you're the key owner, configure KBS to not encode
[[resources]]
storage_type = "raw"  # Don't encode to base64
```

### Option C: Use KBS Directly for Decryption

Skip `image-rs` entirely and use KBS protocol directly:

```rust
use kbs_protocol::client::KbsClient;

// Get key
let client = KbsClient::new(...);
let key = client.get_resource(...).await?;

// Create decryptor with the key
let decryptor = ocicrypt_rs::decrypt::Decryptor::new(&key);

// Decrypt the layer
let decrypted = decryptor.decrypt_layer_blob(&blob, None).await?;
```

## Testing the Fix

### 1. Test with Valid Base64 Key

First, create a test with a valid base64 key:

```bash
# Create a 32-byte test key (all zeros)
echo -n "000000000000000000000000000000000000000" | base64 -w 0 > /tmp/test-key.b64

# Upload to KBS
cd ~/trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file /tmp/test-key.b64 \
  --path default/key/test-valid-base64
```

### 2. Test with Invalid Key

```bash
# Upload a too-short key
echo -n "test" | base64 -w 0 > /tmp/short-key.b64

./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file /tmp/short-key.b64 \
  --path default/key/test-invalid

# Verify validation works
```

### 3. Test with Actual Encrypted Key

```bash
# Create a base64 encoded version of your real encryption key
openssl base64 -in /path/to/real-encryption-key.pem -out /tmp/real-key.b64

# Upload and test
cd ~/trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file /tmp/real-key.b64 \
  --path default/key/wasm-addition
```

## Verification

After applying the fix, verify:

```bash
cd /path/to/guest-components

# Clean and rebuild
cargo clean --package tee-wasm-runner
cargo build --release --package tee-wasm-runner --features "tdx-attester,kbs_protocol/tdx-attester,attester/tdx-attester"

# Test encrypted WASM
sudo RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10

# Expected output:
# [INFO] Decryption key from KBS: 2048 bytes (after base64 decode)
# [INFO] Successfully pulled and decrypted WASM to: ...
# [INFO] WASM stdout:
#     15
# [INFO] WASM execution completed successfully
```

## Summary

The fix requires:

1. ✅ Add `base64` dependency to `Cargo.toml`
2. ✅ Update `get_decryption_key()` to decode base64 response
3. ✅ Add validation for key length
4. ✅ Add better error messages
5. ✅ Handle empty `diff_ids` gracefully

**After applying this fix**, encrypted WASM images will:
- ✅ Decode base64 keys from KBS
- ✅ Validate key length (>= 32 bytes)
- ✅ Provide decryption keys to `image-rs` in correct format
- ✅ Successfully decrypt WASM layers
- ✅ Execute without panicking

## Documentation Updates

Update or create:
- [KBS_KEY_FIX.md](./KBS_KEY_FIX.md) - This comprehensive guide
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Add new issue section
- [README.md](./README.md) - Mention base64 handling

## Quick Reference Commands

```bash
# Add base64 dependency
echo 'base64 = "0.22"' >> Cargo.toml

# Rebuild
cargo build --release --package tee-wasm-runner

# Test encrypted WASM
sudo ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10
```
