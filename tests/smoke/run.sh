#!/usr/bin/env bash
# Load-only smoke test: layer the pre-built plugin .so and a small MaxMind
# DB test fixture onto the public hub image, start the hub, and verify the
# plugin loads under the target architecture (amd64 native, arm64 / armv7
# via QEMU binfmt when the runner is amd64).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
LIB_NAME="libmaxmind_plugin"
GLIBC_VERSION="${GLIBC_VERSION:-2.36}"
HUB_TAG="${HUB_TAG:-latest}"
MMDB_SOURCE="${REPO_ROOT}/tests/fixtures/MaxMind-DB/test-data/GeoIP2-City-Test.mmdb"

ARCH=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --arch) ARCH="$2"; shift 2 ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

if [[ -z "${ARCH}" ]]; then
    echo "Usage: $0 --arch {amd64|arm64|armv7}"
    exit 1
fi

case "${ARCH}" in
    amd64) DOCKER_PLATFORM="linux/amd64" ;;
    arm64) DOCKER_PLATFORM="linux/arm64" ;;
    armv7) DOCKER_PLATFORM="linux/arm/v7" ;;
    *)     echo "Unsupported arch: ${ARCH}"; exit 1 ;;
esac

echo "=== Smoke Test: ${ARCH} (${DOCKER_PLATFORM}) ==="

ARTIFACT="${REPO_ROOT}/artifacts/${LIB_NAME}-${ARCH}-glibc${GLIBC_VERSION}.so"
if [[ ! -f "${ARTIFACT}" ]]; then
    echo "ERROR: Artifact not found: ${ARTIFACT}"
    exit 1
fi
if [[ ! -f "${MMDB_SOURCE}" ]]; then
    echo "ERROR: MaxMind-DB fixture missing: ${MMDB_SOURCE}"
    echo "Ensure the submodule is checked out (GIT_SUBMODULE_STRATEGY=recursive)."
    exit 1
fi
cp "${ARTIFACT}" "${SCRIPT_DIR}/plugin.so"
cp "${MMDB_SOURCE}" "${SCRIPT_DIR}/GeoIP2-City-Test.mmdb"

cleanup() {
    docker rm -f smoke-hub 2>/dev/null || true
    rm -f "${SCRIPT_DIR}/plugin.so" "${SCRIPT_DIR}/GeoIP2-City-Test.mmdb"
}
trap cleanup EXIT

echo "--- Building smoke image (${DOCKER_PLATFORM}) ---"
docker build \
    --platform "${DOCKER_PLATFORM}" \
    --build-arg "HUB_TAG=${HUB_TAG}" \
    -t "smoke-hub:${ARCH}" \
    -f "${SCRIPT_DIR}/Dockerfile.hub" \
    "${SCRIPT_DIR}"

echo "--- Starting hub ---"
docker run -d --name smoke-hub \
    --platform "${DOCKER_PLATFORM}" \
    "smoke-hub:${ARCH}"

echo "--- Waiting for hub to listen ---"
for i in $(seq 1 30); do
    if [ "$(docker inspect -f '{{.State.Running}}' smoke-hub 2>/dev/null)" != "true" ]; then
        echo "FAIL: container exited unexpectedly"
        docker logs smoke-hub
        exit 1
    fi
    if docker logs smoke-hub 2>&1 | grep -q "listening"; then
        echo "Hub is listening"
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "FAIL: hub did not start listening within 30s"
        docker logs smoke-hub
        exit 1
    fi
    sleep 1
done

echo "--- Verifying plugin load ---"
if ! docker logs smoke-hub 2>&1 | grep -q "plugin loaded"; then
    echo "FAIL: 'plugin loaded' not found in logs"
    docker logs smoke-hub
    exit 1
fi

docker logs smoke-hub 2>&1 | grep "plugin loaded"
echo "PASS - smoke test (${ARCH})"
