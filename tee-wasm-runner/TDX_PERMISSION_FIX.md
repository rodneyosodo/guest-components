# TDX Attester Permission Denied Fix

## Problem

When running TEE WASM Runner with TDX support in a TDX environment, you see:

```
[ERROR] Error running TEE WASM runner: Failed to get TEE evidence
    Caused by:
        0: TDX Attester: quote generation using ioctl() fallback failed after a TSM report error
        1: Failed to create TSM Report path instance: Permission denied (os error 13)
            at path "/sys/kernel/config/tsm/report/.tmp4E3bzf"
```

## Root Cause

The TDX attester is trying to create a temporary file in `/sys/kernel/config/tsm/report/` directory to generate the TDX quote. This requires **write permissions** to kernel config directories, which regular users don't have.

The error happens because:
1. The attester needs to create a temp file for TSM report
2. Default temp location is `/sys/kernel/config/tsm/report/`
3. User `propeller` doesn't have write permission to that directory
4. **Error 13** = `EACCES` - Permission denied

## Solutions

### Solution 1: Run with Sudo (Quick Fix)

```bash
# Run with sudo to give it root permissions
sudo RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

**⚠️ Warning**: Running with sudo gives the runner root access, which may not be ideal for security.

### Solution 2: Add User to TDX Group (Recommended)

First, check which group owns the TSM directory:

```bash
# Check owner of TSM directory
ls -ld /sys/kernel/config/tsm/report/

# Common groups: tdx, tsm, kvm
```

Then add user to the appropriate group:

```bash
# Add user to tdx group (most common)
sudo usermod -aG tdx propeller

# Or try tsm group
sudo usermod -aG tsm propeller

# Apply new group membership
newgrp tdx
# Or logout and login again
```

Now try running again:
```bash
# Should work now
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

### Solution 3: Use /tmp for TSM Reports (Workaround)

Configure attester to use `/tmp` instead of `/sys`:

This requires modifying attestation-agent configuration or environment variables.

```bash
# Create writable TSM report directory
sudo mkdir -p /tmp/tsm-report
sudo chown propeller:propeller /tmp/tsm-report

# Set environment variable for TSM report path
export TDX_REPORT_DIR="/tmp/tsm-report"

# Run with the environment variable
TDX_REPORT_DIR="/tmp/tsm-report" RUST_LOG=info \
  ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10
```

⚠️ **Note**: This may or may not work depending on attestation-agent implementation. Check attestation-agent documentation for TSM report path configuration.

### Solution 4: Run in Container with Privileges (for Production)

If deploying in a containerized TEE environment, ensure the container has proper capabilities:

```dockerfile
# Add capabilities for TDX
FROM rust:1.85
RUN useradd -m propeller

# Add TDX capabilities
RUN apt-get install -y libtdx-attester-dev

# Give user TDX access
USER propeller
ENV TDX_REPORT_DIR="/tmp/tsm-report"
```

```yaml
# Kubernetes deployment
apiVersion: v1
kind: Pod
metadata:
  name: tee-wasm-runner
spec:
  containers:
  - name: tee-wasm-runner
    image: tee-wasm-runner:latest
    securityContext:
      capabilities:
        add:
          - SYS_ADMIN  # May be needed for TDX
    env:
      - name: TDX_REPORT_DIR
        value: "/tmp/tsm-report"
```

### Solution 5: Check TDX Kernel Module (System-Level)

Ensure TDX kernel module is loaded and properly configured:

```bash
# Check if TDX module is loaded
lsmod | grep tdx

# Check TDX module status
cat /proc/crypto/tdx-guest/status

# Check if TDX guest device exists
ls -l /dev/tdx_guest

# View kernel messages about TDX
dmesg | grep -i tdx
```

If TDX is not loaded properly, you may need to:
```bash
# Load TDX module (may need root)
sudo modprobe tdx

# Or check with system administrator
```

## Troubleshooting

### Verify Group Change Worked

```bash
# Check user's groups
groups propeller

# Should include tdx or tsm
id propeller

# Check if new group is active
touch /tmp/test-write-$USER
rm /tmp/test-write-$USER
# If group change requires logout/login
```

### Check Directory Permissions

```bash
# Check TSM report directory permissions
ls -ld /sys/kernel/config/tsm/report/

# Check what groups have write access
getfacl /sys/kernel/config/tsm/report/

# If still denied, may need:
sudo chmod o+w /sys/kernel/config/tsm/report/
# ⚠️ Security risk: giving everyone write access to kernel config!
```

### Alternative: Disable TSM Report (Not Recommended)

Some TDX attesters have an option to skip TSM report:

```bash
# Check if attestation-agent supports disabling TSM report
cd /path/to/guest-components/attestation-agent

# Look for TSM report options
grep -r "tsm.*report" attester/tdx-attester/src/

# If available, configure via aa-config.toml or environment
```

Check attestation-agent documentation for TSM report configuration options.

## Verification

After applying one of the solutions:

```bash
# Run again and check for success
RUST_LOG=info ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10

# Expected output:
# [INFO] TEE Platform: Tdx (not Sample!)
# [INFO] TEE evidence obtained: 1500+ bytes (much larger than 44!)
# [INFO] Successfully obtained decryption key from KBS
# [INFO] WASM stdout:
#     15
# [INFO] WASM execution completed successfully
```

## Important Notes

### Evidence Size Difference

| Attester Type | Evidence Size | What It Means |
|--------------|---------------|---------------|
| Sample | ~44 bytes | Just dummy data |
| TDX Real | ~1500+ bytes | Actual TDX quote with measurements |

### Security Considerations

**Running with sudo**:
- ✅ Easy to implement
- ❌ Gives runner elevated privileges
- ❌ May break security model

**Running with proper group membership**:
- ✅ More secure
- ✅ Standard permission model
- ✅ Requires one-time setup

**Containerized deployment**:
- ✅ Most secure for production
- ✅ Isolated environment
- ✅ Can be controlled via pod security policies

## Recommended Approach for Testing

For development in a TDX VM:

```bash
# Option 1: Use sudo (quickest)
sudo ./target/release/tee-wasm-runner ...

# Option 2: Add to tdx group (better)
sudo usermod -aG tdx $USER
newgrp tdx
./target/release/tee-wasm-runner ...

# Option 3: Configure TSM report path (if supported)
TDX_REPORT_DIR="/tmp/tsm-report" ./target/release/tee-wasm-runner ...
```

For production deployment:

- Use properly configured containers with correct security contexts
- Ensure the container/user has necessary TDX capabilities
- Don't run as root in containers unless necessary
- Use least privilege principle

## Documentation Links

- [TDX_BUILD.md](./TDX_BUILD.md) - Build with TDX support
- [ACTION_GUIDE.md](./ACTION_GUIDE.md) - Overall action steps
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Common issues
- [attestation-agent docs](https://github.com/confidential-containers/attestation-agent) - Attestation agent documentation

## Getting More Help

If none of the solutions work:

1. **Collect debug information**:
```bash
RUST_LOG=debug ./target/release/tee-wasm-runner \
  --image-reference docker.io/rodneydav/wasm-addition:encrypted \
  --work-dir /tmp/tee-wasm-runner \
  --kbs-uri http://10.0.2.2:8082 \
  --aa-config aa-config.toml \
  --invoke add \
  --wasm-args 5 --wasm-args 10 2>&1 | tee tee-wasm-debug.log
```

2. **Check system permissions**:
```bash
# Check user groups
groups $USER

# Check TDX device permissions
ls -ld /dev/tdx_guest

# Check TSM report directory
ls -ld /sys/kernel/config/tsm/report/

# Check dmesg for TDX errors
sudo dmesg | grep -i tdx
```

3. **Check attestation-agent configuration**:
```bash
# Check if there are TSM report path options
cd /path/to/guest-components/attestation-agent
find . -name "*.toml" -exec grep -l "tsm.*report" {} \;
```

4. **Open an issue** with:
   - Full error messages
   - System information: `uname -a`, `lsmod | grep tdx`
   - User information: `id`
   - TDX status: `cat /proc/crypto/tdx-guest/status`
   - Steps to reproduce
