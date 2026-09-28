# Scoop 清单（ScoopInstaller/Extras 提交流程）

`altgo.json` 是提交到 [ScoopInstaller/Extras](https://github.com/ScoopInstaller/Extras)
的清单，直接复用 Release 的 NSIS 安装包（Scoop 用 7zip 解压 NSIS，`altgo.exe` 位于根目录，
已实际验证）。

## 提交流程

1. fork `ScoopInstaller/Extras`，把 `altgo.json` 放到 `bucket/` 目录，提 PR。
2. 清单含 `checkver`（GitHub release）与 `autoupdate`（sha256 从 Release 的
   `checksums.txt` 提取），之后发版由 Extras 的自动更新机器人跟进，无需手工维护。

## 注意

- `pre_install` 清理 NSIS 解压残留（`$PLUGINSDIR`、卸载器），这是 Extras 中
  NSIS 包的通行写法。
- 待 Windows arm64 构建（#131）落地后，在 `architecture` 下追加 `arm64` 条目即可。
