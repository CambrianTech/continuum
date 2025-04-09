# Continuum Examples

This directory contains examples of how to use Continuum, the AI Assistant Configuration Protocol.

## Basic Usage

### Initializing a New Configuration

```bash
# Interactive wizard
npx @continuum/cli init

# With a template
npx @continuum/cli init --template tdd
```

### Validating a Configuration

```bash
npx @continuum/cli validate AI_CONFIG.md
```

### Adapting for Different Assistants

```bash
# For Claude
npx @continuum/cli adapt --assistant claude --input AI_CONFIG.md --output CLAUDE.md

# For ChatGPT
npx @continuum/cli adapt --assistant gpt --input AI_CONFIG.md --output system_prompt.txt
```

## Example Files

- `AI_CONFIG.md` - Standard configuration file
- `claude/CLAUDE.md` - Claude-specific configuration
- `gpt/system_prompt.txt` - GPT-specific configuration
- `visualize-config.js` - Utility to visualize a configuration

## Visualizing a Configuration

```bash
node visualize-config.js AI_CONFIG.md
```

## Using with GitHub

You can store your AI configuration in your repository:

```
.github/
  AI_CONFIG.md   # Standard configuration
  CLAUDE.md      # Claude-specific instructions
```

This allows all team members to work with a consistent AI assistant configuration.