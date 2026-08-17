# OPS CLI Windows 安装脚本
# 用法: powershell -ExecutionPolicy Bypass -c "irm https://get.ops.autos/install.ps1 | iex"
# 无需管理员权限: 安装到 %LOCALAPPDATA%\ops\bin 并写入用户 PATH

$ErrorActionPreference = "Stop"

# 配置部分
$Repo = "ops3000/ops-cli"
$AssetName = "ops-windows-amd64"
$InstallDir = Join-Path $env:LOCALAPPDATA "ops\bin"

# PowerShell 5.1 默认不启用 TLS 1.2, GitHub 会拒绝连接
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# 检测架构 (目前只发布 amd64; ARM64 Windows 可通过 x64 模拟运行)
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne "AMD64" -and $arch -ne "ARM64") {
    Write-Error "Unsupported architecture: $arch"
    exit 1
}

# tar.exe: Windows 10 1803+ 自带
if (-not (Get-Command tar.exe -ErrorAction SilentlyContinue)) {
    Write-Error "tar.exe not found. Windows 10 1803+ is required."
    exit 1
}

Write-Host "Detected platform: windows/$($arch.ToLower())"

# 获取最新版本的 Tag
$release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$tag = $release.tag_name
if (-not $tag) {
    Write-Error "Failed to resolve the latest release tag"
    exit 1
}

$url = "https://github.com/$Repo/releases/download/$tag/$AssetName.tar.gz"
Write-Host "Downloading ops $tag ..."

$tmp = Join-Path $env:TEMP "ops-install-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $tarball = Join-Path $tmp "$AssetName.tar.gz"
    Invoke-WebRequest -Uri $url -OutFile $tarball
    tar.exe -xzf $tarball -C $tmp
    if ($LASTEXITCODE -ne 0) { throw "Failed to extract $tarball" }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Move-Item -Force (Join-Path $tmp "ops.exe") (Join-Path $InstallDir "ops.exe")
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# 写入用户 PATH (仅在缺失时)
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ';') -notcontains $InstallDir) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to your user PATH"
}
# 当前会话立即可用
if (($env:Path -split ';') -notcontains $InstallDir) {
    $env:Path = "$env:Path;$InstallDir"
}

Write-Host ""
& (Join-Path $InstallDir "ops.exe") version
Write-Host "ops installed to $InstallDir" -ForegroundColor Green
Write-Host "Open a new terminal (or re-login) for PATH changes to apply everywhere."
