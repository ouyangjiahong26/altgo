# ADR-0015：deb 安装包在应用内经 polkit 授权安装

- **状态**：已接受（2026-10-30）
- **日期**：2026-10-30
- **受影响模块**：`src-tauri/src/deb_update.rs`, `src-tauri/src/updater.rs`, `src-tauri/src/cmd.rs`, `frontend/`
- **修订**：ADR-0004 第 2 条的外部引导档

## 背景与问题

ADR-0004 把 Linux 的 deb、rpm、AUR 统一归为外部引导档：检测到新版本后只展示更新说明，用户自己去 GitHub Release 页面下载安装包。deb 用户占 Linux 用户的大头，这条路径要求用户认出正确的架构、下载 deb、再在终端执行 `apt install`，比 AppImage 的就地更新多出好几步，实际升级率低。

GitHub Release 已经发布两种架构的 deb 和 `checksums.txt`，资产命名稳定（`altgo_<version>_amd64.deb` 与 `altgo_<version>_arm64.deb`），无需改动发布流程就能定位。

## 决策

1. **新增 deb 直装档（`deb_package`）**：运行中的可执行文件由名为 `altgo` 的 dpkg 包安装，且系统具备 `dpkg-query`、`apt-get`、`pkexec` 时，检查更新的支持级别为 `deb_package`。只按当前可执行文件的 dpkg 归属判断，不只看 dpkg 是否存在，避免 AUR、rpm 与手动安装的二进制被 deb 覆盖后失去原包管理器记录。AppImage 仍先按 `APPIMAGE` 环境变量判为就地更新。

2. **下载与校验**：从 `https://github.com/ouyangjiahong26/altgo/releases/download/v<version>/checksums.txt` 取官方 SHA-256，再用共用下载内核（`download::fetch_file`）下载对应架构的 deb 到 `<缓存目录>/altgo/updates/`。校验和与体积下限都通过才落盘，已落盘且哈希一致的文件直接复用。进度经 `update-install-progress` 事件推到更新说明窗。

3. **安装经 pkexec 调用 apt-get**：`pkexec apt-get install -y --allow-downgrades <deb>`。授权对话框由 polkit 认证代理弹出，要求输入用户密码，与 `sudo` 同一套凭据。应用进程不读取、不转发密码，也不以 root 运行自己。`pkexec` 的 126、127 退出码分别映射为“取消授权”与“缺少认证代理”，其他失败把 apt-get 错误尾部展示在窗口内，并附带手动安装命令。

4. **安装成功后由用户重启**：命令返回成功后窗口显示“安装完成”和重启按钮，点击调用 `restart_app`（`AppHandle::restart`），不做静默自动重启。就地更新档保持原有行为，安装后由 updater 插件自行重启。

5. **失败不降级为跳转页面**：deb 档操作区不再提供“打开下载页面”按钮，失败时给出错误与手动命令。rpm/AUR 仍是外部引导档。

## 后果与权衡

### 正向收益
- deb 用户从“发现新版本”到“装好”只需两次确认：下载，然后在系统授权对话框里输入密码。
- 安装包经过发布校验和与 TLS 双重重校验，下载中断、截断与错版本都会被拒绝。
- 授权由系统对话框完成，应用不接触密码，也不需要维护一份特权 helper。

### 代价与局限
- 没有 polkit 认证代理的桌面会话（部分窗口管理器）无法弹授权框，此时只能按错误提示手动安装。
- 更新安装依赖 `apt-get` 的依赖解析，需要本机软件源可用。deb 本身没有 Ed25519 签名，完整性依赖 GitHub 发布通道与 `checksums.txt`，与 AppImage/Windows 的 minisign 校验强度不同。
- 更新说明窗新增三个状态（下载、授权、已安装），窗口状态机比就地更新档复杂。
