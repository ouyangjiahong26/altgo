#requires -version 5.1
# 把离线资产包里的模型与润色运行时放进 altgo 的配置目录。
# 只做校验与复制：不改 altgo.toml，不覆盖已有配置，也不自动拉起安装器，
# 装应用与 WebView2 的顺序由使用者掌握，脚本结尾把顺序打印出来。

[CmdletBinding()]
param(
    # 默认与应用的路径解析一致：dirs::config_dir() 在 Windows 上是 %APPDATA%。
    [string]$Destination = (Join-Path $env:APPDATA 'altgo')
)

$ErrorActionPreference = 'Stop'
$Root = $PSScriptRoot
$AssetDirectories = @('models', 'polish-models', 'runtime')

function Test-PackIntegrity {
    $sumsPath = Join-Path $Root 'SHA256SUMS'
    if (-not (Test-Path -LiteralPath $sumsPath)) {
        throw '找不到 SHA256SUMS，请确认解压完整'
    }
    $problems = @()
    foreach ($line in Get-Content -LiteralPath $sumsPath) {
        if (-not $line.Trim()) { continue }
        $parts = $line -split '\s+', 2
        $expected = $parts[0].ToLower()
        $relative = $parts[1].Trim()
        $file = Join-Path $Root ($relative -replace '/', '\')
        if (-not (Test-Path -LiteralPath $file)) {
            $problems += "缺失 $relative"
            continue
        }
        $actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $expected) { $problems += "哈希不符 $relative" }
    }
    if ($problems.Count -gt 0) {
        $problems | ForEach-Object { Write-Host "校验失败：$_" }
        throw '包内文件与 SHA256SUMS 不符，已停止放置'
    }
    Write-Host '校验通过：包内文件与 SHA256SUMS 一致'
}

function Copy-AssetDirectory {
    $added = 0
    $unchanged = 0
    $replaced = 0
    foreach ($name in $AssetDirectories) {
        $source = Join-Path $Root $name
        if (-not (Test-Path -LiteralPath $source)) { continue }
        $target = Join-Path $Destination $name
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        foreach ($file in Get-ChildItem -LiteralPath $source -Recurse -File) {
            $relative = $file.FullName.Substring($source.Length).TrimStart('\')
            $copyTo = Join-Path $target $relative
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $copyTo) | Out-Null
            if (Test-Path -LiteralPath $copyTo) {
                $sourceHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
                $targetHash = (Get-FileHash -LiteralPath $copyTo -Algorithm SHA256).Hash
                if ($sourceHash -eq $targetHash) {
                    $unchanged++
                    continue
                }
                Copy-Item -LiteralPath $file.FullName -Destination $copyTo -Force
                Write-Host "已替换同名文件：$name\$relative"
                $replaced++
                continue
            }
            Copy-Item -LiteralPath $file.FullName -Destination $copyTo -Force
            $added++
        }
    }
    Write-Host "放置完成：新增 $added 个，替换 $replaced 个，跳过 $unchanged 个相同文件"
}

# 失败时只打印一句话再退出，不把 PowerShell 的异常堆栈甩给使用者看。
try {
    Test-PackIntegrity
    Copy-AssetDirectory
} catch {
    Write-Host ''
    Write-Host "放置未完成：$($_.Exception.Message)"
    exit 1
}

Write-Host "目标目录：$Destination"
Write-Host ''

# Windows 11（内部版本 22000 起）自带 WebView2，Windows 10 需要先装。
if (Test-Path -LiteralPath (Join-Path $Root 'webview2')) {
    $build = [int](Get-ItemPropertyValue 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' 'CurrentBuildNumber')
    if ($build -lt 22000) {
        Write-Host '下一步 1：运行 webview2\MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
    } else {
        Write-Host '本机是 Windows 11，自带 WebView2，可跳过 webview2 目录'
    }
}
Write-Host '下一步：运行 altgo 的安装器（altgo_*_x64-setup.exe），从开始菜单启动'
Write-Host '启动后在设置里把转写模型选为 sense-voice、润色引擎选本地档，模型已在位，无需联网下载'
