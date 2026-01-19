# TEE WASM Runner

A simple client that runs inside a Trusted Execution Environment (TEE) to pull encrypted WASM OCI images from a registry, decrypt them using TEE procedures, and execute them using Wasmtime.

## Quick Links

 - **[TDX_BUILD.md](./TDX_BUILD.md)(** - TDX Support Guide**
- **[QUICK_REFERENCE.md](./QUICK_REFERENCE.md)** - Command cheat sheet (print this!)
 - **ACTION_GUIDE)(./ACTION_GUIDE.md)*** - IMMEDIATE ACTION GUIDE**
- **[TROUBLESHOOTING.md](./TROUBLESHOOTING.md)** - Common issues and solutions
- **[SUMMARY.md](./SUMMARY.md)** - Complete overview and workflow diagram
- **[KBS_SETUP.md](./KBS_SETUP.md)** - KBS setup with Docker Compose (for encrypted images)
- [Quick Start Guide](./QUICKSTART.md) - Get up and running in 5 minutes
 - **[ENCRYPTED_WASM_FIX.md](./ENCRYPTED_WASM_FIX.md)** - Encrypted WASM image fix (IMPORTANT!)
 - **[KBS_AUTH_FIX.md](./KBS_AUTH_FIX.md)** - Fix for KBS admin authentication
- [Complete Encryption Guide](./ENCRYPTING_WASM_GUIDE.md) - Detailed guide for creating, encrypting, and deploying WASM images

## Helper Scripts

## Helper Scripts

The project includes several helper scripts to simplify common tasks:

> **Note**: These scripts use `wasm-to-oci` which is the recommended tool for creating WASM OCI images. Install it with `go install github.com/engineerd/wasm-to-oci@latest`

- `build-examples.sh` - Build the example WASM modules from .wat files
- `create-image.sh` - Create an OCI image from a WASM file
- `encrypt-image.sh` - Encrypt an OCI image using JWE encryption

## Features
- **[TDX_PERMISSION_FIX.md](./TDX_PERMISSION_FIX.md)** - Fix TDX attester permissions

**Encrypted WASM Support** - The runner now correctly handles encrypted WASM images:

- ✅ Detects encrypted media types (`application/vnd.wasm.content.layer.v1+wasm+encrypted`)
- ✅ Uses OCI layer handler with decryption for encrypted images
- ✅ Fetches decryption keys from KBS using attestation
- ✅ Decrypts WASM layers inside TEE before execution

**See [ENCRYPTED_WASM_FIX.md](./ENCRYPTED_WASM_FIX.md)** for details.

**Function Invocation** - Invoke specific WASM functions with arguments:

```bash
./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --invoke add --wasm-args 5 --wasm-args 10
# Output: 15
```

## Troubleshooting

For common issues and solutions, see:
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues and solutions
- [KBS_PROTOCOL_FIX.md](./KBS_PROTOCOL_FIX.md) - KBS protocol mismatch issues
- [WASM_CONFIG_FIX.md](./WASM_CONFIG_FIX.md) - WASM minimal config issues


The project includes several helper scripts to simplify common tasks:

> **Note**: These scripts use `wasm-to-oci` which is the recommended tool for creating WASM OCI images. Install it with `go install github.com/engineerd/wasm-to-oci@latest`

- `build-examples.sh` - Build the example WASM modules from .wat files
- `create-image.sh` - Create an OCI image from a WASM file
- `encrypt-image.sh` - Encrypt an OCI image using JWE encryption

### Building Example WASM Modules

```bash
# Build all example WASM modules
./build-examples.sh

# Creates:
# build/example.wasm - Simple hello world
# build/addition.wasm - Addition demonstration
```

### Creating an OCI Image from WASM

```bash
# Using wasm-to-oci directly (recommended)
wasm-to-oci push build/example.wasm localhost/example-wasm:latest --server localhost

# Or use the helper script
./create-image.sh example.wasm latest localhost

# Push to Docker Hub
wasm-to-oci push build/addition.wasm docker.io/rodneydav/wasm-addition:latest --server docker.io
```

### Encrypting an Image

```bash
# Generate encryption keys
openssl genrsa -out private_key.pem 2048
openssl rsa -in private_key.pem -pubout -out public_key.pem

# Encrypt an image
./encrypt-image.sh \
  docker.io/yourusername/wasm:latest \
  docker.io/yourusername/wasm:encrypted

# Encrypt with custom key file
./encrypt-image.sh \
  docker.io/yourusername/wasm:latest \
  docker.io/yourusername/wasm:encrypted \
  my-custom-key.pem
```

## Features

- **Pull Encrypted WASM Images**: Pulls encrypted WASM modules from OCI registries using `image-rs`
- **TEE Attestation**: Performs TEE attestation to verify the execution environment
- **Key Broker Service (KBS) Integration**: Retrieves decryption keys securely from a KBS
- **WASM Execution**: Executes decrypted WASM modules using Wasmtime with WASI support
- **Multiple TEE Support**: Supports various TEE platforms including:
  - Intel TDX
  - AMD SEV-SNP
  - Intel SGX
  - Azure SNP vTPM
  - Azure TDX vTPM
  - IBM Secure Execution (SE)
  - CCA

## Building

```bash
# Build with default TEE support (Sample)
cargo build --release --package tee-wasm-runner

# Build with specific TEE support
cargo build --release --package tee-wasm-runner --features "tdx-attester"
cargo build --release --package tee-wasm-runner --features "snp-attester"
cargo build --release --package tee-wasm-runner --features "sgx-attester"
```

## Usage

### Basic Usage (Unencrypted Image)

```bash
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:latest \
  --work-dir /tmp/wasm-test \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

### With TEE Attestation and KBS (Encrypted Image)

```bash
RUST_LOG=info  ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://localhost:8080 \
  --aa-config /path/to/aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

## Arguments

| Argument             | Short | Description                                              | Default                       |
| -------------------- | ----- | -------------------------------------------------------- | ----------------------------- |
| `--image-reference`  | `-i`  | OCI image reference (e.g., `docker.io/user/wasm:latest`) | Required                      |
| `--work-dir`         | `-w`  | Working directory for WASM execution                     | `/tmp/tee-wasm-runner`        |
| `--layer-store-path` | `-l`  | Directory to store pulled image layers                   | `/tmp/tee-wasm-runner/layers` |
| `--kbs-uri`          | `-k`  | Key Broker Service URI (required for encrypted images)   | None                          |
| `--kbc-name`         | `-n`  | Key Broker Client name                                   | `sample`                      |
| `--aa-config`        | `-a`  | Attestation Agent configuration file                     | None                          |
| `--runtime`          | `-r`  | WASM runtime to use (wasmtime, wasmer, etc.)             | `wasmtime`                    |
| `--invoke`           |       | Function name to invoke in the WASM module               | None                          |
| `--wasm-args`        |       | Arguments to pass to WASM module/function                | `[]`                          |

## Workflow

1. **Initialization**: Initialize the Attestation Agent
2. **TEE Evidence**: Collect TEE evidence to prove we're running in a TEE
3. **Pull Image**: Pull the WASM OCI image from the registry
4. **Decrypt (if encrypted)**:
   - If image is encrypted, establish secure channel with KBS
   - Retrieve decryption key using TEE attestation
   - Decrypt the WASM layer
5. **Execute**: Run the decrypted WASM module using wasmtime CLI (or other WASM runtime)

## Environment Variables

- `RUST_LOG`: Set logging level (e.g., `RUST_LOG=info`)
- `OCICRYPT_KEYPROVIDER_CONFIG`: Path to OCI crypt keyprovider config (if using custom key providers)

## Example WASM Image

A WASM OCI image should have:

- A WASM layer with media type `application/vnd.wasm.content.layer.v1+wasm` (unencrypted)
  or `application/vnd.wasm.content.layer.v1+wasm+encrypted` (encrypted)
- An OCI manifest and config
- Optional encryption annotations for encrypted images

## Security

- The WASM module is decrypted entirely within the TEE
- Decryption keys are obtained through attested KBS requests
- Evidence is generated to prove the execution environment
- All sensitive operations occur inside the secure enclave

## Building WASM for This Runner

```bash
# Compile a simple WASM module
wat2wasm example.wat -o example.wasm

# Create an OCI image with the WASM layer
# (Use tools like skopeo or buildah to package it)
```

## Dependencies

- `wasmtime`: WASM runtime
- `wasmtime-wasi`: WASI system interface support
- `image-rs`: OCI image management
- `attestation-agent`: TEE attestation
- `kbs_protocol`: KBS client protocol
- `ocicrypt-rs`: OCI encryption/decryption

## License

Apache-2.0
