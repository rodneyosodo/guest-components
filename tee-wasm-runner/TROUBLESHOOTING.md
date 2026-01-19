# Troubleshooting Guide

## Common Issues and Solutions

### KBS Issues

#### 1. "Plugin auth not found" Error

**Symptom:**
```bash
$ curl http://localhost:8082/kbs/v0/auth
{"type":"https://github.com/confidential-containers/kbs/errors/PluginNotFound","detail":"Plugin auth not found"}
```

**Solution:** This is **NORMAL** and means KBS is running correctly! The `/kbs/v0/auth` endpoint doesn't exist in all KBS configurations. This error message confirms the KBS is accessible.

**How to verify KBS is working:**
```bash
# Check KBS container is running
docker-compose ps kbs

# View KBS logs
docker-compose logs kbs

# Try uploading a resource (requires kbs-client)
cd trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego
```

---

#### 2. Wrong Port Number

**Symptom:**
```bash
$ curl http://localhost:8080/kbs/v0/auth
curl: (7) Failed to connect to localhost port 8080: Connection refused
```

**Solution:** Your KBS may be running on a different port (commonly 8082).

**Check the actual port:**
```bash
docker-compose ps kbs
# Look for the port mapping, e.g., "0.0.0.0:8082->8080/tcp"
```

**Update all commands to use the correct port:**
- If mapped to 8082: use `http://localhost:8082`
- If mapped to 8080: use `http://localhost:8080`

**Update your aa-config.toml:**
```toml
[token_configs]
[token_configs.coco_kbs]
url = "http://localhost:8082"  # Use your actual port
```

---

#### 3. Admin Authentication Error

**Symptom:**
```bash
$ kbs-client set-resource-policy ...
Error: Request Failed, Response: "{\"type\":\"...AdminAuth\",\"detail\":\"Admin auth error: Admin Token could not be verified for any admin persona\"}"
```

**Solution:** The private.key doesn't match the public.pub configured in KBS.

**Quick Fix:**
```bash
cd ~/trustee

# Regenerate keys
docker-compose down
rm -f kbs/config/private.key kbs/config/public.pub kbs/config/*.pem
docker-compose up -d
sleep 15

# Verify keys were created
ls -lh kbs/config/private.key kbs/config/public.pub

# Restart KBS
docker-compose restart kbs
sleep 5

# Try again
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego
```

**For detailed troubleshooting, see [KBS_AUTH_FIX.md](./KBS_AUTH_FIX.md)**

---

#### 4. Sample Attester Rejected

**Symptom:**
```
Error: Failed to get decryption key from KBS
Attestation failed: Sample evidence rejected
```

**Solution:** Set allow-all policy for testing:

```bash
cd trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego
```

**⚠️ Warning:** Only use `allow_all.rego` for testing! In production, use strict attestation policies.

---

### TEE WASM Runner Issues

#### 5. "wasmtime: command not found"

**Symptom:**
```
Error: Failed to execute WASM runtime
wasmtime: command not found
```

**Solution:** Install wasmtime:

```bash
# Option 1: Using package manager
sudo apt-get install wasmtime

# Option 2: Using official installer
curl https://wasmtime.dev/install.sh -sSf | bash
source ~/.bashrc

# Verify installation
wasmtime --version
```

---

#### 6. "Failed to pull manifest: Not authorized"

**Symptom:**
```
Error: Failed to pull manifest: Not authorized
```

**Solutions:**

1. **For private registries**, log in first:
```bash
docker login docker.io
# Enter your credentials
```

2. **For public images**, ensure the image exists:
```bash
# Test with a known-good public image
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest
```

3. **Check image name spelling**

---

#### 7. Decryption Failed

**Symptom:**
```
Error: Failed to decrypt layer
```

**Solutions:**

1. **Verify the encryption key was uploaded to KBS:**
```bash
cd trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  get-resource \
  --path default/key/wasm-addition
```

2. **Ensure public/private keys match:**
```bash
# Extract public key from private key
openssl rsa -in private_key.pem -pubout -out verify_public.pem

# Compare with original public key
diff public_key.pem verify_public.pem
```

3. **Verify image is actually encrypted:**
```bash
skopeo inspect docker://docker.io/user/wasm:encrypted | grep mediaType
# Should contain "encrypted" in the layer media type
```

---

#### 8. "unhandled media type"

**Symptom:**
```
Error: Failed to decode layer: unhandled media type: application/vnd.wasm.content.layer.v1+wasm
```

**Solution:** This error should NOT occur with the current runner. If you see it:

1. **Ensure you're using the latest version:**
```bash
cd /path/to/guest-components
git pull
cargo build --release --package tee-wasm-runner
```

2. **Check the image format:**
```bash
skopeo inspect docker://docker.io/user/wasm:latest
```

---

#### 9. Connection Refused to KBS

**Symptom:**
```
Error: Failed to connect to KBS: Connection refused
```

**Solutions:**

1. **Check KBS is running:**
```bash
docker-compose ps kbs
```

2. **Verify the port:**
```bash
docker-compose ps kbs | grep -o "0.0.0.0:[0-9]*"
```

3. **If running runner in Docker container:**
   - Use: `--kbs-uri http://kbs:8080` (container name)

4. **If running runner on host:**
   - Use: `--kbs-uri http://localhost:8082` (host port)

5. **Check firewall:**
```bash
sudo ufw status
# If active, allow the KBS port:
sudo ufw allow 8082/tcp
```

---

### Docker Compose Issues

#### 10. Services Won't Start

**Symptom:**
```bash
$ docker-compose up -d
Error: ... port already allocated
```

**Solution:** Another service is using the port:

```bash
# Find what's using the port
sudo netstat -tlnp | grep 8082

# Kill the process
sudo kill <PID>

# Or change the port in docker-compose.yml
# Edit: ports: - "8083:8080"  # Use port 8083 instead
```

---

#### 11. Permission Denied

**Symptom:**
```
Error: Got permission denied while trying to connect to the Docker daemon socket
```

**Solution:**

```bash
# Add user to docker group
sudo usermod -aG docker $USER

# Apply group changes
newgrp docker

# Or use sudo
sudo docker-compose up -d
```

---

### Image Creation Issues

#### 12. wasm-to-oci Not Found

**Symptom:**
```
wasm-to-oci: command not found
```

**Solution:** Install wasm-to-oci:

```bash
# Install Go (if not installed)
sudo apt-get install golang-go

# Install wasm-to-oci
go install github.com/engineerd/wasm-to-oci@latest

# Add Go bin to PATH
export PATH=$PATH:$(go env GOPATH)/bin
echo 'export PATH=$PATH:$(go env GOPATH)/bin' >> ~/.bashrc

# Verify
wasm-to-oci --version
```

---

#### 13. skopeo Not Found

**Symptom:**
```
skopeo: command not found
```

**Solution:**

```bash
sudo apt-get update
sudo apt-get install skopeo

# Verify
skopeo --version
```

---

## Debugging Tips

### Enable Debug Logging

```bash
# For TEE WASM Runner
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/user/wasm:latest

# For KBS
docker-compose logs -f kbs

# For all services
docker-compose logs -f
```

### Check File Integrity

```bash
# Verify WASM file is valid
file /tmp/tee-wasm-runner/layers/*.wasm
# Should show: "WebAssembly (wasm) binary module"

# Check file size
ls -lh /tmp/tee-wasm-runner/layers/
```

### Test Components Separately

```bash
# 1. Test wasmtime directly
wasmtime addition.wasm

# 2. Test image pull (without runner)
skopeo copy docker://docker.io/user/wasm:latest oci:./test-image

# 3. Test KBS (without runner)
cd trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  get-resource --path default/key/test
```

### Verify Network Connectivity

```bash
# Check DNS resolution
nslookup docker.io

# Check registry connectivity
curl -v https://index.docker.io/v2/

# Check KBS connectivity
curl -v http://localhost:8082/
```

---

## Getting More Help

If none of these solutions work:

1. **Collect debug information:**
```bash
# System info
uname -a
docker --version
docker-compose --version

# Service status
docker-compose ps

# Logs
docker-compose logs > kbs-logs.txt

# Runner output
RUST_LOG=debug ./target/release/tee-wasm-runner ... 2>&1 | tee runner-debug.log
```

2. **Check documentation:**
   - [README.md](./README.md)
   - [KBS_SETUP.md](./KBS_SETUP.md)
   - [ENCRYPTING_WASM_GUIDE.md](./ENCRYPTING_WASM_GUIDE.md)

3. **Run the test suite:**
```bash
./test-runner.sh
```

4. **Open an issue:**
   - Repository: https://github.com/confidential-containers/guest-components
   - Include: logs, error messages, system info, steps to reproduce

---

## Quick Reference: Port Mappings

| Service | Internal Port | Common Host Port | Protocol |
|---------|---------------|------------------|----------|
| KBS | 8080 | 8080 or 8082 | HTTP |
| AS | 50004 | 50004 | gRPC |
| RVPS | 50003 | 50003 | gRPC |
| Keyprovider | 50000 | 50000 | gRPC |

**Always check actual ports with:** `docker-compose ps`
