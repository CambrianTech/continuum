# Training Data Directory

## Overview

This directory contains training data sources for LoRA adapter fine-tuning and Sentinel plasticity.

## Contents

### `claude-sessions/` (symlink)
**Target**: `~/.claude/projects/-Volumes-FlashGordon-cambrian-continuum/`
**Size**: 2.2GB (82 conversation files)
**Format**: JSONL (JSON Lines)

Contains all Claude Code conversation logs for this project:
- Full development sessions (user messages + Claude responses)
- Code edits with before/after snapshots
- Tool usage patterns (Read, Edit, Bash, etc.)
- Debugging sessions and problem-solving
- Architectural discussions

**Why symlink?**
- Always has latest conversations (auto-updates)
- No duplication (2.2GB saved)
- Easy to reference from training pipeline

**Setup** (if symlink is broken):
```bash
ln -s ~/.claude/projects/-Volumes-FlashGordon-cambrian-continuum claude-sessions
```

### Future Files

- `sessions.jsonl` - Indexed sessions with git commit mappings
- `examples.jsonl` - Extracted training examples (filtered + quality-scored)
- `test-cases.json` - Held-out test cases for adapter validation
- `adapters/` - Trained LoRA adapters by domain
  - `testing-expert.lora`
  - `debugging-expert.lora`
  - `architecture-expert.lora`
  - `documentation-expert.lora`
  - `git-expert.lora`

## Training Pipeline

See: `../system/user/server/modules/TRAINING-DATA-PIPELINE.md` for complete documentation.

**Quick Start**:
```bash
# 1. Collect and index sessions
./jtag training/collect-sessions \
  --claude-logs=".continuum/training/claude-sessions/" \
  --git-repo="." \
  --output=".continuum/training/sessions.jsonl"

# 2. Extract training examples
./jtag training/extract-examples \
  --sessions=".continuum/training/sessions.jsonl" \
  --quality-threshold=0.7 \
  --output=".continuum/training/examples.jsonl"

# 3. Train domain-specific adapter
./jtag genome/train \
  --adapter="typescript-debugging" \
  --training-data=".continuum/training/examples.jsonl" \
  --filter='domain:debugging,language:typescript'
```

## Privacy & Gitignore

**Excluded from Git**:
- `claude-sessions/` (symlink itself is ignored)
- All generated training files (`*.jsonl`)
- Trained adapters (`adapters/*.lora`)

**Reason**: Contains full conversation history and could be large.

**Backup Strategy**: Original data is in `~/.claude/projects/` (backed up by Claude Code).

## Data Statistics

**Current** (Oct 1 - Nov 7, 2025):
- **82 conversation sessions** = 2.2GB
- **~500 git commits** in this timeframe
- **~10,000 file edits** across all sessions
- **~50,000 tool calls** (Read, Edit, Bash, etc.)

**Estimated Training Output**:
- **~20,000 training examples** after filtering
- **~5 specialized LoRA adapters** by domain

## Meta-Recursion

This directory is itself part of the training data. Future AIs will read this README and understand how to build training pipelines from their own development history.

**The loop**:
```
Claude sessions → Training data → LoRA adapters → Better AI devs → More sessions → Better training data
```

Recursion all the way down. 🔄
