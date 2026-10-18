# Windows 代码签名

本文记录 Windows 安装包的 Authenticode 签名方案、SignPath Foundation 的申请条件，以及审核通过后发布流程的改造方案。面向维护者，不是用户手册。用户可见的签名政策写在仓库根目录 [README.md](../README.md) 的 Code signing policy 一节，隐私政策在文档站[隐私政策](../docs-site/docs/privacy.mdx)。

## 现状与目标

v2.7.8 的 Release 安装包没有 Authenticode 签名。`Get-AuthenticodeSignature` 返回 `NotSigned`，`SignerCertificate` 为空，安装包版本资源里的 `CompanyName` 也为空。用户双击安装会看到 SmartScreen 的来源不明提示，开启智能应用控制的机器会被直接阻止。issue #274 记录了完整复现与渠道对比。

目标是从签名生效的下一个 Release 开始，x64 与 arm64 安装包由同一份长期稳定的签名身份签名，签名带可信时间戳，发布流程在产物未签名时失败。签名身份一次选定后不再更换，避免 SmartScreen 信誉归零。

## 选定渠道

选定 SignPath Foundation 的免费开源签名。证书签发给 SignPath Foundation，私钥保存在 SignPath 的 HSM 里，通过 SignPath GitHub App 与 `signpath/github-action-submit-signing-request` 接入。它不要求法律实体、付费或硬件令牌，适合本项目。

代价有三点。Windows 里显示的发布者是 SignPath Foundation 而不是 altgo。每次发布要在 SignPath 里人工批准签名请求。项目能否通过审核由 SignPath 判断，理由是项目声誉与可验证的构建。

其他路线只作备选。自购 OV 证书要求身份核验与付费，私钥必须在云 HSM 或硬件令牌里，发布者显示证书主体。Azure Artifact Signing 的 Public Trust 证书对组织限美国、加拿大、欧盟、英国、澳大利亚、新西兰、日本、韩国、新加坡、瑞士、挪威、以色列，对个人限美国与加拿大，国内主体不符合。EV 证书自 2024 年起没有 SmartScreen 直通，不值得为免提示多花钱。

## SignPath 的申请条件

依据 <https://signpath.org/terms>：

- 项目使用 OSI 认可的开源许可，没有商业双许可与专有组件。本项目是 MIT。
- 项目无恶意代码，活跃维护，已经发布，并在下载页或应用商店说明功能。
- 项目主页有 Code signing policy 段落，写明 `Free code signing provided by SignPath.io, certificate by SignPath Foundation`，列出 Committers and reviewers 与 Approvers，并给出隐私政策。
- 被签名的二进制来自本仓库的可复核构建，所有前置 job 跑在 GitHub 托管 runner 上。
- 每个签名请求由 Approver 人工批准，每个版本都要批准。
- 被签名的二进制有统一的 ProductName 与版本元数据。当前 NSIS 安装包的 ProductName 为 `altgo`，ProductVersion 与 FileVersion 都等于 Release 版本。

README 已补政策段与角色，文档站已有隐私政策页。

## 申请材料

申请表单与 SignPath 界面都是英文，下面是一份草稿，提交前按实际情况修改。

```text
Project name: altgo
Repository: https://github.com/ouyangjiahong26/altgo
License: MIT
Download page: https://ouyangjiahong26.github.io/altgo/docs/quick-start
Documentation: https://ouyangjiahong26.github.io/altgo/
Code signing policy: https://github.com/ouyangjiahong26/altgo#code-signing-policy
Privacy policy: https://ouyangjiahong26.github.io/altgo/docs/privacy
Maintainer: ouyangjiahong26

Description: altgo is a desktop speech-to-text app built with Tauri, Rust and React.
It supports Windows 10+ and Ubuntu 22.04+ on x86_64 and arm64. While a configurable
key is held it records audio, then transcribes with a bundled local SenseVoice model
or an online MiMo ASR backend, optionally polishes the text with an LLM, and writes
the result to the clipboard.

Why code signing: the Windows NSIS installers are currently unsigned, so SmartScreen
labels them as an unknown app. Every release is built by the public GitHub Actions
release workflow from the tagged source. Signing would let users verify that the
installer was built from this repository and would replace the unknown publisher
label with a verifiable one.
```

Download page 用文档站的快速开始页面，因为 GitHub Releases 页面不会提到 SignPath Foundation，不满足表单对 Download URL 的要求。

## 申请步骤

1. 先合并 README 的 Code signing policy、文档站的隐私政策页与本文，等文档站部署完成，确认 README 的锚点与 `/docs/privacy` 都能公开访问。SignPath 审核会检查公开页面。
2. 打开 <https://signpath.org/apply> 填申请表。表单由 HubSpot 托管，不需要先登录，也不要求先有 SignPath 账号。材料用上一节的英文草稿。
3. 等 SignPath 回复。审核期间如果需要补充项目信息或说明构建方式，按邮件要求提供。
4. 收到开通通知后登录 SignPath，确认组织信息。登录方式以对方邮件说明为准。
5. 安装 SignPath GitHub App 到 `ouyangjiahong26/altgo` 仓库，需要仓库管理员在浏览器里授权。
6. 在 SignPath 的组织设置里把 GitHub.com 加为 trusted build system。
7. 创建 project，slug 用 `altgo`。
8. 创建 signing policy，slug 用 `release`。
9. 创建 artifact configuration，slug 用 `nsis-installer`，内容见下一节。
10. 生成有 submitter 权限的 API token，存进仓库 secret `SIGNPATH_API_TOKEN`。组织 ID 存进仓库变量 `SIGNPATH_ORGANIZATION_ID`。
11. 在签名流程跑通前不改 `.github/workflows/release.yml`，避免缺少 secret 时 CI 失败。

## Artifact configuration

签名请求从 GitHub workflow artifact 读取未签名安装包。上传用 `actions/upload-artifact` 的默认 zip 形式，artifact configuration 的根节点是 `zip-file`：

```xml
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <parameters>
    <parameter name="version" required="true"/>
  </parameters>
  <zip-file>
    <pe-file path="*-setup.exe" product-name="altgo" product-version="${version}" file-version="${version}">
      <authenticode-sign/>
    </pe-file>
  </zip-file>
</artifact-configuration>
```

提交签名请求时把 `version` 参数传成 Release 版本号。版本或产品名对不上时 SignPath 拒绝签名，这能防止把错误版本签出去。

## release.yml 改造方案

审核通过后按下面的方式改 `.github/workflows/release.yml`。签名放在 build-windows 的 matrix job 内，每个架构各提交一次签名请求，release job 的产物组装环节不变，但下载产物时排除未签名中间件。

### build-windows

给 job 加 permissions，允许 SignPath action 读取 workflow artifact：

```yaml
    permissions:
      actions: read
      contents: read
```

在现有“构建 NSIS 安装包”步骤之后加入这几步，原来的“上传 Windows 安装包产物”步骤保留，它上传替换后的已签名安装包与重新生成的 `.sig`：

```yaml
      - name: 读取版本号
        id: version
        shell: bash
        run: echo "value=${GITHUB_REF_NAME#v}" >> "$GITHUB_OUTPUT"

      - name: 暂存未签名安装包
        shell: bash
        run: |
          mkdir -p unsigned
          cp src-tauri/target/${{ matrix.target }}/release/bundle/nsis/*-setup.exe unsigned/

      - name: 上传未签名安装包
        id: upload-unsigned
        uses: actions/upload-artifact@v7
        with:
          name: altgo-windows-${{ matrix.arch }}-unsigned
          path: unsigned/*-setup.exe
          if-no-files-found: error

      - name: 提交 SignPath 签名请求
        id: signpath
        uses: signpath/github-action-submit-signing-request@v3
        with:
          api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
          organization-id: ${{ vars.SIGNPATH_ORGANIZATION_ID }}
          project-slug: altgo
          signing-policy-slug: release
          artifact-configuration-slug: nsis-installer
          github-artifact-id: ${{ steps.upload-unsigned.outputs.artifact-id }}
          wait-for-completion: true
          wait-for-completion-timeout-in-seconds: 3600
          output-artifact-directory: signed
          parameters: |
            version: "${{ steps.version.outputs.value }}"

      - name: 替换为已签名安装包并重签 updater
        shell: pwsh
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_KEY_PASSWORD }}
          VERSION: ${{ steps.version.outputs.value }}
        run: |
          $signed = Get-ChildItem signed -Filter *-setup.exe -Recurse | Select-Object -First 1
          if (-not $signed) { throw '没有找到签名后的安装包' }
          $target = "src-tauri/target/${{ matrix.target }}/release/bundle/nsis/$($signed.Name)"
          Copy-Item $signed.FullName $target -Force
          Remove-Item "$target.sig" -ErrorAction SilentlyContinue
          npm --prefix frontend exec -- tauri signer sign --app-version $env:VERSION $target
          $status = (Get-AuthenticodeSignature $target).Status
          if ($status -ne 'Valid') { throw "Authenticode 校验失败：$status" }
```

### release

把“下载全部 artifact”改成按名字下载，避免把未签名中间件当成发布产物：

```yaml
      - name: 下载发布产物
        uses: actions/download-artifact@v8
        with:
          pattern: altgo-*-packages
          path: release-assets
          merge-multiple: true

      - name: 下载 AUR 产物
        uses: actions/download-artifact@v8
        with:
          name: altgo-aur-pkgbuild
          path: release-assets
```

`altgo-*-packages` 匹配 Linux 的 `altgo-amd64-packages`、`altgo-aarch64-packages` 与 Windows 的 `altgo-windows-x64-packages`、`altgo-windows-arm64-packages`。AUR 的 `altgo-aur-pkgbuild` 单独下载。未签名 artifact 的名字是 `altgo-windows-x64-unsigned` 与 `altgo-windows-arm64-unsigned`，不在发布产物里。

### 为什么签名后要重签 updater 的 .sig

Tauri 在 `tauri build` 结束时对安装包计算 updater 的 minisign 签名，写到 `<安装包>.sig`。SignPath 在打包完成之后才修改安装包，文件内容一变，原来的 `.sig` 就不再匹配。改造里先删掉旧 `.sig`，再用 `tauri signer sign --app-version <版本>` 对签名后的文件重新生成。`--app-version` 把版本写进签名的 trusted comment，与 `latest.json` 里的版本对应。

### 想只批准一次

上面的方案每个 Release 需要批准两次签名请求。如果想合并成一次，在 build-windows 之后加一个独立的 windows-latest job，下载两个架构的未签名安装包，合并成一个 artifact 提交一次签名请求，下载签名产物后分别重签 updater，再上传成 `altgo-windows-x64-packages` 与 `altgo-windows-arm64-packages`。这样改造更大，收益是少一次人工批准。

## 发版流程

1. 推送 `v*` tag，validate、test、build-linux、build-windows 依次执行。
2. build-windows 跑到签名请求后会在 SignPath 里生成待批准请求，CI 等待最多 3600 秒。
3. 在 SignPath 里核对构建来源与版本，批准 x64 与 arm64 两个请求。
4. CI 下载签名产物，替换安装包，重签 `.sig`，校验 Authenticode 状态，上传最终产物。
5. release job 生成 `checksums.txt` 与 `latest.json`，创建 GitHub Release。

签名请求超时或拒绝会让 build-windows 失败，Release 不会生成。

## 验证清单

- [ ] 从 Release 下载 x64 与 arm64 的 `*-setup.exe`，`Get-AuthenticodeSignature` 的 `Status` 为 `Valid`，`SignerCertificate` 的主体是 SignPath Foundation，签名带时间戳。
- [ ] 安装包的数字签名页显示 SignPath Foundation，SmartScreen 与 UAC 提示不再显示未知发布者。首次下载仍可能出现提示，但会显示发布者。
- [ ] `checksums.txt` 里的哈希与下载文件一致。
- [ ] 从上一版本触发自动更新，`latest.json` 的签名通过校验，安装成功。
- [ ] `release.yml` 在缺少签名或 Authenticode 校验失败时让 job 失败。
- [ ] winget 与 Scoop 清单里的 `InstallerSha256` 随新签名安装包更新。

## 接入后的收尾

- README 的 Code signing policy 改为已生效的措辞，删除“接入完成后”的前提。
- 文档站 `quick-start.mdx` 的 Windows 小节改写，删除未签名提示与智能应用控制说明，保留哈希核对。
- `CONTRIBUTING.md` 的 Release 一节补签名请求批准与 secret 名称。
- CHANGELOG 增加一条签名相关的条目。
- 视情况写 ADR 记录签名渠道与身份不复用的决定。
- 关闭 issue #274。

## 风险与未决

- 申请可能被拒，理由通常是项目声誉不足。被拒后改走自购 OV 证书，需要重新设计接入方式，见 issue #274 的渠道对比。
- 每次发布的人工批准会成为发版流程的新增步骤，approver 不在线时 CI 会等到超时。
- 签名身份一旦更换，SmartScreen 信誉归零，申请前就要确认长期使用 SignPath Foundation。
