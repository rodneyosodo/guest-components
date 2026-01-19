#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

IMAGE_SOURCE="${1}"
IMAGE_DEST="${2}"
KEY_FILE="${3:-private_key.pem}"

if [ -z "${1}" ] || [ -z "${2}" ]; then
	echo "Usage: $0 <source-image> <dest-image> [private-key-file]"
	echo ""
	echo "Encrypts a WASM OCI image using JWE encryption."
	echo ""
	echo "Arguments:"
	echo "  source-image      Source image reference (e.g., docker.io/user/wasm:latest)"
	echo "  dest-image        Destination encrypted image (e.g., docker.io/user/wasm:encrypted)"
	echo "  private-key-file  Path to RSA private key (default: private_key.pem)"
	echo ""
	echo "Examples:"
	echo "  # Generate keys and encrypt"
	echo "  openssl genrsa -out private_key.pem 2048"
	echo "  openssl rsa -in private_key.pem -pubout -out public_key.pem"
	echo "  $0 docker.io/user/wasm:latest docker.io/user/wasm:encrypted"
	echo ""
	echo "  # Encrypt with custom key file"
	echo "  $0 docker.io/user/wasm:latest docker.io/user/wasm:encrypted my-key.pem"
	exit 1
fi

# Check if skopeo is available
if ! command -v skopeo &>/dev/null; then
	echo "Error: skopeo not found. Please install skopeo:"
	echo "  sudo apt-get install skopeo"
	exit 1
fi

# Check if key file exists
if [ ! -f "${KEY_FILE}" ]; then
	echo "Error: Private key file '${KEY_FILE}' not found"
	echo ""
	echo "Generate a new key pair:"
	echo "  openssl genrsa -out ${KEY_FILE} 2048"
	echo "  openssl rsa -in ${KEY_FILE} -pubout -out ${KEY_FILE%.pem}.pub"
	exit 1
fi

# Derive public key path
PUBLIC_KEY="${KEY_FILE%.pem}.pub"
if [ ! -f "${PUBLIC_KEY}" ]; then
	echo "Error: Public key '${PUBLIC_KEY}' not found"
	echo ""
	echo "Extract public key from private key:"
	echo "  openssl rsa -in ${KEY_FILE} -pubout -out ${PUBLIC_KEY}"
	exit 1
fi

echo "Encrypting image..."
echo "  Source: ${IMAGE_SOURCE}"
echo "  Destination: ${IMAGE_DEST}"
echo "  Public key: ${PUBLIC_KEY}"
echo ""

# Encrypt using skopeo
skopeo copy \
	--encryption-key type:jwe:method:pkcs1:pubkey:"${PUBLIC_KEY}" \
	docker://"${IMAGE_SOURCE}" \
	docker://"${IMAGE_DEST}"

echo ""
echo "✓ Image encrypted successfully!"
echo ""
echo "Encrypted image: ${IMAGE_DEST}"
echo ""
echo "To verify encryption:"
echo "  docker manifest inspect ${IMAGE_DEST}"
echo ""
echo "Look for media type: 'application/vnd.wasm.content.layer.v1+wasm+encrypted'"
echo ""
echo "IMPORTANT: Keep '${KEY_FILE}' secure! It's needed to decrypt the image."
echo "To decrypt, the key must be available via KBS or specified in aa-config.toml"
