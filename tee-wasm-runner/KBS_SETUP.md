# Setting Up KBS with Docker Compose

This guide explains how to set up a Key Broker Service (KBS) using Docker Compose for testing the TEE WASM Runner with encrypted images.

> **Port Configuration Note**: The KBS container runs on port 8080 internally, but may be mapped to different host ports (commonly 8080 or 8082). Check your `docker-compose ps` output to see which port is used. In this guide, we use **port 8082** as an example - adjust to match your setup.

## Prerequisites

```bash
# Install Docker and Docker Compose
sudo apt-get update
sudo apt-get install -y docker.io docker-compose git

# Add your user to docker group (optional, avoids sudo)
sudo usermod -aG docker $USER
newgrp docker
```

## Quick Start with Trustee KBS

### 1. Clone the Trustee Repository

```bash
git clone https://github.com/confidential-containers/trustee.git
cd trustee
```

### 2. Start KBS Stack with Docker Compose

The Trustee repository includes a complete KBS stack:

- **KBS**: Key Broker Service (port 8080 internal → 8080 or 8082 host)
- **AS**: Attestation Service (gRPC, port 50004)
- **RVPS**: Reference Value Provider Service (port 50003)
- **Keyprovider**: CoCo Keyprovider (port 50000)

```bash
# Start all services
docker-compose up -d

# Check services are running
docker-compose ps

# View logs
docker-compose logs -f kbs
```

### 3. Verify KBS is Running

```bash
# First, check which port KBS is mapped to
docker-compose ps kbs

# Common ports: 8080 (internal) may be mapped to 8080 or 8082 (host)
# Test the KBS endpoint
curl http://localhost:8082/kbs/v0/auth

# Note: You may see an error like:
# {"type":"...PluginNotFound","detail":"Plugin auth not found"}
# This is NORMAL for newer KBS versions - it means KBS is running!

# The auth endpoint doesn't exist in all KBS configurations.
# The KBS is working if you get ANY response (not "connection refused")
```

**Important**: If your KBS is on port 8082, use `http://localhost:8082` in all subsequent commands.

## Understanding the Docker Compose Setup

### Services Overview

```yaml
services:
  kbs: # Key Broker Service (port 8080)
  as: # Attestation Service (port 50004)
  rvps: # Reference Value Provider (port 50003)
  keyprovider: # CoCo Keyprovider (port 50000)
  setup: # Initial setup (generates keys)
```

### Configuration Files

The setup uses these config files:

1. **KBS Config** (`kbs/config/docker-compose/kbs-config.toml`):

```toml
[http_server]
sockets = ["0.0.0.0:8080"]
insecure_http = true

[attestation_token]
trusted_certs_paths = ["/opt/confidential-containers/kbs/user-keys/ca-cert.pem"]

[attestation_service]
type = "coco_as_grpc"
as_addr = "http://as:50004"

[admin]
type = "Simple"

[[admin.personas]]
id = "admin"
public_key_path = "/opt/confidential-containers/kbs/user-keys/public.pub"

[[plugins]]
name = "resource"
type = "LocalFs"
dir_path = "/opt/confidential-containers/kbs/repository"
```

2. **AS Config** (`kbs/config/as-config.json`):

```json
{
  "work_dir": "/opt/confidential-containers/attestation-service",
  "policy_engine": "opa",
  "rvps_config": {
    "type": "GrpcRemote",
    "address": "http://rvps:50003"
  },
  "attestation_token_broker": {
    "duration_min": 5,
    "signer": {
      "key_path": "/opt/confidential-containers/kbs/user-keys/token.key",
      "cert_path": "/opt/confidential-containers/kbs/user-keys/token-cert-chain.pem"
    }
  }
}
```

### Generated Keys

The `setup` service automatically generates these keys:

```
kbs/config/
├── private.key              # Admin authentication key (ed25519)
├── public.pub              # Admin public key
├── token.key               # Token signing key (EC P-256)
├── token-cert.pem          # Token certificate
├── token-cert-chain.pem    # Token cert chain
├── ca.key                  # Root CA key
└── ca-cert.pem             # Root CA certificate
```

## Uploading Keys for Encrypted Images

### 1. Install kbs-client

```bash
cd trustee

# Build kbs-client
make cli

# Install (optional)
sudo make install-cli

# Or use directly
./target/release/kbs-client --version
```

### 2. Set Allow-All Policy (for testing with Sample Attester)

When running outside a real TEE, the Sample Attester is used. By default, KBS rejects sample evidence, so we need to set a permissive policy:

```bash
# Important: Replace 8082 with your actual KBS port (check with docker-compose ps)

# Using installed kbs-client
kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego

# Or using built binary
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego
```

**If you get an "Admin auth error"**, the keys may not have been generated correctly. See **[KBS_AUTH_FIX.md](./KBS_AUTH_FIX.md)** for the fix.

### 3. Generate Encryption Keys for Your WASM Image

```bash
# Generate RSA key pair for image encryption
openssl genrsa -out wasm-image-key.pem 2048
openssl rsa -in wasm-image-key.pem -pubout -out wasm-image-key.pub
```

### 4. Upload Private Key to KBS

KBS expects resources to follow the path pattern: `<repository>/<type>/<tag>`

For encryption keys, the common pattern is: `default/key/<key-id>`

```bash
# Upload the private key to KBS
kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file ./encryption.key \
  --path default/wasm-keys/my-app

# Verify it was uploaded
kbs-client \
  --url http://127.0.0.1:8082 \
  get-resource \
  --path default/key/wasm-addition
```

**Note**: The key is stored in the local filesystem at:

```
kbs/data/kbs-storage/default/key/wasm-addition
```

## Encrypting WASM Images

### 1. Encrypt with Skopeo

```bash
# Encrypt the WASM image using the public key
skopeo copy \
  --encryption-key jwe:wasm-image-key.pub \
  docker://docker.io/rodneydav/wasm-addition:latest \
  docker://docker.io/rodneydav/wasm-addition:encrypted

# Or use the helper script
cd /path/to/guest-components/tee-wasm-runner
./encrypt-image.sh \
  docker.io/rodneydav/wasm-addition:latest \
  docker.io/rodneydav/wasm-addition:encrypted \
  /path/to/wasm-image-key.pem
```

### 2. Verify Encryption

```bash
# Check the encrypted image manifest
skopeo inspect docker://docker.io/rodneydav/wasm-addition:encrypted

# Look for encrypted media type:
# "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip+encrypted"
```

## Configuring TEE WASM Runner

### Create AA Config File

Create an `aa-config.toml` file for the attestation agent:

```toml
# aa-config.toml
[token_configs]
[token_configs.coco_kbs]
url = "http://10.0.2.2:8082"
```

### Run with Encrypted Image

```bash
cd /path/to/guest-components

# Run with encrypted image
sudo OCICRYPT_KEYPROVIDER_CONFIG=/etc/ocicrypt_keyprovider.conf RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --kbs-resource-path default/key/wasm-addition \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

coco_keyprovider --socket 127.0.0.1:50000 --kbs http://10.0.2.2:8082
  
./target/release/tee-wasm-runner \
 --image-reference docker.io/rodneydav/wasm-addition:encrypted \
 --work-dir /tmp/tee-wasm-runner \
 --kbs-uri http://10.0.2.2:8082 \
 --aa-config aa-config.toml \
 --invoke add \
 --wasm-args 5 --wasm-args 10

## KBS Resource URI Pattern

The TEE WASM Runner will request keys from KBS using this URI pattern:

```
kbs:///<repository>/<type>/<tag>
```

**Example**: `kbs:///default/key/wasm-addition`

This is configured in the runner's code (`src/main.rs:99`):

```rust
let resource_uri = "kbs:///default/key/1";  // Default resource URI
```

To use a custom key path, you'll need to modify this line or make it configurable.

## Troubleshooting

### KBS Not Starting

```bash
# Check logs
docker-compose logs kbs

# Common issue: Port already in use
sudo netstat -tlnp | grep 8080
sudo kill <PID>

# Restart
docker-compose restart kbs
```

### Cannot Upload Resources

```bash
# Check if private.key exists
ls -la kbs/config/private.key

# Regenerate keys if needed
docker-compose down
rm kbs/config/*.key kbs/config/*.pub kbs/config/*.pem
docker-compose up -d

# Wait for setup to complete
docker-compose logs setup
```

### Sample Attester Rejected

```bash
# Set allow-all policy (for testing only!)
kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego
```

### Decryption Failed

**Check key upload**:

```bash
kbs-client \
  --url http://127.0.0.1:8082 \
  get-resource \
  --path default/key/wasm-addition
```

**Verify encryption key matches**:

```bash
# The public key used for encryption must match
# the private key uploaded to KBS
openssl rsa -pubin -in wasm-image-key.pub -text -noout
openssl rsa -in wasm-image-key.pem -pubout | openssl rsa -pubin -text -noout
```

### Connection Refused

```bash
# Check if KBS is accessible from host
curl http://localhost:8082/kbs/v0/auth

# Check Docker network
docker network inspect trustee_default

# If running runner in Docker, use:
--kbs-uri http://kbs:8080  # (container name)

# If running on host, use:
--kbs-uri http://localhost:8082
```

## Advanced Configuration

### Custom Attestation Policy

For production, create a stricter attestation policy:

```bash
# Create custom policy file (example for TDX)
cat > my-tdx-policy.rego << 'EOF'
package my_policy

import future.keywords.if

default allow = false

# Allowed TCB and measurements
reference_tdx_tcb_svn = [ "03000500000000000000000000000000" ]
reference_tdx_mr_td = [ "abcd1234..." ]

allow if {
    input["tdx.quote.body.tcb_svn"] == reference_tdx_tcb_svn[_]
    input["tdx.quote.body.mr_td"] == reference_tdx_mr_td[_]
}
EOF

# Upload policy
kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-attestation-policy \
  --policy-file my-tdx-policy.rego
```

### HTTPS Configuration

To enable HTTPS for KBS:

1. Generate server certificates:

```bash
openssl req -x509 -newkey rsa:4096 \
  -keyout kbs-server.key \
  -out kbs-server.crt \
  -days 365 -nodes \
  -subj "/CN=localhost"
```

2. Update `docker-compose.yml`:

```yaml
services:
  kbs:
    volumes:
      - ./kbs-server.key:/certs/server.key:ro
      - ./kbs-server.crt:/certs/server.crt:ro
    command:
      [
        "/usr/local/bin/kbs",
        "--config-file",
        "/opt/confidential-containers/kbs/user-keys/docker-compose/kbs-config.toml",
        "--private-key",
        "/certs/server.key",
        "--certificate",
        "/certs/server.crt",
      ]
```

3. Update KBS config to remove `insecure_http`:

```toml
[http_server]
sockets = ["0.0.0.0:8080"]
# insecure_http = true  # Remove this line
```

## Stopping KBS

```bash
cd trustee

# Stop all services
docker-compose down

# Stop and remove volumes (clean slate)
docker-compose down -v
```

## Production Considerations

⚠️ **This setup is for TESTING only!** For production:

1. **Use Real TEE**: Deploy in TDX/SNP/SGX environment
2. **Remove `insecure_http`**: Enable HTTPS
3. **Strict Policies**: Remove `allow_all.rego`, use platform-specific policies
4. **Secure Key Storage**: Don't use LocalFs plugin, use secure vault
5. **Network Security**: Firewall rules, VPN, mTLS
6. **Key Rotation**: Implement key rotation strategy
7. **Audit Logging**: Enable comprehensive logging
8. **Monitoring**: Set up alerting for attestation failures

## References

- [Trustee GitHub](https://github.com/confidential-containers/trustee)
- [KBS Quickstart](https://github.com/confidential-containers/trustee/blob/main/kbs/quickstart.md)
- [KBS Configuration](https://github.com/confidential-containers/trustee/blob/main/kbs/docs/config.md)
- [Attestation Service](https://github.com/confidential-containers/attestation-service)
- [OPA Policies](https://www.openpolicyagent.org/)
