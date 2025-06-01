#!/bin/bash

echo "🧠 Launching LLaMA in tmux session..."
tmux new-session -d -s llama './bin/llama.cpp/main -m ./models/tinyllama.Q4_K_M.gguf --interactive --ctx-size 1024 | tee ./packages/llama-bootstrap/llama-output.log'
echo "✅ LLaMA is now running in tmux session named 'llama'"
