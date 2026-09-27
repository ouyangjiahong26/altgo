#!/usr/bin/env bash
# Generate PKGBUILD and .SRCINFO from template.
# Usage: ./generate-pkgbuild.sh <version> [path-to-amd64-deb] [path-to-arm64-deb]
# Output: PKGBUILD and .SRCINFO in current directory

set -euo pipefail

VERSION="${1:?Usage: generate-pkgbuild.sh <version> [path-to-amd64-deb] [path-to-arm64-deb]}"
AMD64_DEB="${2:-}"
ARM64_DEB="${3:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKGBUILD_TEMPLATE="${SCRIPT_DIR}/PKGBUILD.in"
SRCINFO_TEMPLATE="${SCRIPT_DIR}/SRCINFO.in"

echo "[INFO] Generating PKGBUILD for v${VERSION}..."

checksum_for_deb() {
    local arch="$1"
    local local_deb="$2"
    local deb_url="https://github.com/ouyangjiahong26/altgo/releases/download/v${VERSION}/altgo_${VERSION}_${arch}.deb"

    if [[ -n "${local_deb}" && -f "${local_deb}" ]]; then
        echo "[INFO] Using local ${arch} deb: ${local_deb}" >&2
        sha256sum "${local_deb}" | cut -d' ' -f1
        return
    fi

    local tmp_deb
    tmp_deb=$(mktemp)
    trap 'rm -f "${tmp_deb}"' RETURN
    echo "[INFO] Downloading ${arch} deb for checksum..." >&2
    curl --fail --progress-bar -L -o "${tmp_deb}" "${deb_url}" >&2
    sha256sum "${tmp_deb}" | cut -d' ' -f1
}

AMD64_SHA256=$(checksum_for_deb "amd64" "${AMD64_DEB}")
ARM64_SHA256=$(checksum_for_deb "arm64" "${ARM64_DEB}")
echo "[OK] amd64 sha256: ${AMD64_SHA256}"
echo "[OK] arm64 sha256: ${ARM64_SHA256}"

render_template() {
    sed \
        -e "s/VERSION/${VERSION}/g" \
        -e "s/AMD64_SHA256_PLACEHOLDER/${AMD64_SHA256}/g" \
        -e "s/ARM64_SHA256_PLACEHOLDER/${ARM64_SHA256}/g" \
        "$1"
}

render_template "${PKGBUILD_TEMPLATE}" > PKGBUILD
render_template "${SRCINFO_TEMPLATE}" > .SRCINFO
echo "[OK] Generated PKGBUILD and .SRCINFO"
