# 发布产物精简：更新产物改用 v2 原生形态，Windows 只保留 NSIS 安装包

v2.6.22 的 Release 挂了 32 个发布产物，其中约三分之一是重复内容或无消费者的构建中间物，根因是 `bundle.createUpdaterArtifacts` 配置成了 `"v1Compatible"`。该模式只为让 Tauri v1 时代的客户端继续可更新而存在，而 altgo 自首个版本起就是 Tauri 2 应用，不存在 v1 客户端；它把 AppImage 的更新产物额外 gzip 成 `.AppImage.tar.gz`，与人类下载的裸 `.AppImage` 内容相同，每版多发布约 185 MB。决定：

- `createUpdaterArtifacts` 改为 `true`。Linux 的更新产物直接使用裸 `.AppImage`（Tauri v2 原生形态），`.AppImage.tar.gz` 及其签名不再生成、不再上传；`merge-updater-json.sh` 的匹配规则同步修改。Windows 同理为 v2 原生形态：updater 产物就是签名后的 NSIS 安装包（`*-setup.exe` 与 `.sig`），只有 `v1Compatible` 模式才额外 gzip 出 `.nsis.zip`。已装客户端升级无损：v2 更新器按 latest.json 内的 URL 下载并用其内嵌签名校验，不关心产物是否 gzip。
- Windows 移除 MSI 打包，NSIS 安装器成为唯一形态。安装、卸载与自动更新 NSIS 已全覆盖，MSI 的独有价值只剩企业 GPO/SCCM 部署，本项目没有该场景的证据；winget 与 scoop 清单本就基于 NSIS 安装器。MSI 的更新产物 `.msi.zip` 在 latest.json 中本就没有条目，属于无消费者产物。
- 上传清单只保留有明确消费者的文件：deb、rpm、AUR 的 PKGBUILD/.SRCINFO、checksums.txt 与 latest.json 各自服务独立渠道，保留；`*-setup.exe.sig` 是 latest.json 的 Windows 签名输入，继续上传。

## 备选方案

- 保留 v1Compatible、只收紧上传清单：被否决。tar.gz 与裸 AppImage 双份发布的根因不除，每版约 185 MB 的浪费照旧重现。
- 保留 `.msi` 作手动安装器、仅删 `.msi.zip`：被否决。省一半体积但留下双安装器并存的维护与说明成本；未来若出现真实的企业部署需求，恢复 `msi` target 与工作流条目即可。

## 后果

- latest.json 的 Linux 条目 URL 改指 `.AppImage`；已发布版本的 latest.json 不回改，旧客户端按各自版本的 URL 更新不受影响。
- 发布产物从 32 个降到 20 个（含 GitHub 自动生成的两个源码包），每版体积减少约 230 MB。
- 已用 MSI 安装的用户不受影响：其更新器本就下载 NSIS 更新包就地升级，不依赖 MSI 继续发布。
- 勘误（随 v2.7.0 发布修正）：初版决策误以为 `createUpdaterArtifacts: true` 下 Windows 仍生成 `.nsis.zip`——实际该形态仅 `v1Compatible` 模式生成，v2 原生的 updater 产物是签名安装包本体。`merge-updater-json.sh` 的匹配规则与工作流的上传、发布清单已随 v2.7.0 修正，`*-setup.exe.sig` 恢复上传。
