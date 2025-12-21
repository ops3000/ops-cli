#!/bin/sh
set -e

# 配置部分
REPO="ops3000/ops-cli"
BINARY_NAME="ops"
INSTALL_DIR="/usr/local/bin"

# 检测系统
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# 映射架构名称
if [ "$ARCH" = "x86_64" ]; then
    ARCH="amd64"
elif [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
    ARCH="arm64"
else
    echo "Unsupported architecture: $ARCH"
    exit 1
fi

# 映射 Asset 名称前缀
# 对应 release.yml 中的 asset_name: ops-linux-amd64 / ops-darwin-amd64
if [ "$OS" = "linux" ]; then
    ASSET_PREFIX="ops-linux"
elif [ "$OS" = "darwin" ]; then
    ASSET_PREFIX="ops-darwin"
else
    echo "Unsupported OS: $OS"
    exit 1
fi

ASSET_NAME="${ASSET_PREFIX}-${ARCH}"

echo "Detected platform: $OS/$ARCH"

# 获取最新版本的 Tag
LATEST_TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo "Failed to fetch latest version info."
    exit 1
fi

echo "Latest version: $LATEST_TAG"

# 构造下载链接 (现在下载 .tar.gz)
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/${ASSET_NAME}.tar.gz"

echo "Downloading $DOWNLOAD_URL ..."

# 创建临时目录
TMP_DIR=$(mktemp -d)
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

# 1. 下载压缩包
curl -L -o "$TMP_DIR/ops.tar.gz" "$DOWNLOAD_URL"

# 2. 解压 (包内包含一个名为 'ops' 的二进制文件)
tar -xzf "$TMP_DIR/ops.tar.gz" -C "$TMP_DIR"

SOURCE_BINARY="$TMP_DIR/ops"

if [ ! -f "$SOURCE_BINARY" ]; then
    echo "Error: Binary 'ops' not found inside the archive."
    exit 1
fi

# 3. 安装
# 如果当前用户不是 root 且目录不可写，尝试使用 sudo
echo "Installing to $INSTALL_DIR..."

if [ ! -w "$INSTALL_DIR" ]; then
    echo "Permission needed to write to $INSTALL_DIR. Using sudo..."
    sudo mv "$SOURCE_BINARY" "$INSTALL_DIR/$BINARY_NAME"
    sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
else
    mv "$SOURCE_BINARY" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
fi

echo ""
echo "✅ ops has been installed to $INSTALL_DIR/$BINARY_NAME"
echo "Run 'ops --help' to get started."