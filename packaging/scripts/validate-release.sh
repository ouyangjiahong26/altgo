#!/usr/bin/env bash
# 校验 Release tag 与各构建入口的版本一致，并要求 CHANGELOG 有对应版本小节。
set -euo pipefail

TAG="${GITHUB_REF_NAME:?缺少 GITHUB_REF_NAME}"
if [[ ! "${TAG}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]]; then
    echo "Release tag 必须是 vMAJOR.MINOR.PATCH（可带预发布或构建后缀）：${TAG}" >&2
    exit 1
fi

VERSION="${TAG#v}"
METADATA_FILE="$(mktemp)"
trap 'rm -f "${METADATA_FILE}"' EXIT

cargo metadata \
    --manifest-path src-tauri/Cargo.toml \
    --no-deps \
    --format-version 1 >"${METADATA_FILE}"

CARGO_VERSION="$(node -e '
const fs = require("fs");
const metadata = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const packageInfo = metadata.packages.find((item) => item.name === "altgo-tauri");
if (!packageInfo) process.exit(1);
process.stdout.write(packageInfo.version);
' "${METADATA_FILE}")"
CONFIG_VERSION="$(node -e 'process.stdout.write(require("./src-tauri/tauri.conf.json").version)' )"
FRONTEND_VERSION="$(node -e 'process.stdout.write(require("./frontend/package.json").version)' )"
# package-lock.json 顶层与 packages[""] 两个 version 都由 npm 依据 package.json 写入，发版时须一并跟上。
# 只改 package.json 会让锁文件版本长期滞后——npm ci 不校验该字段，故不会报错，容易被漏掉。
LOCK_TOP_VERSION="$(node -e 'process.stdout.write(require("./frontend/package-lock.json").version)' )"
LOCK_ROOT_VERSION="$(node -e '
const lock = require("./frontend/package-lock.json");
const root = lock.packages && lock.packages[""];
process.stdout.write((root && root.version) || "");
')"

for entry in "Cargo=${CARGO_VERSION}" "Tauri=${CONFIG_VERSION}" "Frontend=${FRONTEND_VERSION}" "FrontendLockTop=${LOCK_TOP_VERSION}" "FrontendLockRoot=${LOCK_ROOT_VERSION}"; do
    name="${entry%%=*}"
    version="${entry#*=}"
    if [[ "${version}" != "${VERSION}" ]]; then
        echo "${name} 版本 ${version} 与 tag ${TAG} 不一致" >&2
        exit 1
    fi
done

if ! grep -Fq "## v${VERSION} " CHANGELOG.md; then
    echo "CHANGELOG.md 缺少 v${VERSION} 版本小节" >&2
    exit 1
fi

echo "Release ${TAG} 版本校验通过（Cargo/Tauri/Frontend/FrontendLockTop/FrontendLockRoot/CHANGELOG）"
