# winget 清单（microsoft/winget-pkgs 提交流程）

本目录存放提交到 [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) 的清单，
基于 GitHub Release 已发布的 MSI / NSIS 安装包。

## 提交流程

1. 发版后更新 `manifests/` 下对应版本目录中的 `PackageVersion`、`InstallerUrl` 与
   `InstallerSha256`（sha256 取自 Release 的 `checksums.txt`）。
2. 本地校验：

   ```powershell
   winget validate --manifest packaging\winget\manifests\o\ouyangjiahong26\altgo\<版本>\
   ```

3. fork `microsoft/winget-pkgs`，把清单放到
   `manifests/o/ouyangjiahong26/altgo/<版本>/`，提 PR。也可用
   [wingetcreate](https://github.com/microsoft/winget-create) 生成并直接发起 PR：

   ```powershell
   wingetcreate new https://github.com/ouyangjiahong26/altgo/releases/download/v<版本>/altgo_<版本>_x64-setup.exe
   ```

## 注意

- winget 要求首个提交的包先通过人工审核，之后的版本更新可走自动化。
- MSI 的 `ProductCode` 每次构建都会变（Tauri 未固定），如改用 MSI 清单需逐版本更新。
- 目前 Release 仅有 x64 安装包；待 Windows arm64 构建（#131）落地后，在
  installer 清单中追加 `Architecture: arm64` 条目即可。

# winget Manifests (microsoft/winget-pkgs Submission)

This directory holds the manifests submitted to [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs),
based on the MSI / NSIS installers already published on GitHub Release.

## Submission Process

1. After a release, update `PackageVersion`, `InstallerUrl`, and
   `InstallerSha256` in the corresponding version directory under `manifests/`
   (the sha256 comes from the release's `checksums.txt`).
2. Validate locally:

   ```powershell
   winget validate --manifest packaging\\winget\\manifests\\o\\ouyangjiahong26\\altgo\\<version>\\
   ```

3. Fork `microsoft/winget-pkgs`, put the manifests under
   `manifests/o/ouyangjiahong26/altgo/<version>/`, and open a PR. Alternatively use
   [wingetcreate](https://github.com/microsoft/winget-create) to generate them and open the PR directly:

   ```powershell
   wingetcreate new https://github.com/ouyangjiahong26/altgo/releases/download/v<version>/altgo_<version>_x64-setup.exe
   ```

## Notes

- winget requires a newly submitted package to pass manual review first; later version updates can be automated.
- The MSI's `ProductCode` changes on every build (Tauri does not pin it), so an MSI manifest would need per-version updates.
- Currently only x64 installers are published on Release; once the Windows arm64 build (#131) lands, add an
  `Architecture: arm64` entry to the installer manifest.
