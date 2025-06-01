#!/bin/bash
set -e

echo "🔧 Cloning and building llama.cpp..."
mkdir -p bin && cd bin
git clone https://github.com/ggerganov/llama.cpp || true
cd llama.cpp
make
cd ../../

echo "📥 Downloading TinyLlama model..."
mkdir -p models && cd models
curl -L -o tinyllama.Q4_K_M.gguf https://huggingface.co/TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF/resolve/main/tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf
cd ..

echo "✅ Installation complete. You can now run ./launch-tmux.sh"
