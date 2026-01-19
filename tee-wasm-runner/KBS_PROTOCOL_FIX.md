# KBS Protocol Mismatch - RCAR vs Background Check

## Problem

When running TEE WASM Runner with Trustee KBS Docker Compose stack, you see:

```
WARN kbs_protocol::client::rcar_client] Authenticating with KBS failed...
ErrorInformation { "Attestation Token not found" }
INFO] Successfully obtained decryption key from KBS
thread 'main' panicked at .../image-rs/src/pull.rs:137:21:
index out of bounds: len is 0 but index is 0
```

## Root Cause

The TEE WASM Runner's KBS client (`kbs_protocol`) and Trustee KBS server are using **different protocol modes**:

| Mode | Description | KBS Behavior |
|-------|-------------|----------------|
| **Background Check** | Direct resource requests | Simple KBS server |
| **RCAR (Passport)** | Token → Resource | Separate Issuer + Resource KBS |

The runner is trying to use **RCAR** mode (passport):
1. Attempts to get attestation token first
2. Gets "Token not found" error
3. Somehow gets empty key (0 bytes)
4. image-rs panics when trying to decrypt with empty key

But the Trustee KBS Docker Compose stack is configured as:
- **KBS** service (single service)
- **AS** service (gRPC Attestation Service)
- This should support **Background Check** mode

## Why This Happens

The `kbs_protocol` crate's `KbsClientBuilder` auto-detects which protocol to use based on the KBS server's advertised capabilities. It seems to be incorrectly choosing **RCAR** mode when it should use **Background Check** mode.

## Solution 1: Configure KBS for Background Check Mode (Recommended)

Update the Trustee KBS configuration to explicitly use background check mode:

```bash
cd ~/trustee

# Edit KBS config
cat > kbs/config/docker-compose/kbs-config.toml << 'EOF'
[http_server]
sockets = ["0.0.0.0:8080"]
insecure_http = true

[attestation_service]
type = "coco_as_grpc"
as_addr = "http://as:50004"

# Explicitly use background check (not passport/RCAR)
attestation_type = "background_check"

[admin]
type = "Simple"

[[admin.personas]]
id = "admin"
public_key_path = "/opt/confidential-containers/kbs/user-keys/public.pub"

[[plugins]]
name = "resource"
type = "LocalFs"
dir_path = "/opt/confidential-containers/kbs/repository"
EOF

# Restart KBS to apply config
docker-compose restart kbs
sleep 5
```

Then try running the runner again:
```bash
cd /path/to/guest-components
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10
```

## Solution 2: Use Simple Local KBS (Alternative)

If the Trustee KBS is too complex, use a simple filesystem-based KBS:

### Create Simple KBS Config

```bash
mkdir -p ~/simple-kbs/config ~/simple-kbs/resources

# Create simple KBS that just serves files from filesystem
cat > ~/simple-kbs/config/kbs-config.toml << 'EOF'
[http_server]
sockets = ["0.0.0.0:8083"]
insecure_http = true

[attestation_token]
# For testing with sample attester
trusted_certs_paths = []

[[plugins]]
name = "resource"
type = "LocalFs"
dir_path = "/opt/confidential-containers/kbs/repository"
EOF

# Store your encryption key
mkdir -p ~/simple-kbs/repository/default/key
cp /path/to/encryption-key.pem ~/simple-kbs/repository/default/key/wasm-addition

# Use simple KBS server from kbs repo
# (This may not work with docker-compose setup)
```

### Update Runner Config

```bash
cd /path/to/guest-components

# Update aa-config.toml to point to simple KBS
cat > aa-config.toml << 'EOF'
[token_configs]
[token_configs.coco_kbs]
url = "http://localhost:8083"
EOF
```

## Solution 3: Use KBS in Background Check Mode (from Trustee)

Run Trustee's KBS in background check mode instead of the docker-compose stack:

```bash
cd ~/trustee

# Stop docker-compose stack
docker-compose down

# Build and run KBS in background check mode
make background-check-kbs

# This runs KBS with embedded attestation service
# and supports direct resource requests
```

Check Trustee documentation for background check mode:
```bash
cd ~/trustee
make cli
make background-check-kbs
```

## Solution 4: Use Offline Filesystem KBS (for Testing)

For testing without a network KBS, use the attestation-agent's built-in filesystem support:

### Update aa-config.toml

```bash
cat > aa-config.toml << 'EOF'
[attestation]
type = "file"

[key_providers]
# Use filesystem key provider instead of KBS
type = "local"
path = "/path/to/keys"

[resources]
# Resource definitions
EOF
```

This bypasses KBS entirely and reads keys from local filesystem.

## Troubleshooting

### Check KBS Server Mode

```bash
cd ~/trustee

# Check KBS logs to see what mode it's in
docker-compose logs kbs | grep -i "mode\|passport\|background"

# Check KBS config
cat kbs/config/docker-compose/kbs-config.toml
```

### Verify Client-Server Compatibility

```bash
# Check what protocol client is trying to use
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --kbs-uri http://10.0.2.2:8082

# Look for:
# - "Using RCAR client" (wrong mode)
# - "Using background check client" (correct mode)
```

### Check KBS Server Capabilities

```bash
# Query KBS to see what it supports
curl http://10.0.2.2:8082/kbs/v0/resource/default/key/test

# If it returns auth challenge, it expects RCAR/passport
# If it returns resource (or error about attestation), it's background check
```

## Recommended Approach

For TEE WASM Runner with Trustee KBS stack:

### Option A: Reconfigure Trustee KBS (Best for Production)

1. Update KBS config to use `attestation_type = "background_check"`
2. Restart KBS services
3. Rebuild runner with TDX support
4. Test with encrypted image

### Option B: Use Separate Issuer + Resource KBS (Passport Mode)

If you prefer to use passport/RCAR mode:

1. Deploy Issuer KBS (port 50001)
2. Deploy Resource KBS (port 50002)
3. Configure runner to use issuer URL first
4. Use RCAR-compatible KBS client

See Trustee documentation for passport mode setup.

### Option C: Use Simple Local KBS (Best for Testing)

1. Use the Trustee CLI to manage resources locally
2. Or use the attestation-agent filesystem provider
3. Skip KBS entirely for encrypted image testing

## Technical Details

### Background Check Mode Flow

```
Runner                    KBS (Single Service)
  |                              |
  |  1. Get Evidence              |
  |  2. Request Resource  ----->  Check Attestation ----> Return Resource
  |  3. Get Decrypted Layer        |
  v                              |
```

### RCAR/Passport Mode Flow

```
Runner                  Issuer KBS            Resource KBS
  |                        |                          |
  |  1. Get Token    ---->  Validate TEE      ---->  Return Token
  |                        |                          |
  |  2. Use Token      ---->  Get Resource
  |                        |
  v                        v
```

## Quick Fix Summary

**For immediate testing**, try:

1. **Reconfigure KBS** to use background check mode (Solution 1)
2. **Or use local filesystem** keys instead of KBS (Solution 4)
3. **Check documentation** for your Trustee KBS version

**For production**, ensure:

1. KBS and runner use compatible protocol modes
2. Attation agent is built with correct features
3. TEE platform is properly detected (TDX feature)

## Related Documentation

- [KBS_SETUP.md](./KBS_SETUP.md) - KBS Docker Compose setup
- [TDX_BUILD.md](./TDX_BUILD.md) - Building with TDX support
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues
- [Trustee Documentation](https://github.com/confidential-containers/trustee/blob/main/kbs/quickstart.md)

## Getting Help

If none of these solutions work:

1. Check Trustee KBS version:
```bash
cd ~/trustee
git log --oneline -1
```

2. Check Trustee documentation:
```bash
cd ~/trustee
ls kbs/docs/
```

3. Try running KBS with debug:
```bash
cd ~/trustee
RUST_LOG=debug docker-compose up kbs
```

4. Check kbs_protocol crate documentation:
```bash
cd /path/to/guest-components/attestation-agent/kbs_protocol
ls docs/
```

5. Open an issue with:
   - Full error messages
   - KBS version
   - Runner version
   - Steps to reproduce
