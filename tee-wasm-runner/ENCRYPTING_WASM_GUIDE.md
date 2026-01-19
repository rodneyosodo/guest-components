# Guide: Creating, Encrypting, and Running WASM Images with TEE WASM Runner

This guide walks you through creating a WASM module, packaging it as an OCI image, encrypting it, uploading to a registry, and running it inside a TEE using the TEE WASM Runner.

> **Quick Reference**: For detailed KBS setup with Docker Compose, see **[KBS_SETUP.md](./KBS_SETUP.md)**

## Prerequisites

- Rust and Cargo
- `wasm-to-oci` for creating WASM OCI images
- `wat2wasm` (from WABT) for compiling WAT files
- `skopeo` for image encryption
- Access to a container registry (Docker Hub, GHCR, etc.)

### Install Tools

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install wasm-to-oci (recommended for WASM OCI images)
go install github.com/engineerd/wasm-to-oci@latest

# Install WABT tools (includes wat2wasm)
sudo apt-get install wabt

# Install skopeo (for encryption)
sudo apt-get install skopeo

# Build tee-wasm-runner
cd /path/to/guest-components
cargo build --release --package tee-wasm-runner
```

## Step 1: Create a WASM Module

### Option A: Using Rust with wasm-pack

```bash
# Create a new Rust project
cargo new wasm-addition --lib
cd wasm-addition

# Add dependencies
cat >> Cargo.toml << 'EOF'
[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-bindgen = "0.2"
EOF

# Create the WASM library
cat > src/lib.rs << 'EOF'
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[wasm_bindgen(start)]
pub fn start() {
    // Entry point for WASI compatibility
}
EOF

# Build WASM
wasm-pack build --target web --out-dir pkg
```

### Option B: Using WebAssembly Text (WAT)

Create a simple addition WASM module:

```wat
;; addition.wat
(module
  (func $add (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add)
  (export "add" (func $add))
)
```

Compile to binary:

```bash
wat2wasm addition.wat -o addition.wasm
```

### Option C: Using C/C++ with Emscripten

```bash
# Install emscripten
git clone https://github.com/emscripten-core/emsdk.git
cd emsdk
./emsdk install latest
./emsdk activate latest
source ./emsdk_env.sh

# Compile C to WASM
cat > addition.c << 'EOF'
int add(int a, int b) {
    return a + b;
}
EOF

emcc addition.c -o addition.wasm -s EXPORTED_FUNCTIONS='["_add"]' -s SIDE_MODULE=1
```

## Step 2: Package WASM as OCI Image

### Using wasm-to-oci (Recommended)

```bash
# Push WASM module directly to registry
wasm-to-oci push addition.wasm docker.io/rodneydav/wasm-addition:latest --server docker.io

# Or use the helper script
./create-image.sh addition.wasm latest docker.io

# Verify
wasm-to-oci pull docker.io/rodneydav/wasm-addition:latest
```

### Alternative: Using Dockerfile

```bash
mkdir wasm-image && cd wasm-image

# Copy your WASM module
cp ../addition.wasm .

# Create Dockerfile
cat > Dockerfile << 'EOF'
FROM scratch

# Set appropriate media type for WASM layers
LABEL org.opencontainers.image.title="wasm-addition"
LABEL org.opencontainers.image.description="Simple WASM addition module"

# Add WASM layer
ADD addition.wasm /
EOF

# Build image
docker build -t localhost/wasm-addition:latest .
```

"mediaType": "application/vnd.oci.image.manifest.v1+json",
"config": {
"mediaType": "application/vnd.oci.image.config.v1+json",
"digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
"size": 2
},
"layers": [
{
"mediaType": "application/vnd.wasm.content.layer.v1+wasm",
"digest": "sha256:$(sha256sum addition.wasm | cut -d' ' -f1)",
"size": $(stat -c%s addition.wasm)
}
]
}
EOF

# Create config (empty for scratch images)

echo '{}' > config.json

# Calculate config digest

CONFIG_DIGEST=$(sha256sum config.json | cut -d' ' -f1)
mv config.json blobs/sha256/$CONFIG_DIGEST

### Using crane (simpler approach)

```bash
# Create a directory with your WASM
mkdir -p wasm-image
cp addition.wasm wasm-image/

# Create OCI manifest
cat > wasm-image/index.json << 'EOF'
{
  "schemaVersion": 2,
  "manifests": [
    {
      "mediaType": "application/vnd.oci.image.manifest.v1+json",
      "digest": "sha256:PLACEHOLDER",
      "size": 0,
      "annotations": {
        "org.opencontainers.image.title": "wasm-addition"
      }
    }
  ]
}
EOF
```

## Step 3: Upload Unencrypted Image (Testing)

```bash
# Push directly using wasm-to-oci
wasm-to-oci push addition.wasm docker.io/rodneydav/wasm-addition:latest --server docker.io

# Or use helper script
./create-image.sh addition.wasm latest docker.io/rodneydav

# Test with tee-wasm-runner (unencrypted)
./tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/wasm-test
```

## Step 4: Encrypt the Image

### Generate Encryption Keys

```bash
# Generate a key pair for encryption
openssl genrsa -out private_key.pem 2048
openssl rsa -in private_key.pem -pubout -out public_key.pem

# Alternative: Generate JWE key
openssl rand -base64 32 > encryption.key
```

### Create ocicrypt Configuration

```bash
cat > ocicrypt.conf << 'EOF'
{
  "key-providers": {
    "local": {
      "cmd": {
        "path": "/usr/local/bin/ocicrypt",
        "args": ["local"]
      }
    }
  }
}
EOF

export OCICRYPT_KEYPROVIDER_CONFIG=$(pwd)/ocicrypt.conf
```

### Encrypt with ocicrypt

```bash
# Using JWE encryption
ocicrypt encrypt \
  --recipient jwe:/path/to/public_key.pem \
  docker.io/rodneydav/wasm-addition:latest \
  docker.io/rodneydav/wasm-addition:encrypted

# Using GPG encryption
ocicrypt encrypt \
  --recipient pgp:your-email@example.com \
  docker.io/rodneydav/wasm-addition:latest \
  docker.io/rodneydav/wasm-addition:encrypted

# Using local key file
ocicrypt encrypt \
  --recipient jwe:/path/to/public_key.pem \
  -o docker.io/rodneydav/wasm-addition:encrypted \
  oci:docker.io/rodneydav/wasm-addition:latest
```

### Encrypt with skopeo

```bash
# Skopeo supports encryption natively
skopeo copy \
  --encryption-key type:jwe:method:pkcs1:pubkey:./encryption.key \
  docker://docker.io/rodneydav/wasm-addition:latest \
  docker://docker.io/rodneydav/wasm-addition:encrypted
```

### Encrypt working

```bash
mkdir -p output

# Encrypt the WASM image
docker run -v "$PWD/output:/output" docker.io/rodneydav/coco-keyprovider:latest /encrypt.sh \
	-k "$(cat ./encryption.key)" \
 -i kbs:///default/wasm-keys/my-app \
 -s docker://docker.io/rodneydav/wasm-addition:latest \
 -d dir:/output

# Push to your registry
skopeo copy dir:./output docker://docker.io/rodneydav/wasm-addition:encrypted
```

### Verify Encrypted Layers

```bash
# Inspect the encrypted image
docker manifest inspect docker.io/rodneydav/wasm-addition:encrypted

# You should see layers with media type:
# application/vnd.wasm.content.layer.v1+wasm+encrypted
```

## Step 5: Setup Key Broker Service (KBS)

### Deploy KBS with Docker Compose (Recommended)

The easiest way to set up KBS is using the official Trustee stack:

```bash
# Clone Trustee repository
git clone https://github.com/confidential-containers/trustee.git
cd trustee

# Start KBS stack (KBS, AS, RVPS, Keyprovider)
docker-compose up -d

# Wait for services to start
sleep 10

# Verify KBS is running
curl http://10.0.2.2:8080/kbs/v0/auth
# Should return: {"challenge":"...","attestation-service-url":"..."}

# Build kbs-client tool
make cli

# Set allow-all policy (for testing with Sample Attester)
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego
```

**Services in the stack:**
- **KBS** (port 8080) - Key Broker Service
- **AS** (port 50004) - Attestation Service (gRPC)
- **RVPS** (port 50003) - Reference Value Provider Service
- **Keyprovider** (port 50000) - CoCo Keyprovider

### Register Decryption Key with KBS

```bash
# Upload the private key to KBS using kbs-client
cd trustee
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file /path/to/private_key.pem \
  --path default/key/wasm-addition

# Verify the key was uploaded
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  get-resource \
  --path default/key/wasm-addition
```

**Note**: The resource path follows the pattern `<repository>/<type>/<tag>`. For encryption keys, use `default/key/<key-name>`.

**See [KBS_SETUP.md](./KBS_SETUP.md) for detailed setup instructions and troubleshooting.**

## Step 6: Run Encrypted WASM in TEE

### Configure Attestation Agent

```bash
cat > aa-config.toml << 'EOF'
# aa-config.toml
[token_configs]
[token_configs.coco_kbs]
url = "http://localhost:8080"
EOF
```

### Run the Encrypted Image

```bash
# Run with TEE attestation and KBS
./tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://localhost:8080 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10

# Output: 15
```

### Expected Output

```
[INFO] Starting TEE WASM Runner...
[INFO] TEE evidence obtained: 1234 bytes
[INFO] Successfully pulled manifest for image: docker.io/rodneydav/wasm-addition:encrypted
[INFO] Detected encrypted layers, setting up KBS client
[INFO] Successfully obtained decryption key from KBS
[INFO] Successfully pulled and decrypted WASM to: /tmp/tee-wasm-runner/layers/...
[INFO] Setting up Wasmtime engine...
[INFO] Loading WASM module from: /tmp/tee-wasm-runner/layers/...
[INFO] Looking for '_start' function...
[INFO] Running WASM with args: ["wasm", "docker.io/rodneydav/wasm-addition:encrypted"]
[INFO] WASM execution completed successfully
```

## Step 7: Verify and Debug

### Check Image Layers

```bash
# Pull the image to inspect layers
skopeo copy docker://docker.io/rodneydav/wasm-addition:encrypted oci:local-image
ls -la local-image/blobs/sha256/

# Look for encrypted layers (they should have +encrypted in media type)
```

### Enable Debug Logging

```bash
# Run with verbose logging
RUST_LOG=debug ./tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --kbs-uri http://localhost:8080 \
  --aa-config aa-config.toml
```

### Common Issues

#### Issue: "Failed to get decryption key from KBS"

**Solution:**

- Verify KBS is running and accessible:
  ```bash
  curl http://localhost:8080/kbs/v0/auth
  ```
- Check the KBS URI is correct (use `http://localhost:8080` when running on host)
- Ensure the resource was uploaded correctly:
  ```bash
  cd trustee
  ./target/release/kbs-client \
    --url http://127.0.0.1:8080 \
    get-resource \
    --path default/key/wasm-addition
  ```
- Verify allow-all policy is set (for Sample Attester testing):
  ```bash
  ./target/release/kbs-client \
    --url http://127.0.0.1:8080 \
    config \
    --auth-private-key kbs/config/private.key \
    set-resource-policy \
    --policy-file kbs/sample_policies/allow_all.rego
  ```

#### Issue: "WASM module has no '\_start' or 'main' function"

**Solution:**

- Ensure your WASM module exports a `_start` or `main` function
- For library-style WASM, create a wrapper that calls your exported functions

#### Issue: "Decryption failed"

**Solution:**

- Verify the encryption/decryption keys match
- Check the encryption method (jwe/pgp) is supported
- Ensure the media type is `application/vnd.wasm.content.layer.v1+wasm+encrypted`

## Example: Complete Workflow Script

```bash
#!/bin/bash

set -e

# Variables
IMAGE_NAME="wasm-addition"
REGISTRY="docker.io/rodneydav"
TAG="encrypted"
WASM_FILE="addition.wasm"
PRIVATE_KEY="private_key.pem"
PUBLIC_KEY="public_key.pem"

echo "=== 1. Building WASM module ==="
wat2wasm addition.wat -o $WASM_FILE

echo "=== 2. Creating OCI image ==="
# Use wasm-to-oci to push directly to registry
wasm-to-oci push $WASM_FILE $REGISTRY/$IMAGE_NAME:latest --server $REGISTRY

echo "=== 4. Generating encryption keys ==="
openssl genrsa -out $PRIVATE_KEY 2048
openssl rsa -in $PRIVATE_KEY -pubout -out $PUBLIC_KEY

echo "=== 5. Encrypting image ==="
skopeo copy \
  --encryption-key type:jwe:method:pkcs1:pubkey:$PUBLIC_KEY \
  oci:$REGISTRY/$IMAGE_NAME:latest \
  docker://$REGISTRY/$IMAGE_NAME:$TAG

echo "=== 6. Setting up KBS ==="
git clone https://github.com/confidential-containers/trustee.git
cd trustee
docker-compose up -d
sleep 10

echo "=== 7. Configuring KBS ==="
make cli
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego

./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file ../$PRIVATE_KEY \
  --path default/key/wasm-addition

echo "=== 8. Running in TEE ==="
cd /path/to/guest-components
./target/release/tee-wasm-runner \
  --image-reference $REGISTRY/$IMAGE_NAME:$TAG \
  --kbs-uri http://localhost:8080 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 100 --wasm-args 250

echo "=== Complete! ==="
```

## Testing with Your Example Image

To test the unencrypted image `docker.io/rodneydav/wasm-addition:latest`:

```bash
# Pull and run unencrypted image
./tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/wasm-addition-test
```

## Security Considerations

1. **Key Management**: Never commit private keys to version control
2. **KBS Security**: Ensure KBS runs in a secure environment with proper authentication
3. **TEE Verification**: Verify TEE evidence before trusting the execution environment
4. **Least Privilege**: Use minimal permissions for WASM modules
5. **Replay Protection**: Use nonces or timestamps in KBS requests

## Additional Resources

- [OCI Image Spec](https://github.com/opencontainers/image-spec)
- [Wasmtime Documentation](https://docs.wasmtime.dev/)
- [Confidential Containers](https://confidentialcontainers.org/)
- [ocicrypt](https://github.com/containers/ocicrypt)
