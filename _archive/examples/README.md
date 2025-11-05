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
npx @continuum/cli validate .continuum/default/config.md
```

### Adapting for Different Assistants

```bash
# For Claude
npx @continuum/cli adapt --assistant claude --input .continuum/default/config.md --output .continuum/claude/config.md

# For ChatGPT
npx @continuum/cli adapt --assistant gpt --input .continuum/default/config.md --output .continuum/gpt/config.json
```

## Example Files

- `.continuum/default/config.md` - Standard configuration file
- `.continuum/claude/config.md` - Claude-specific configuration 
- `.continuum/gpt/config.json` - GPT-specific configuration
- `visualize-config.js` - Utility to visualize a configuration

## Visualizing a Configuration

```bash
node visualize-config.js .continuum/default/config.md
```

## Using with GitHub

You can store your AI configuration in your repository:

```
.github/
  .continuum/
    default/     # Standard configuration
      config.md
    claude/      # Claude-specific instructions
      config.md
    gpt/         # GPT-specific instructions
      config.json
```

This allows all team members to work with a consistent AI assistant configuration.