# 🔄 Continuum

**DevOps for Cognitive Systems** - A protocol for configuring AI assistants to work consistently with your team, codebase, and philosophy.

<p align="center">
  <img src="https://img.shields.io/badge/status-early_preview-orange" alt="Status: Early Preview">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT">
</p>

## What is Continuum?

Continuum is the missing layer between human cognition and AI capability. It provides:

- **Standardized team memory**: A shared schema for how AI should behave across orgs, repos, and contributors
- **Embeddable intelligence**: AI configuration that lives in git, gets versioned, reviewed, and tested
- **Dynamic behavior tuning**: Templates for different development approaches (TDD, open-source, etc.)
- **Role-based permissions**: Security and permission controls for AI assistant actions

> Continuum is to AI assistants what `.editorconfig` and `tsconfig.json` are to your development tools.

## 🚀 Quick Start

### Requirements
- Node.js 18.0.0 or higher

```bash
# Install the CLI globally
npm install -g @continuum/cli

# Initialize a new AI configuration
continuum init

# Use a specific template
continuum init --template tdd

# Validate an existing configuration
continuum validate

# Generate assistant-specific configuration
continuum adapt --assistant claude
```

You can also use npx without installing:

```bash
npx @continuum/cli init
```

## 💡 Why Continuum Matters

AI assistants are becoming core team members, but:

- Their behavior is inconsistent across different users
- They lack knowledge of team conventions and workflows
- Different assistants (Claude, ChatGPT) need different instructions
- Teams want to control what the AI can and cannot do

Continuum solves these problems with a standardized configuration protocol.

## 🧩 Templates & Personas

Continuum ships with pre-configured templates for common development approaches:

- **TDD**: Test-driven development focus
- **Open-Source**: Community contribution standards
- **Rapid Prototyping**: Quick iteration and experimentation
- **Enterprise**: Compliance and security-focused

## 📊 Configuration Schema

AI configurations in Continuum follow a standardized schema:

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

## 🔌 Assistant Adapters

Continuum translates your configuration to various AI assistants:

- **Claude**: Generates `CLAUDE.md` with system instructions
- **ChatGPT**: Creates prompt configurations compatible with OpenAI models
- *(and more coming soon)*

## 🛠️ Use Cases

- **Standardize Team Practices**: Ensure all developers get consistent AI help
- **Onboard New Developers**: Help them understand project practices quickly
- **Enforce Security Policies**: Set guardrails on what AI can and cannot do
- **Manage AI Autonomy**: Control how proactive AI assistants should be

## 🧠 Philosophy

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
  <i>Continuum is a protocol for continuous, cooperative intelligence - building a bridge between human intention and AI capability.</i>
</p>