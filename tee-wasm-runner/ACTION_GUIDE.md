# Quick Action Guide for Current Issues

## Current Situation

You're experiencing multiple issues:

1. ❌ **TEE Platform Not Detected** - Shows "Sample Attester" despite being in TDX
2. ❌ **KBS Protocol Mismatch** - Runner trying RCAR, KBS expects Background Check
3. ✅ **Encrypted WASM Fix Applied** - Runner now handles encrypted images correctly
4. ✅ **WASM Config Fix Applied** - Runner handles minimal WASM configs

## Immediate Actions

### Step 1: Fix TEE Detection

```bash
cd ~/guest-components

# Rebuild with TDX support (enable all sub-features)
cargo clean --package tee-wasm-runner
cargo build --release --package tee-wasm-runner \
  --features "tdx-attester,kbs_protocol/tdx-attester,attester/tdx-attester"

# Verify TDX is detected
ls -l /dev/tdx_guest
```

**See [TDX_FEATURE_FIX.md](./TDX_FEATURE_FIX.md) for detailed TDX build instructions** if this fails.

### Step 2: Fix KBS Protocol

**Choose ONE of these options:**

#### Option A: Reconfigure Trustee KBS (Recommended)

```bash
cd ~/trustee

# Stop current services
docker-compose down

# Update KBS config to use background check mode
cat > kbs/config/docker-compose/kbs-config.toml << 'EOF'
[http_server]
sockets = ["0.0.0.0:8080"]
insecure_http = true

[attestation_service]
type = "coco_as_grpc"
as_addr = "http://as:50004"
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

# Restart services
docker-compose up -d
sleep 10

# Verify KBS is running
curl http://localhost:8082/kbs/v0/auth
```

#### Option B: Use Local Filesystem Keys (Simplest for Testing)

```bash
# Bypass KBS entirely and use local keys
mkdir -p /tmp/kbs-keys/default/key
cp /path/to/encryption-key.pem /tmp/kbs-keys/default/key/wasm-addition

# Update aa-config.toml to use filesystem instead of KBS
cat > aa-config.toml << 'EOF'
[attestation]
type = "file"

[key_providers]
type = "local"
path = "/tmp/kbs-keys"

[resources]
id = "default/key/wasm-addition"
path = "/tmp/kbs-keys/default/key/wasm-addition"
EOF
```

**Then run WITHOUT --kbs-uri:**
```bash
cd /path/to/guest-components

# Run with local keys (no KBS)
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

### Step 3: Test Encrypted WASM

```bash
cd /path/to/guest-components

# Run with encrypted image
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

**Expected Output (if working):**
```
[INFO] TEE Platform: Tdx  ← Should show Tdx!
[INFO] Image encrypted: true
[INFO] Successfully obtained decryption key from KBS
[INFO] Successfully pulled and decrypted WASM to: /tmp/...
[INFO] WASM stdout:
    15
[INFO] WASM execution completed successfully
```

## What Should Happen After Fixes

### With TDX + KBS (Background Check Mode)

```
✅ TEE Platform: Tdx (not Sample)
✅ Image encrypted: true
✅ KBS client connects successfully
✅ Decryption key obtained from KBS
✅ WASM layer decrypted
✅ WASM executed with result: 15
```

### With Local Filesystem Keys

```
✅ No KBS connection needed
✅ Key loaded from filesystem
✅ WASM layer decrypted
✅ WASM executed with result: 15
```

## Verification Checklist

### TEE Detection
- [ ] Rebuilt with `--features "tdx-attester"`
- [ ] `/dev/tdx_guest` device exists
- [ ] Logs show `TEE Platform: Tdx`
- [ ] Evidence size > 100 bytes (real TEE, not sample's 44)

### KBS Connection
- [ ] KBS is running: `docker-compose ps kbs`
- [ ] KBS responds: `curl http://10.0.2.2:8082/kbs/v0/auth`
- [ ] No RCAR errors in logs
- [ ] Client mode matches server mode

### Encrypted WASM Execution
- [ ] `Image encrypted: true` in logs
- [ ] `Successfully obtained decryption key` in logs
- [ ] `Successfully pulled and decrypted WASM` in logs
- [ ] No panic or index out of bounds errors
- [ ] WASM execution completes with exit code 0

## Quick Reference Commands

### Rebuild with TDX Support
```bash
cargo build --release --package tee-wasm-runner --features "tdx-attester"
```

### Restart Trustee KBS
```bash
cd ~/trustee
docker-compose restart kbs
```

### Test Unencrypted Image (Baseline)
```bash
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/tee-wasm-runner \
  --invoke add --wasm-args 5 --wasm-args 10
# Should output: 15
```

### Test Encrypted Image with Local Keys
```bash
# Setup keys
mkdir -p /tmp/kbs-keys/default/key
cp /path/to/key.pem /tmp/kbs-keys/default/key/wasm-addition

# Config
cat > aa-config.toml << 'EOF'
[attestation]
type = "file"

[key_providers]
type = "local"
path = "/tmp/kbs-keys"

[resources]
id = "default/key/wasm-addition"
path = "/tmp/kbs-keys/default/key/wasm-addition"
EOF

# Run (no --kbs-uri)
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10
```

## Recommended Path

**For Development/Testing in TDX:**

1. ✅ Use local filesystem keys (Option 2 above)
   - Simpler to set up
   - No KBS protocol mismatch
   - Works for testing

2. ✅ Or reconfigure Trustee KBS (Option 1 above)
   - Requires understanding KBS config
   - Matches production setup
   - More complex but proper

**For Production:**

1. Use properly configured KBS
2. Ensure both client and server use same protocol mode
3. Use real TDX attestation
4. Test thoroughly before deployment

## Documentation Links

- [KBS_PROTOCOL_FIX.md](./KBS_PROTOCOL_FIX.md) - Detailed protocol mismatch guide
- [TDX_BUILD.md](./TDX_BUILD.md) - Build with TDX support
- [KBS_SETUP.md](./KBS_SETUP.md) - KBS Docker Compose setup
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues and solutions
- [ENCRYPTED_WASM_FIX.md](./ENCRYPTED_WASM_FIX.md) - Encrypted WASM handling
- [WASM_CONFIG_FIX.md](./WASM_CONFIG_FIX.md) - WASM config handling

## Still Having Issues?

1. **Enable debug logging:**
```bash
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 5 --wasm-args 10 2>&1 | tee runner-debug.log
```

2. **Check KBS logs:**
```bash
cd ~/trustee
docker-compose logs kbs | tail -50
```

3. **Verify all fixes applied:**
```bash
# Check runner was rebuilt
ls -lh target/release/tee-wasm-runner
cargo tree --features tee-wasm-runner | grep tdx

# Check TDX device exists
ls -l /dev/tdx_guest

# Check aa-config.toml
cat aa-config.toml
```

4. **Try with unencrypted image first:**
```bash
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/tee-wasm-runner \
  --invoke add --wasm-args 5 --wasm-args 10

# This should work and helps verify basic functionality
```

## Next Steps

Choose your path:

**Path A: Use Local Keys (Easiest)**
```bash
# Follow "Option B: Use Local Filesystem Keys" above
# Rebuild with TDX support
# Run without --kbs-uri
```

**Path B: Fix KBS (Proper Production)**
```bash
# Follow "Option A: Reconfigure Trustee KBS" above
# Rebuild with TDX support
# Test encrypted images
```

**Path C: Alternative KBS Setup**
```bash
# Use Trustee's background check mode CLI
cd ~/trustee
make cli
make background-check-kbs
```

---

**Pick one path and work through the steps systematically!** 🚀
