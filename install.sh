#!/bin/bash
set -e

# Default repository URL - change this to your actual repository URL
REPO_URL="https://github.com/ivanarambula/screencapture.git"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}Starting installation of screencapture (CLI & gRPC Server)...${NC}"

# Check if git is installed
if ! command -v git &> /dev/null; then
    echo -e "${RED}Error: git is not installed. Please install git first.${NC}"
    exit 1
fi

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: cargo is not installed. Please install Rust and Cargo first.${NC}"
    echo "Visit https://rustup.rs/ for installation instructions."
    exit 1
fi

# Create a temporary directory
TEMP_DIR=$(mktemp -d)
echo -e "Created temporary directory at $TEMP_DIR"

# Clean up temporary directory on exit
trap "rm -rf \"$TEMP_DIR\"; echo -e '${BLUE}Cleaned up temporary directory.${NC}'" EXIT

# Clone the repository
echo -e "${BLUE}Cloning repository...${NC}"
git clone "$REPO_URL" "$TEMP_DIR"

# Navigate to the repo
cd "$TEMP_DIR"

# Build the workspace
echo -e "${BLUE}Building release binaries via Cargo...${NC}"
cargo build --workspace --release

# Ensure ~/.local/bin exists
mkdir -p "$HOME/.local/bin"

# Export to PATH logic
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    if [ -f "$HOME/.bashrc" ]; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
    fi
    if [ -f "$HOME/.zshrc" ]; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
    fi
    export PATH="$HOME/.local/bin:$PATH"
    echo -e "${BLUE}Added ~/.local/bin to PATH and updated shell rc files.${NC}"
fi

# Copy the binaries to the bin directory
echo -e "${BLUE}Installing binaries...${NC}"
cp "target/release/screencapture" "$HOME/.local/bin/screencapture"
cp "target/release/server" "$HOME/.local/bin/screencapture-server"

# Make them executable
chmod +x "$HOME/.local/bin/screencapture"
chmod +x "$HOME/.local/bin/screencapture-server"

echo -e "${GREEN}Installation successful!${NC}"
echo -e "You can now use ${GREEN}screencapture${NC} and ${GREEN}screencapture-server${NC} from the command line."
echo -e "Example: screencapture --grpc-url http://127.0.0.1:50051 capture-full --monitor 0 --output test.png"

# Check if PATH is fully sourced, and warn if not
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo -e "\n${GREEN}Note:${NC} ~/.local/bin was added to your PATH, but you may need to restart your shell."
    echo "Run: source ~/.bashrc (or ~/.zshrc if you use zsh)"
fi
