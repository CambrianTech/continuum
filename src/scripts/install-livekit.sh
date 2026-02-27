#!/bin/bash
# Install LiveKit server binary
#
# LiveKit is an open-source WebRTC SFU (Selective Forwarding Unit).
# This replaces our custom WebSocket call server with industry-standard
# encrypted WebRTC transport (DTLS-SRTP, Opus/VP8 codecs, congestion control).
#
# Usage: ./scripts/install-livekit.sh
#
# macOS: installs via Homebrew (brew install livekit)
# Linux: downloads binary to ~/.continuum/bin/livekit-server

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Check if already installed (brew or manual)
if command -v livekit-server &> /dev/null; then
    CURRENT_VERSION=$(livekit-server --version 2>/dev/null || echo "unknown")
    echo -e "${GREEN}LiveKit server already installed: $CURRENT_VERSION${NC}"
    echo -e "   Path: $(which livekit-server)"
    exit 0
fi

# Also check our manual install location
INSTALL_DIR="$HOME/.continuum/bin"
BINARY="$INSTALL_DIR/livekit-server"
if [ -f "$BINARY" ]; then
    CURRENT_VERSION=$("$BINARY" --version 2>/dev/null || echo "unknown")
    echo -e "${GREEN}LiveKit server already installed: $CURRENT_VERSION${NC}"
    echo -e "   Path: $BINARY"
    exit 0
fi

OS=$(uname -s | tr '[:upper:]' '[:lower:]')

if [ "$OS" = "darwin" ]; then
    # macOS: LiveKit provides official Homebrew formula (no GitHub release binaries for macOS)
    echo -e "${YELLOW}Installing LiveKit server via Homebrew...${NC}"

    if ! command -v brew &> /dev/null; then
        echo -e "${RED}Homebrew not found. Install from https://brew.sh${NC}"
        exit 1
    fi

    brew update --quiet
    brew install livekit

    VERSION=$(livekit-server --version 2>/dev/null || echo "unknown")
    echo -e "${GREEN}LiveKit server installed successfully${NC}"
    echo -e "   Version: $VERSION"
    echo -e "   Path: $(which livekit-server)"
else
    # Linux: download binary from GitHub releases
    echo -e "${YELLOW}Installing LiveKit server...${NC}"
    mkdir -p "$INSTALL_DIR"

    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64) ARCH="amd64" ;;
        aarch64|arm64) ARCH="arm64" ;;
        *)
            echo -e "${RED}Unsupported architecture: $ARCH${NC}"
            exit 1
            ;;
    esac

    # Fetch latest release version
    echo -e "${YELLOW}Fetching latest LiveKit server release...${NC}"
    LATEST_VERSION=$(curl -sL https://api.github.com/repos/livekit/livekit/releases/latest | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')

    if [ -z "$LATEST_VERSION" ]; then
        echo -e "${RED}Failed to fetch latest version. Using fallback v1.9.11${NC}"
        LATEST_VERSION="1.9.11"
    fi

    echo -e "   Version: v${LATEST_VERSION}"
    echo -e "   Platform: linux_${ARCH}"

    FILENAME="livekit_${LATEST_VERSION}_linux_${ARCH}.tar.gz"
    URL="https://github.com/livekit/livekit/releases/download/v${LATEST_VERSION}/${FILENAME}"

    echo -e "${YELLOW}Downloading from: $URL${NC}"
    TMPDIR=$(mktemp -d)
    curl -sL "$URL" -o "$TMPDIR/$FILENAME"

    echo -e "${YELLOW}Extracting...${NC}"
    tar -xzf "$TMPDIR/$FILENAME" -C "$TMPDIR"

    if [ -f "$TMPDIR/livekit-server" ]; then
        mv "$TMPDIR/livekit-server" "$BINARY"
    else
        echo -e "${RED}Could not find livekit-server binary in archive${NC}"
        ls -la "$TMPDIR/"
        rm -rf "$TMPDIR"
        exit 1
    fi

    chmod +x "$BINARY"
    rm -rf "$TMPDIR"

    VERSION=$("$BINARY" --version 2>/dev/null || echo "unknown")
    echo -e "${GREEN}LiveKit server installed successfully${NC}"
    echo -e "   Version: $VERSION"
    echo -e "   Path: $BINARY"
fi

echo -e ""
echo -e "   Dev mode: livekit-server --dev --bind 127.0.0.1"
echo -e "   Default ports: 7880 (signaling), 7881 (ICE TCP), 50000-60000 (UDP media)"
echo -e "   Dev credentials: API key=devkey, API secret=secret"
