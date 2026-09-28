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
