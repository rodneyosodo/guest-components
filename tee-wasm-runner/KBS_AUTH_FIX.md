# Fixing KBS Admin Authentication Error

## Problem

```bash
Error: Request Failed, Response: "{\"type\":\"https://github.com/confidential-containers/kbs/errors/AdminAuth\",\"detail\":\"Admin auth error: Admin Token could not be verified for any admin persona\"}"
```

## Root Cause

The `private.key` used by `kbs-client` doesn't match the `public.pub` configured in KBS, or the keys weren't generated correctly.

## Solution

### Step 1: Check if keys exist

```bash
cd ~/trustee

# Check if keys exist
ls -la kbs/config/private.key kbs/config/public.pub

# If they don't exist or are empty, proceed to Step 2
```

### Step 2: Regenerate keys

```bash
cd ~/trustee

# Stop the services
docker-compose down

# Remove old keys
rm -f kbs/config/private.key kbs/config/public.pub
rm -f kbs/config/*.pem kbs/config/token.*

# Restart services (setup container will regenerate keys)
docker-compose up -d

# Wait for setup to complete
sleep 15

# Check setup logs
docker-compose logs setup

# Verify keys were created
ls -la kbs/config/private.key kbs/config/public.pub
```

### Step 3: Verify key format

```bash
cd ~/trustee

# Check if private key is valid
openssl pkey -in kbs/config/private.key -text -noout

# Check if public key is valid
openssl pkey -pubin -in kbs/config/public.pub -text -noout

# They should both be Ed25519 keys
```

### Step 4: Check KBS configuration

```bash
cd ~/trustee

# View KBS config
cat kbs/config/docker-compose/kbs-config.toml

# Look for this section:
# [[admin.personas]]
# id = "admin"
# public_key_path = "/opt/confidential-containers/kbs/user-keys/public.pub"
```

### Step 5: Restart KBS to pick up new keys

```bash
cd ~/trustee

# Restart KBS service
docker-compose restart kbs

# Wait for KBS to start
sleep 5

# Check KBS logs for any errors
docker-compose logs kbs | tail -20
```

### Step 6: Try the command again

```bash
cd ~/trustee

# Try setting policy again
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego

# Should succeed with no output (or success message)
```

### Step 7: Verify policy was set

```bash
cd ~/trustee

# Try to get the policy back
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  get-resource-policy
```

## Alternative: Manual Key Generation

If the automatic generation doesn't work, generate keys manually:

```bash
cd ~/trustee

# Generate Ed25519 key pair
openssl genpkey -algorithm ed25519 -out kbs/config/private.key
openssl pkey -in kbs/config/private.key -pubout -out kbs/config/public.pub

# Verify they were created
ls -la kbs/config/private.key kbs/config/public.pub

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

## Debugging

### Check if keys match

```bash
cd ~/trustee

# Extract public key from private key
openssl pkey -in kbs/config/private.key -pubout -out /tmp/derived_public.pub

# Compare with configured public key
diff kbs/config/public.pub /tmp/derived_public.pub

# Should show no differences
```

### Check KBS can read the keys

```bash
cd ~/trustee

# Check KBS container has access to keys
docker-compose exec kbs ls -la /opt/confidential-containers/kbs/user-keys/

# Should show private.key and public.pub
```

### View KBS admin configuration

```bash
cd ~/trustee

# Check what KBS thinks the admin config is
docker-compose exec kbs cat /opt/confidential-containers/kbs/user-keys/docker-compose/kbs-config.toml | grep -A 5 "\[admin\]"
```

## Common Issues

### Issue 1: Keys are empty

```bash
# Check file sizes
ls -lh kbs/config/private.key kbs/config/public.pub

# If size is 0 bytes, regenerate:
docker-compose down
rm -f kbs/config/*.key kbs/config/*.pub
docker-compose up -d
```

### Issue 2: Permission denied

```bash
# Fix permissions
chmod 600 kbs/config/private.key
chmod 644 kbs/config/public.pub
```

### Issue 3: Wrong key format

The keys should be **Ed25519** format, not RSA. Check with:

```bash
openssl pkey -in kbs/config/private.key -text -noout | head -1
# Should show: "ED25519 Private-Key:"
```

If it's RSA, regenerate using Ed25519:
```bash
openssl genpkey -algorithm ed25519 -out kbs/config/private.key
openssl pkey -in kbs/config/private.key -pubout -out kbs/config/public.pub
docker-compose restart kbs
```

## Complete Reset (Nuclear Option)

If nothing works, do a complete reset:

```bash
cd ~/trustee

# Stop everything
docker-compose down -v

# Remove all generated files
rm -rf kbs/config/private.key kbs/config/public.pub
rm -rf kbs/config/*.pem kbs/config/token.*
rm -rf kbs/data/

# Start fresh
docker-compose up -d

# Wait longer for setup
sleep 30

# Check setup completed
docker-compose logs setup

# Verify keys exist and are not empty
ls -lh kbs/config/private.key kbs/config/public.pub

# Try the command
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource-policy \
  --policy-file kbs/sample_policies/allow_all.rego
```

## Verification Checklist

- [ ] Keys exist: `ls -la kbs/config/private.key kbs/config/public.pub`
- [ ] Keys are not empty: `ls -lh kbs/config/private.key kbs/config/public.pub`
- [ ] Keys are Ed25519 format: `openssl pkey -in kbs/config/private.key -text -noout | head -1`
- [ ] Keys match: `openssl pkey -in kbs/config/private.key -pubout | diff - kbs/config/public.pub`
- [ ] KBS is running: `docker-compose ps kbs`
- [ ] KBS can access keys: `docker-compose exec kbs ls /opt/confidential-containers/kbs/user-keys/`
- [ ] KBS config points to correct keys: `docker-compose exec kbs cat /opt/confidential-containers/kbs/user-keys/docker-compose/kbs-config.toml`

## Success Indicators

When the command succeeds, you should see:
```bash
# Either no output (success)
# Or a success message

# You can verify by checking the logs:
docker-compose logs kbs | tail -20
# Should show something like: "Policy updated successfully"
```

## Next Steps

Once authentication works:

1. **Upload encryption keys:**
```bash
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  config \
  --auth-private-key kbs/config/private.key \
  set-resource \
  --resource-file /path/to/encryption-key.pem \
  --path default/key/mykey
```

2. **Verify key was uploaded:**
```bash
./target/release/kbs-client \
  --url http://127.0.0.1:8082 \
  get-resource \
  --path default/key/mykey
```

3. **Test with TEE WASM Runner:**
```bash
cd /path/to/guest-components
./target/release/tee-wasm-runner \
  --image-reference docker.io/user/wasm:encrypted \
  --kbs-uri http://localhost:8082 \
  --aa-config aa-config.toml
```

## Need More Help?

If you're still having issues:

1. Share the output of:
```bash
ls -lh ~/trustee/kbs/config/
docker-compose logs setup
docker-compose logs kbs | tail -50
```

2. Check [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for more issues

3. Open an issue with full logs
