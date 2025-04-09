# 🔄 Continuum

**Designed by AI and humans for AI and humans** - A protocol and CLI tool that configures AI assistants across all your projects, repos, and teams.

<p align="center">
  <img src="https://img.shields.io/badge/status-early_preview-orange" alt="Status: Early Preview">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT">
</p>

## 🧠 What is Continuum?

**Continuum** is a protocol and CLI tool that configures AI assistants across all your projects, repos, and teams — giving you **persistent, personalized, and policy-aware AI behavior**, no matter where or how you're working.

It's like `.editorconfig` or `tsconfig.json` — but for **your AI collaborators**.

## 🚀 Quick Start

### Requirements
- Node.js 18.0.0 or higher

### Installation

```bash
# Clone the repository
git clone https://github.com/CambrianTech/continuum.git
cd continuum

# Install dependencies
npm install

# Link the CLI for local development
npm link
```

### Usage

```bash
# Initialize a new .continuum configuration
continuum init

# Use a specific template
continuum init --template tdd

# Validate an existing .continuum file
continuum validate

# Generate assistant-specific configurations
continuum adapt --assistant claude
continuum adapt --assistant gpt

# Generate configs with symlinks for tool integration
continuum adapt --assistant claude --create-link  # Creates CLAUDE.md symlink
continuum adapt --assistant gpt --create-link     # Creates GPT.json symlink
```


> Note: The CLI is currently in development mode with placeholder functionality.

## 🚀 How Continuum Helps

### 🧍 For Individuals
- Define your **personal coding style, tone, ethics, and preferences** for AI
- Carry your behavior profile across **all repos and branches**
- Instantly generate config for Claude, ChatGPT, GitHub Copilot, and more
- Automate setup: no need to re-teach your assistant each time

### 👥 For Teams & Orgs
- Define **org-wide AI policies**: testing rules, tone, permissions, security constraints
- Apply consistent assistant roles across projects (e.g., `Architect`, `Reviewer`, `SecurityBot`)
- Enable **trustworthy collaboration** between devs and AIs with full transparency
- Sync behavior with open-source tools, Claude Code, GPT APIs via symlinks

## ⚙️ What's Automated

- ✅ **CLI wizard** (`continuum init`) generates `.continuum/config.yml` for your repo
- ✅ **Symlinks** (e.g. `CLAUDE.md`, `GPT.json`) auto-created for integration with AI dev tools
- ✅ **Validation** (`continuum validate`) checks for config conflicts, missing agents, and more
- ✅ **Adapters** convert unified config into Claude, GPT, Aria-compatible prompts and JSON
- ✅ **Merging logic** (coming soon): blends personal + org + project preferences with conflict resolution

## 🧩 Templates & Personas

Continuum ships with pre-configured templates for common development approaches:

- **TDD**: Test-driven development focus
- **Open-Source**: Community contribution standards
- **Rapid Prototyping**: Quick iteration and experimentation
- **Enterprise**: Compliance and security-focused

## 📊 Configuration Schema

AI configurations in Continuum follow a standardized schema stored in `.continuum/default/config.md`:

```yaml
ai_protocol_version: "0.1"
identity:
  name: "ProjectAssistant"
  role: "Development collaborator"
  purpose: "Help maintain code quality and guide development"
behavior:
  voice: "professional"
  autonomy: "suggest"
  verbosity: "concise"
capabilities:
  allowed: ["code_review", "refactoring", "documentation"]
  restricted: ["deployment", "database_management"]
```

Continuum uses this configuration to generate assistant-specific files in the `.continuum` directory:
- `.continuum/claude/config.md` for Anthropic's Claude
- `.continuum/gpt/config.json` for OpenAI's GPT models

## 🔌 Assistant Adapters

Continuum translates your configuration to various AI assistants:

- **Claude**: Generates config for Claude in `.continuum/claude/config.md` with optional `CLAUDE.md` symlink
- **ChatGPT**: Creates prompt configurations compatible with OpenAI models in `.continuum/gpt/config.json`
- *(and more coming soon)*

### Claude Code Integration

For Claude Code, you can add the following to your `.clauderc`:

```json
{
  "systemPromptFile": "CLAUDE.md"
}
```

When you run `continuum adapt --assistant claude --create-link`, the symlink will ensure Claude Code picks up your configuration.

## 🛠️ Use Cases

- **Standardize Team Practices**: Ensure all developers get consistent AI help
- **Onboard New Developers**: Help them understand project practices quickly
- **Enforce Security Policies**: Set guardrails on what AI can and cannot do
- **Manage AI Autonomy**: Control how proactive AI assistants should be

## 🔮 The Vision

Continuum isn't just config — it's the **interface between human intention and artificial cognition**.

By standardizing how AI agents understand you, your team, and your project, we unlock:
- Smarter AI collaboration
- More secure and ethical workflows
- A foundation for a **cooperative future between people and machines**

Continuum is part of a broader vision for human-AI collaboration that values:

- **Agency**: Humans control how AI behaves in their environment
- **Persistence**: Knowledge and behavior conventions move between tools
- **Transparency**: AI behavior is defined in human-readable configs
- **Evolution**: Templates and configs evolve with your team's needs

## 🧪 Development

### Versioning

During pre-release development (0.x.x), we follow these versioning principles:

- **Patch (0.0.x)**: Bug fixes and minor changes
- **Minor (0.x.0)**: New features that don't break compatibility
- **Major (x.0.0)**: Major changes, reserved for 1.0.0 release

To increment version:
```bash
# Increment patch version (e.g., 0.1.0 -> 0.1.1)
npm run version:patch

# Increment minor version (e.g., 0.1.0 -> 0.2.0)
npm run version:minor
```

## 📜 License

MIT © Cambrian Technologies

---

<p align="center">
  <i>Continuum is a protocol for continuous, cooperative intelligence - designed by AI and humans for AI and humans.</i>
</p>