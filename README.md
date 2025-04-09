# 🔄 Continuum v0.1.0

**Designed by AI and humans for AI and humans** - A protocol and CLI tool that configures AI assistants across all your projects, repos, and teams.

<p align="center">
  <img src="https://img.shields.io/badge/status-early_preview-orange" alt="Status: Early Preview">
  <img src="https://img.shields.io/badge/version-0.1.0-green" alt="Version: 0.1.0">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT">
</p>

<p align="center">
  <img src="assets/under_construction.gif" alt="Under Construction">
</p>

> **⚠️ UNDER CONSTRUCTION:** Continuum is currently in early development (v0.1). We've implemented the context-aware CLI foundation, but many planned features are still in progress. See our [Roadmap](ROADMAP.md) for development plans.

## 🧠 What is Continuum?

**Continuum** is a context-aware protocol and CLI tool that configures AI assistants across all your projects, repos, and teams — giving you **persistent, personalized, and policy-aware AI behavior**, no matter where or how you're working.

It's like `.editorconfig` or `tsconfig.json` — but for **your AI collaborators**.

### 🌟 Context-Aware Intelligence

Continuum's CLI is designed with a "zero-friction cognition" paradigm:

- **Run `continuum` → and the AI figures out what to do** based on your environment
- **Intelligent context detection** analyzes your repository state, configs, and integrations
- **Automatic action determination** so you don't need to remember command verbs
- **Personalized suggestions** based on your project's specific needs
- **Seamless integration** with Claude, GPT, and other AI assistants

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
# Just run continuum - it will intelligently detect what to do
continuum

# Initialize a new .continuum configuration
continuum --template tdd

# Generate assistant-specific configurations
continuum --assistant claude
continuum --assistant gpt

# Create symlinks for tool integration
continuum --assistant claude --create-link  # Creates CLAUDE.md symlink
continuum --assistant gpt --create-link     # Creates GPT.json symlink

# Ask your configured AI assistant a question
continuum --ask "How do I improve error handling in this codebase?"

# Legacy command format (still supported)
continuum init --template tdd
continuum adapt --assistant claude
continuum validate
```

Continuum is context-aware and will automatically detect what to do based on your repository's state. Just run `continuum` and it will figure out the right action!

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

- ✅ **Context detection** analyzes your environment and automatically determines the right action
- ✅ **CLI wizard** (`continuum` or `continuum init`) generates `.continuum/config.yml` for your repo
- ✅ **Symlinks** (e.g. `CLAUDE.md`, `GPT.json`) auto-created for integration with AI dev tools
- ✅ **Validation** automatically checks for config conflicts, missing agents, and suggests fixes
- ✅ **Adapters** convert unified config into Claude, GPT, and other AI-compatible formats
- ✅ **Assistant integration** with the `--ask` feature to interact with your configured assistants
- ✅ **Environment awareness** includes repository, branch, and project-specific context
- ✅ **Merging logic** (coming soon): blends personal + org + project preferences with conflict resolution

## 🧩 Templates & Personas

Continuum ships with pre-configured templates for common development approaches:

- **TDD**: Test-driven development focus
- **Open-Source**: Community contribution standards
- **Rapid Prototyping**: Quick iteration and experimentation
- **Enterprise**: Compliance and security-focused

## 📊 Configuration Schema

AI configurations in Continuum follow a standardized schema stored in `.continuum/default/config.md`. The system automatically detects and includes environment information:

```yaml
ai_protocol_version: "0.1"
identity:
  name: "ProjectAssistant"
  role: "Development collaborator"
  purpose: "Help maintain code quality and guide development"
environment:
  type: "Node.js/JavaScript"
  vcs: "git"
  branch: "main"
behavior:
  voice: "professional"
  autonomy: "suggest"
  verbosity: "concise"
  risk_tolerance: "medium"
capabilities:
  allowed: ["code_review", "refactoring", "documentation", "testing"]
  restricted: ["deployment", "database_management"]
```

Continuum automatically detects:
- Your development environment (Node.js, Python, etc.)
- Version control system and current branch
- Existing configurations and integrations

Continuum uses this information to generate context-aware assistant-specific files:
- `.continuum/claude/config.md` for Anthropic's Claude
- `.continuum/gpt/config.json` for OpenAI's GPT models
- Additional configurations for other assistants as they're added

## 🔌 Assistant Adapters

Continuum translates your configuration to various AI assistants:

### AI Assistant Compatibility

| Assistant | Status | Configuration | Integration | Notes |
|-----------|--------|---------------|------------|-------|
| **Claude** | ✅ Available | `.continuum/claude/config.md` | `CLAUDE.md` symlink | Fully supported with Claude Code integration |
| **GPT** | ✅ Available | `.continuum/gpt/config.json` | `GPT.json` symlink | Full support for ChatGPT and OpenAI models |
| **Gemini** | 🔜 Planned | `.continuum/gemini/config.json` | `GEMINI.json` symlink | Google Gemini support coming soon |
| **Llama** | 🔜 Planned | `.continuum/llama/config.md` | `LLAMA.md` symlink | Meta Llama support in development |
| **Mistral** | 🔜 Planned | `.continuum/mistral/config.json` | `MISTRAL.json` symlink | Mistral AI support planned |
| **GitHub Copilot** | 🔄 Exploring | TBD | TBD | Investigating integration options |

### Claude Code Integration

For Claude Code, you can add the following to your `.clauderc`:

```json
{
  "systemPromptFile": "CLAUDE.md"
}
```

When you run `continuum adapt --assistant claude --create-link`, the symlink will ensure Claude Code picks up your configuration.

### GPT Integration

For OpenAI's models, the generated configuration includes appropriate system prompts and parameters that can be used with the OpenAI API or CLI tools.

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