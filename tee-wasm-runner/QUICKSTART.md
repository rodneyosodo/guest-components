# Quick Start: Encrypt and Run WASM Image

A minimal guide to get started with encrypted WASM images.

## Prerequisites

```bash
# Install wasm-to-oci (for WASM OCI images)
go install github.com/engineerd/wasm-to-oci@latest

# Install skopeo (for encryption)
sudo apt-get install skopeo

# Install wabt (for wat2wasm)
sudo apt-get install wabt

# Build tee-wasm-runner
cd /path/to/guest-components
cargo build --release --package tee-wasm-runner
```

## 1. Create a Simple WASM Module

```bash
# Create a WASM text file
cat > hello.wat << 'EOF'
(module
  (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))

  (memory (export "memory") 1)
  (data (i32.const 8) "Hello from encrypted WASM!\n")

  (func $main (export "_start")
    (call $fd_write
      (i32.const 1)      ; stdout
      (i32.const 8)      ; iov_base (address of string)
      (i32.const 1)      ; iov_len (one iovec)
      (i32.const 24)     ; nwritten
    )
    drop
    (call $proc_exit (i32.const 0))
  )
)
EOF

# Compile to binary WASM
wat2wasm hello.wat -o hello.wasm
```

## 2. Create OCI Image

```bash
# Push WASM as OCI image using wasm-to-oci
wasm-to-oci push hello.wasm localhost/hello-wasm:latest --server localhost

# Or use the provided helper script
./create-image.sh hello.wasm latest localhost
```

## 3. Generate Encryption Keys

```bash
# Generate RSA key pair
openssl genrsa -out private_key.pem 2048
openssl rsa -in private_key.pem -pubout -out public_key.pem
```

## 4. Push Unencrypted Image (for Testing)

```bash
# Push to your registry using wasm-to-oci
wasm-to-oci push hello.wasm docker.io/yourusername/hello-wasm:latest --server docker.io

# Verify
wasm-to-oci pull docker.io/yourusername/hello-wasm:latest
```

## 5. Test Unencrypted Image

```bash
# Make sure wasmtime is installed
sudo apt-get install -y wasmtime
# Or: cargo install wasmtime-cli

# Run with tee-wasm-runner (no KBS needed for unencrypted)
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/yourusername/hello-wasm:latest \
  --work-dir /tmp/wasm-test
```

## 6. Encrypt the Image

```bash
# Encrypt using skopeo
skopeo copy \
  --encryption-key type:jwe:method:pkcs1:pubkey:public_key.pem \
  docker://docker.io/yourusername/hello-wasm:latest \
  docker://docker.io/yourusername/hello-wasm:encrypted
```

## 7. Setup KBS with Docker Compose

Use the Trustee KBS stack with Docker Compose:

```bash
# Clone Trustee repo
git clone https://github.com/confidential-containers/trustee.git
cd trustee

# Start KBS stack (KBS, AS, RVPS, Keyprovider)
docker-compose up -d

# Wait for services to start
sleep 10

# Verify KBS is running
curl http://localhost:8080/kbs/v0/auth

# Set allow-all policy (for testing with Sample Attester)
make cli
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego

# Upload your encryption key to KBS
./target/release/kbs-client \
  --url http://127.0.0.1:8080 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file /path/to/private_key.pem \
  --path default/key/wasm-addition
```

**See [KBS_SETUP.md](./KBS_SETUP.md) for complete setup instructions.**

## 8. Run Encrypted Image in TEE

```bash
# With KBS setup
./target/release/tee-wasm-runner \
  --image-reference docker.io/yourusername/hello-wasm:encrypted \
  --work-dir /tmp/wasm-encrypted-test \
  --kbs-uri http://localhost:8080 \
  --aa-config aa-config.toml

# Or with offline filesystem key (simpler for testing)
./target/release/tee-wasm-runner \
  --image-reference docker.io/yourusername/hello-wasm:encrypted \
  --work-dir /tmp/wasm-encrypted-test
```

## Testing with Your Example Image

For your unencrypted image `docker.io/rodneydav/wasm-addition:latest`:

```bash
# Install wasmtime first
sudo apt-get install -y wasmtime

# Test it runs unencrypted
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/addition-test

# Invoke the 'add' function with arguments
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/addition-test \
  --invoke add \
  --wasm-args "5" "10"

# Output: 15
```

To encrypt it:

```bash
# Generate keys
openssl genrsa -out addition_key.pem 2048
openssl rsa -in addition_key.pem -pubout -out addition_key.pub

# Encrypt the image
skopeo copy \
  --encryption-key type:jwe:method:pkcs1:pubkey:addition_key.pub \
  docker://docker.io/rodneydav/wasm-addition:latest \
  docker://docker.io/rodneydav/wasm-addition:encrypted

# Setup KBS with the private key
# (See ENCRYPTING_WASM_GUIDE.md for KBS setup)

# Now test encrypted version
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/addition-encrypted-test \
  --kbs-uri http://localhost:8080 \
  --aa-config aa-config.toml
```

## Verification

Check the encrypted image:

```bash
# Inspect manifest
docker manifest inspect docker.io/yourusername/hello-wasm:encrypted

# Look for media type:
# "application/vnd.wasm.content.layer.v1+wasm+encrypted"
```

## Troubleshooting

**"wasmtime: command not found"**: Install wasmtime runtime:

```bash
sudo apt-get install -y wasmtime
# Or from source:
curl https://wasmtime.dev/install.sh -sSf | bash
```

**"No '\_start' or 'main' function"**: Your WASM needs a `_start` function for WASI compatibility. The wasmtime CLI will look for this entry point.

**"Decryption failed"**: Ensure the private key matches the public key used for encryption.

**"Failed to get decryption key from KBS"**: Check that KBS is running and the key resource exists:

```bash
curl http://localhost:8080/health
```

## Next Steps

- Read [ENCRYPTING_WASM_GUIDE.md](./ENCRYPTING_WASM_GUIDE.md) for detailed instructions
- Set up a proper KBS server for production
- Explore more complex WASM applications
- Add WASI system calls for file/network access
