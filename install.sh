#!/bin/sh
set -e

# 配置部分
REPO="ops3000/ops-cli" # <--- 修改这里
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

# 映射系统名称 (目前只编译了 linux 和 darwin)
if [ "$OS" = "linux" ]; then
    TARGET="linux-$ARCH"
elif [ "$OS" = "darwin" ]; then
    TARGET="darwin-$ARCH"
else
    echo "Unsupported OS: $OS"
    exit 1
fi

echo "Detected platform: $OS/$ARCH"

# 获取最新版本的 Tag
LATEST_TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo "Failed to fetch latest version info."
    exit 1
fi

echo "Latest version: $LATEST_TAG"

# 构造下载链接
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/${BINARY_NAME}-${TARGET}"

echo "Downloading $DOWNLOAD_URL ..."

# 下载并安装
# 如果当前用户不是 root 且目录不可写，尝试使用 sudo
if [ ! -w "$INSTALL_DIR" ]; then
    echo "Permission needed to write to $INSTALL_DIR. Using sudo..."
    sudo curl -L -o "$INSTALL_DIR/$BINARY_NAME" "$DOWNLOAD_URL"
    sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
else
    curl -L -o "$INSTALL_DIR/$BINARY_NAME" "$DOWNLOAD_URL"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
fi

echo ""
echo "✅ ops has been installed to $INSTALL_DIR/$BINARY_NAME"
echo "Run 'ops --help' to get started."