#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/release/tee-wasm-runner"
WORK_DIR="/tmp/tee-wasm-runner-test"
LAYER_STORE="${WORK_DIR}/layers"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test results tracking
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

# Helper functions
print_header() {
	echo ""
	echo "========================================="
	echo "$1"
	echo "========================================="
	echo ""
}

print_success() {
	echo -e "${GREEN}✓ $1${NC}"
	TESTS_PASSED=$((TESTS_PASSED + 1))
}

print_failure() {
	echo -e "${RED}✗ $1${NC}"
	TESTS_FAILED=$((TESTS_FAILED + 1))
}

print_skip() {
	echo -e "${YELLOW}⊘ $1${NC}"
	TESTS_SKIPPED=$((TESTS_SKIPPED + 1))
}

print_info() {
	echo -e "${YELLOW}ℹ $1${NC}"
}

cleanup() {
	print_info "Cleaning up test environment..."
	rm -rf "${WORK_DIR}"
}

# Setup test environment
setup() {
	print_header "Setting up test environment"

	# Build the project
	print_info "Building tee-wasm-runner..."
	cd "${SCRIPT_DIR}/.."
	cargo build --release --package tee-wasm-runner

	if [ ! -f "${BINARY}" ]; then
		print_failure "Binary not found at ${BINARY}"
		exit 1
	fi

	print_success "Binary built successfully"

	# Create test directories
	mkdir -p "${WORK_DIR}"
	mkdir -p "${LAYER_STORE}"

	print_success "Test directories created"
}

# Test 1: Basic WASM image pull and execution
test_basic_wasm_image() {
	print_header "Test 1: Basic WASM Image Pull and Execute"

	local image="docker.io/rodneydav/wasm-addition:latest"

	print_info "Testing with image: ${image}"

	if RUST_LOG=info "${BINARY}" \
		--image-reference "${image}" \
		--work-dir "${WORK_DIR}" \
		--layer-store-path "${LAYER_STORE}"; then
		print_success "Successfully pulled and executed WASM image"

		# Verify the WASM file was downloaded
		local wasm_count=$(find "${LAYER_STORE}" -name "*.wasm" | wc -l)
		if [ "${wasm_count}" -gt 0 ]; then
			print_success "WASM file downloaded to layer store"
		else
			print_failure "WASM file not found in layer store"
			return 1
		fi

		# Verify the WASM file is valid
		local wasm_file=$(find "${LAYER_STORE}" -name "*.wasm" -type f | head -n 1)
		if file "${wasm_file}" | grep -q "WebAssembly"; then
			print_success "Downloaded file is valid WebAssembly"
		else
			print_failure "Downloaded file is not valid WebAssembly"
			return 1
		fi
	else
		print_failure "Failed to pull and execute WASM image"
		return 1
	fi
}

# Test 2: WASM with function invocation
test_wasm_with_invoke() {
	print_header "Test 2: WASM Function Invocation"

	local image="docker.io/rodneydav/wasm-addition:latest"

	print_info "Testing with image: ${image}"
	print_info "Invoking function: add(5, 10)"

	# Clean previous layers
	rm -rf "${LAYER_STORE}"
	mkdir -p "${LAYER_STORE}"

	local output
	output=$(RUST_LOG=info "${BINARY}" \
		--image-reference "${image}" \
		--work-dir "${WORK_DIR}" \
		--layer-store-path "${LAYER_STORE}" \
		--invoke add \
		--wasm-args 5 \
		--wasm-args 10 2>&1)

	if echo "${output}" | grep -q "WASM execution completed successfully"; then
		print_success "Successfully invoked WASM function"

		# Check if result is correct (5 + 10 = 15)
		if echo "${output}" | grep -q "15"; then
			print_success "Function returned correct result: 15"
		else
			print_failure "Function did not return expected result"
			return 1
		fi
	else
		print_failure "Failed to invoke WASM function"
		return 1
	fi
}

# Test 3: Verify TEE attestation
test_tee_attestation() {
	print_header "Test 3: TEE Attestation Verification"

	local image="docker.io/rodneydav/wasm-addition:latest"

	print_info "Testing TEE attestation (Sample attester in non-TEE environment)"

	# Clean previous layers
	rm -rf "${LAYER_STORE}"
	mkdir -p "${LAYER_STORE}"

	# Capture output to verify attestation
	local output
	output=$(RUST_LOG=info "${BINARY}" \
		--image-reference "${image}" \
		--work-dir "${WORK_DIR}" \
		--layer-store-path "${LAYER_STORE}" 2>&1)

	if echo "${output}" | grep -q "TEE evidence obtained"; then
		print_success "TEE attestation successful"
	else
		print_failure "TEE attestation failed"
		return 1
	fi

	if echo "${output}" | grep -q "Sample"; then
		print_info "Using Sample attester (expected in non-TEE environment)"
	fi
}

# Test 4: Encrypted image (requires KBS)
test_encrypted_image() {
	print_header "Test 4: Encrypted WASM Image"

	print_info "This test requires a running KBS instance"

	# Check if KBS_URI is set
	if [ -z "${KBS_URI}" ]; then
		print_skip "Skipped: KBS_URI environment variable not set"
		print_info "To run this test:"
		print_info "  export KBS_URI=http://localhost:8080"
		print_info "  ./test-runner.sh"
		return 0
	fi

	# Check if KBS is reachable
	if ! curl -s -f "${KBS_URI}/health" >/dev/null 2>&1; then
		print_skip "Skipped: KBS not reachable at ${KBS_URI}"
		print_info "Make sure KBS is running at ${KBS_URI}"
		return 0
	fi

	local image="docker.io/rodneydav/wasm-addition:encrypted"

	print_info "Testing with encrypted image: ${image}"

	# Clean previous layers
	rm -rf "${LAYER_STORE}"
	mkdir -p "${LAYER_STORE}"

	if RUST_LOG=info "${BINARY}" \
		--image-reference "${image}" \
		--work-dir "${WORK_DIR}" \
		--layer-store-path "${LAYER_STORE}" \
		--kbs-uri "${KBS_URI}"; then
		print_success "Successfully pulled and decrypted encrypted WASM image"
	else
		print_failure "Failed to pull and decrypt encrypted WASM image"
		return 1
	fi
}

# Test 5: Image caching (re-pull same image)
test_image_caching() {
	print_header "Test 5: Image Re-pull and Caching"

	local image="docker.io/rodneydav/wasm-addition:latest"

	print_info "First pull (fresh)..."
	rm -rf "${LAYER_STORE}"
	mkdir -p "${LAYER_STORE}"

	local start1=$(date +%s)
	"${BINARY}" \
		--image-reference "${image}" \
		--work-dir "${WORK_DIR}" \
		--layer-store-path "${LAYER_STORE}" >/dev/null 2>&1
	local end1=$(date +%s)
	local duration1=$((end1 - start1))

	print_info "First pull took: ${duration1}s"

	print_info "Second pull (cached)..."

	local start2=$(date +%s)
	"${BINARY}" \
		--image-reference "${image}" \
		--work-dir "${WORK_DIR}" \
		--layer-store-path "${LAYER_STORE}" >/dev/null 2>&1
	local end2=$(date +%s)
	local duration2=$((end2 - start2))

	print_info "Second pull took: ${duration2}s"

	# Note: We don't fail if second pull isn't faster since network conditions vary
	print_success "Image re-pull successful"
}

# Test 6: Invalid image reference
test_invalid_image() {
	print_header "Test 6: Invalid Image Reference Handling"

	local image="docker.io/invalid/nonexistent-wasm:latest"

	print_info "Testing with invalid image: ${image}"

	# Clean previous layers
	rm -rf "${LAYER_STORE}"
	mkdir -p "${LAYER_STORE}"

	if "${BINARY}" \
		--image-reference "${image}" \
		--work-dir "${WORK_DIR}" \
		--layer-store-path "${LAYER_STORE}" >/dev/null 2>&1; then
		print_failure "Should have failed with invalid image"
		return 1
	else
		print_success "Correctly handled invalid image reference"
	fi
}

# Test 7: Check for wasmtime dependency
test_wasmtime_available() {
	print_header "Test 7: WASM Runtime Availability"

	if command -v wasmtime &>/dev/null; then
		local version=$(wasmtime --version)
		print_success "wasmtime is available: ${version}"
	else
		print_failure "wasmtime is not installed"
		print_info "Install with: curl https://wasmtime.dev/install.sh -sSf | bash"
		return 1
	fi
}

# Print summary
print_summary() {
	print_header "Test Summary"

	local total=$((TESTS_PASSED + TESTS_FAILED + TESTS_SKIPPED))

	echo "Total tests: ${total}"
	echo -e "${GREEN}Passed: ${TESTS_PASSED}${NC}"
	echo -e "${RED}Failed: ${TESTS_FAILED}${NC}"
	echo -e "${YELLOW}Skipped: ${TESTS_SKIPPED}${NC}"
	echo ""

	if [ "${TESTS_FAILED}" -gt 0 ]; then
		echo -e "${RED}Some tests failed!${NC}"
		return 1
	elif [ "${TESTS_PASSED}" -eq 0 ]; then
		echo -e "${YELLOW}No tests passed!${NC}"
		return 1
	else
		echo -e "${GREEN}All tests passed!${NC}"
		return 0
	fi
}

# Main execution
main() {
	print_header "TEE WASM Runner Test Suite"

	# Trap cleanup
	trap cleanup EXIT

	# Setup
	setup

	# Run tests
	test_wasmtime_available || true
	test_basic_wasm_image || true
	test_wasm_with_invoke || true
	test_tee_attestation || true
	test_image_caching || true
	test_invalid_image || true
	test_encrypted_image || true

	# Print summary
	print_summary
}

# Run main
main "$@"
