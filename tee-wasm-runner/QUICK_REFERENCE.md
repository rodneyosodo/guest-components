# TEE WASM Runner - Quick Reference Card

## 📋 One-Liner Commands

### Unencrypted WASM Image

```bash
# Basic execution
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/tee-wasm-runner

# With function invocation
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --invoke add --wasm-args 5 --wasm-args 10
```

### Encrypted WASM Image (with KBS)

```bash
# Start KBS first (one-time setup)
git clone https://github.com/confidential-containers/trustee.git && \
cd trustee && docker-compose up -d

# Then run encrypted image
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --kbs-uri http://localhost:8080 \
  --aa-config aa-config.toml \
  --invoke add --wasm-args 100 --wasm-args 250
```

## 🔧 Build Commands

```bash
# Build the runner
cargo build --release --package tee-wasm-runner

# Run tests
./test-runner.sh

# Build examples
./build-examples.sh
```

## 🐳 KBS Docker Compose Commands

```bash
# Start KBS stack
cd trustee && docker-compose up -d

# Stop KBS stack
docker-compose down

# View logs
docker-compose logs -f kbs

# Restart KBS
docker-compose restart kbs
```

## 🔑 KBS Key Management

```bash
# Build kbs-client
cd trustee && make cli

# Set allow-all policy (for Sample Attester)
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config --auth-private-key kbs/config/private.key \
  set-resource-policy --policy-file kbs/sample_policies/allow_all.rego

# Upload encryption key
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config --auth-private-key kbs/config/private.key \
  set-resource --resource-file key.pem --path default/key/mykey

# Verify key exists
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  get-resource --path default/key/mykey
```

## 🔐 Image Encryption

```bash
# Generate keys
openssl genrsa -out private_key.pem 2048
openssl rsa -in private_key.pem -pubout -out public_key.pem

# Encrypt with skopeo
skopeo copy \
  --encryption-key jwe:public_key.pem \
  docker://docker.io/user/wasm:latest \
  docker://docker.io/user/wasm:encrypted

# Or use helper script
./encrypt-image.sh \
  docker.io/user/wasm:latest \
  docker.io/user/wasm:encrypted \
  private_key.pem
```

## 📦 Create WASM OCI Image

```bash
# Install wasm-to-oci
go install github.com/engineerd/wasm-to-oci@latest

# Push WASM to registry
wasm-to-oci push addition.wasm \
  docker.io/user/wasm-addition:latest \
  --server docker.io

# Or use helper script
./create-image.sh addition.wasm latest docker.io/user
```

## 📝 Configuration Files

### aa-config.toml
```toml
[token_configs]
[token_configs.coco_kbs]
url = "http://localhost:8080"
```

## 🐛 Troubleshooting Commands

```bash
# Check KBS is running
curl http://localhost:8080/kbs/v0/auth

# View KBS logs
docker-compose logs -f kbs

# Check if key exists in KBS
kbs-client --url http://127.0.0.1:8080 \
  get-resource --path default/key/mykey

# Run with debug logging
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/user/wasm:latest

# Verify WASM file is valid
file /tmp/tee-wasm-runner/layers/*.wasm

# Check wasmtime is installed
wasmtime --version
```

## 📊 Common CLI Arguments

| Argument | Short | Required | Default | Example |
|----------|-------|----------|---------|---------|
| `--image-reference` | `-i` | Yes | - | `docker.io/user/wasm:latest` |
| `--work-dir` | `-w` | No | `/tmp/tee-wasm-runner` | `/tmp/mydir` |
| `--invoke` | - | No | - | `add` |
| `--wasm-args` | - | No | `[]` | `5 10` |
| `--kbs-uri` | `-k` | No* | - | `http://localhost:8080` |
| `--aa-config` | `-a` | No* | - | `aa-config.toml` |

\* Required for encrypted images

## 🧪 Test Commands

```bash
# Run full test suite
./test-runner.sh

# Run with specific image
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --invoke add --wasm-args 5 --wasm-args 10

# Expected output: 15
```

## 🔗 Quick Links

| Document | Purpose |
|----------|---------|
| [README.md](./README.md) | Main documentation |
| [KBS_SETUP.md](./KBS_SETUP.md) | KBS Docker Compose setup |
| [QUICKSTART.md](./QUICKSTART.md) | 5-minute guide |
| [ENCRYPTING_WASM_GUIDE.md](./ENCRYPTING_WASM_GUIDE.md) | Complete encryption guide |
| [SUMMARY.md](./SUMMARY.md) | Project overview |

## 🎯 Common Workflows

### Workflow 1: Test Unencrypted Image
```bash
# 1. Build runner
cargo build --release --package tee-wasm-runner

# 2. Run image
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --invoke add --wasm-args 5 --wasm-args 10
```

### Workflow 2: Create and Push WASM Image
```bash
# 1. Compile WASM
wat2wasm addition.wat -o addition.wasm

# 2. Push to registry
wasm-to-oci push addition.wasm \
  docker.io/user/wasm-addition:latest \
  --server docker.io

# 3. Test
./target/release/tee-wasm-runner \
  --image-reference docker.io/user/wasm-addition:latest
```

### Workflow 3: Encrypt and Run Image
```bash
# 1. Generate keys
openssl genrsa -out key.pem 2048
openssl rsa -in key.pem -pubout -out key.pub

# 2. Encrypt image
skopeo copy \
  --encryption-key jwe:key.pub \
  docker://docker.io/user/wasm:latest \
  docker://docker.io/user/wasm:encrypted

# 3. Start KBS
cd trustee && docker-compose up -d && make cli

# 4. Configure KBS
./target/release/kbs-client --url http://127.0.0.1:8080 \
  config --auth-private-key kbs/config/private.key \
  set-resource-policy --policy-file kbs/sample_policies/allow_all.rego

./target/release/kbs-client --url http://127.0.0.1:8080 \
  config --auth-private-key kbs/config/private.key \
  set-resource --resource-file ../key.pem --path default/key/mykey

# 5. Run encrypted image
cd /path/to/guest-components
./target/release/tee-wasm-runner \
  --image-reference docker.io/user/wasm:encrypted \
  --kbs-uri http://localhost:8080 \
  --aa-config aa-config.toml
```

## 🚨 Error Messages

| Error | Solution |
|-------|----------|
| `wasmtime: command not found` | Install: `sudo apt-get install wasmtime` |
| `Failed to pull manifest: Not authorized` | Check registry credentials or use public image |
| `Failed to get decryption key from KBS` | Verify KBS is running and key is uploaded |
| `Connection refused` | Check KBS is accessible: `curl http://localhost:8080/kbs/v0/auth` |
| `Sample attester rejected` | Set allow-all policy in KBS |
| `unhandled media type` | Image might not be WASM format |

## 📞 Getting Help

1. Check the [README.md](./README.md) for detailed documentation
2. See [KBS_SETUP.md](./KBS_SETUP.md) for KBS configuration issues
3. Run tests: `./test-runner.sh`
4. Enable debug logs: `RUST_LOG=debug`
5. Open an issue in the guest-components repository

---

**Print this page for quick reference while developing!**
