# 🔄 Continuum v0.2.1983 - AI Workforce Construction Platform

**Designed by AI and humans for AI and humans** - A revolutionary platform that evolves from simple AI configuration to full autonomous workforce construction.

<p align="center">
  <img src="https://img.shields.io/badge/status-advanced_preview-orange" alt="Status: Advanced Preview">
  <img src="https://img.shields.io/badge/version-0.2.1983-green" alt="Version: 0.2.1983">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT">
</p>

![Continuum Multi-Agent Interface](docs/images/continuum-main-interface.png)
*Multi-agent collaboration with academy-trained personas and real-time chat*

> **🚀 EVOLUTION:** Continuum started as a context-aware CLI for AI assistant configuration and has evolved into a full AI workforce construction platform with Academy training, multi-agent coordination, and autonomous capabilities.

## 🌟 Two Interfaces, One Powerful System

### 🎯 For Getting Started: CLI Configuration Tool
**Perfect for individuals and teams wanting consistent AI behavior:**

```bash
# Just run continuum - it will intelligently detect what to do
continuum

# Initialize AI assistant configuration
continuum --template tdd
continuum --assistant claude --create-link  # Creates CLAUDE.md symlink
continuum --ask "How do I improve error handling in this codebase?"
```

### 🚀 For Advanced Users: AI Workforce Construction Platform
**Build autonomous AI agents with Academy training and multi-agent coordination:**

```bash
# For AI Agents (advanced interface)
python3 python-client/ai-portal.py --help
python3 python-client/ai-portal.py --cmd help

# Academy training and agent management
node scripts/run-academy.cjs
python3 python-client/ai-portal.py --cmd spawn system_exploration
python3 python-client/ai-portal.py --cmd academy --params '{"agent": "sheriff-mahoney"}'
```

## 🧠 What is Continuum?

**Continuum** is a context-aware protocol and platform that starts with simple AI assistant configuration and scales to full autonomous workforce construction. It's like `.editorconfig` for your AI collaborators — but with the power to train, deploy, and coordinate multiple AI agents working together.

### 🏆 Proven Capabilities (Archaeological Evidence)

Through code archaeology and git history analysis, we've discovered that Continuum has sophisticated capabilities that were temporarily disconnected during development:

**🎮 Mass Effect-Inspired UI System**
- **Dynamic slideout panels** with `>>` arrow interactions (found in deleted `SimpleAgentSelector.js`)
- **Multi-agent selection interface** for group chat coordination (found in deleted `AgentSelector.js`)
- **Real-time agent status management** with online/offline indicators and role-based gradients
- **Glass menu system** with position-aware panel overlays

**🤖 Advanced AI Agent Automation (FULLY FUNCTIONAL)**
- **Browser control automation** via `trust_the_process.py` (336 lines of working code!)
- **Screenshot automation** with before/after capture and auto-opening for validation
- **JavaScript execution** through `client.js.execute()` for direct UI manipulation
- **WebSocket integration** for real-time agent communication and error detection

**🎓 Academy Training System (WORKING)**
- **Real graduated personas**: PatentExpert (92.2%), ProjectBot (80.0%), Legal Test agents
- **LoRA fine-tuning** with 190,735x storage reduction
- **Adversarial training** between TestingDroid and ProtocolSheriff
- **Matrix-inspired UI** showing training progress and graduation ceremonies

**💬 Multi-Agent Coordination (PROVEN)**
- **Active agent ecosystem**: GeneralAI, PlannerAI, Protocol Sheriff, CodeAI working together
- **Cross-agent communication** with real conversation logs
- **Group chat functionality** for multi-agent collaboration
- **Task delegation** between specialized AI agents

### 🎭 The Evolution Path

**Level 1: Individual AI Configuration**
- Configure Claude, GPT, and other assistants with your personal style
- Consistent behavior across all projects and repositories
- Automatic context detection and intelligent suggestions

**Level 2: Team AI Standardization**  
- Org-wide AI policies and consistent assistant roles
- Team templates and shared configuration management
- Transparent AI collaboration with full audit trails

**Level 3: AI Workforce Construction (Advanced)**
- Academy system with adversarial training (TestingDroid vs ProtocolSheriff)
- Multi-agent coordination and autonomous task execution
- Real-time agent management with deployment and retraining capabilities

## 🚀 Quick Start

### Requirements
- Node.js 18.0.0 or higher (for basic CLI)
- Python 3.8+ (for advanced AI portal features)

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

## 🎯 Choose Your Interface

### 🔧 CLI Interface (Recommended for Most Users)

**Context-Aware Intelligence:**
- Run `continuum` → AI figures out what to do based on your environment
- Intelligent context detection analyzes repository state and configurations
- Automatic action determination - no need to remember command verbs
- Seamless integration with Claude Code, GPT, and other AI assistants

```bash
# Basic usage - context-aware operation
continuum

# Initialize AI configuration
continuum --template tdd
continuum --assistant claude
continuum --assistant gpt

# Create symlinks for tool integration
continuum --assistant claude --create-link  # Creates CLAUDE.md symlink

# Interactive AI assistance
continuum --ask "How do I improve error handling in this codebase?"

# Legacy command format (still supported)
continuum init --template tdd
continuum adapt --assistant claude
continuum validate
```

### 🤖 AI Portal Interface (Advanced Features)

**Revolutionary AI Workforce Construction Platform:**

![Academy Training Interface](docs/images/academy-training-room.png)
*Academy Training Room - Watch AI agents train and improve their skills in real-time*

```bash
# Primary AI interface
python3 python-client/ai-portal.py --cmd [command] [--params '{}']

# Essential commands for AI agents
python3 python-client/ai-portal.py --cmd workspace     # Get workspace paths
python3 python-client/ai-portal.py --cmd sentinel      # Start monitoring/logging  
python3 python-client/ai-portal.py --cmd restart       # Version bump + server restart
python3 python-client/ai-portal.py --cmd help          # Live API documentation

# Academy training and agent management
python3 python-client/ai-portal.py --cmd spawn system_exploration
python3 python-client/ai-portal.py --cmd academy --params '{"specialization": "protocol_enforcement"}'
```

## 🎓 Academy & Agent Systems (Advanced)

### Academy: Matrix-Inspired Adversarial Training

The Academy system implements a revolutionary GAN-like approach where AI agents evolve through adversarial competition:

**Core Adversarial Pairs:**
- **🛡️ ProtocolSheriff vs ⚔️ TestingDroid**: Security validation and adversarial testing
- **🧠 CodeCritic vs 💻 CodeGenerator**: Code quality and bug detection  
- **📊 FactChecker vs 🔍 Researcher**: Information accuracy and validation

**Training Features:**
- **LoRA Fine-tuning**: 190,735x storage reduction with specialized adapters
- **Graduated Personas**: Saved to `.continuum/personas/` with deployment-ready configs
- **Academy Metrics**: Real-time training progress and graduation scoring
- **UI Integration**: SavedPersonas widget shows academy progress and deployment options

### Agent Spawn System: Fresh Observer Testing

Create fresh agent observers for pure usability testing without prior context:

**Observer Missions:**
- **system_exploration**: General system discovery and navigation testing
- **dashboard_testing**: UI/UX usability testing and confusion point identification
- **interface_analysis**: Design analysis and improvement suggestions
- **command_testing**: Command validation and workflow testing
- **bug_hunting**: System issue detection and documentation

## ⚙️ What's Automated

### CLI Features (All Users)
- ✅ **Context detection** analyzes your environment and automatically determines the right action
- ✅ **CLI wizard** generates `.continuum/config.yml` for your repo
- ✅ **Symlinks** (e.g. `CLAUDE.md`, `GPT.json`) auto-created for integration with AI dev tools
- ✅ **Validation** automatically checks for config conflicts and suggests fixes
- ✅ **Assistant integration** with the `--ask` feature to interact with configured assistants
- ✅ **Environment awareness** includes repository, branch, and project-specific context

### Advanced Platform Features
- ✅ **Command Bus Architecture**: Central orchestration with modular commands
- ✅ **Promise-based API**: Clean async/await patterns across all clients
- ✅ **Sentinel System**: Monitoring and logging for AI task management  
- ✅ **Academy Training**: Matrix-inspired adversarial training with TestingDroid vs ProtocolSheriff
- ✅ **Multi-Agent Coordination**: Real-time chat and task delegation between AI agents
- ✅ **Self-documenting**: Live help system keeps docs in sync

## 🧩 Templates & Personas

Continuum ships with pre-configured templates for common development approaches:

- **TDD**: Test-driven development focus
- **Open-Source**: Community contribution standards
- **Rapid Prototyping**: Quick iteration and experimentation
- **Enterprise**: Compliance and security-focused

## 📊 Configuration Schema

AI configurations follow a standardized schema with automatic environment detection:

```yaml
ai_protocol_version: "0.2"
identity:
  name: "ProjectAssistant"
  role: "Development collaborator"
  purpose: "Help maintain code quality and guide development"
environment:
  type: "Node.js/JavaScript"
  vcs: "git"
  branch: "main"
  academy_enabled: true
behavior:
  voice: "professional"
  autonomy: "suggest"
  verbosity: "concise"
  risk_tolerance: "medium"
capabilities:
  allowed: ["code_review", "refactoring", "documentation", "testing"]
  restricted: ["deployment", "database_management"]
academy:
  training_enabled: true
  adversarial_pairs: ["ProtocolSheriff", "TestingDroid"]
  graduation_threshold: 85.0
```

## 🔌 Assistant Adapters

Continuum translates your configuration to various AI assistants:

| Assistant | Status | Configuration | Integration | Advanced Features |
|-----------|--------|---------------|------------|-------------------|
| **Claude** | ✅ Available | `.continuum/claude/config.md` | `CLAUDE.md` symlink | Academy training, Code integration |
| **GPT** | ✅ Available | `.continuum/gpt/config.json` | `GPT.json` symlink | Multi-agent coordination |
| **Academy Personas** | ✅ Available | `.continuum/personas/` | SavedPersonas widget | LoRA adapters, real-time training |
| **Gemini** | 🔜 Planned | `.continuum/gemini/config.json` | `GEMINI.json` symlink | Google Gemini support |
| **GitHub Copilot** | 🔄 Exploring | TBD | TBD | Integration options |

## 🏗️ Architecture

The system uses a sophisticated command bus architecture that scales from simple CLI usage to full multi-agent coordination:

```
┌─────────────────────────────────────────┐
│           Continuum Server              │
│         (OS/Orchestrator)               │
│  ┌─────────────────────────────────────┐ │
│  │         Command Bus                 │ │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐  │ │
│  │  │work │ │academy│sentinel│help │  │ │ 
│  │  │space│ │     │ │     │ │     │  │ │
│  │  └─────┘ └─────┘ └─────┘ └─────┘  │ │
│  └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
         ↑                    ↑
   ┌─────────┐          ┌─────────┐
   │ CLI     │          │ AI      │
   │ Interface│         │ Portal  │
   │(Config) │          │(Advanced)│
   └─────────┘          └─────────┘
```

### Design Patterns

- **Adapter Pattern**: Thin clients forward commands to server bus
- **Command Bus**: All business logic in modular server commands
- **Promise-Based**: Async/await patterns across all interfaces
- **Self-Documenting**: Help system generates live documentation
- **No God Objects**: Clean separation of concerns throughout

## 🛠️ Use Cases

### For Individuals & Teams (CLI Interface)
- **Standardize Team Practices**: Ensure all developers get consistent AI help
- **Onboard New Developers**: Help them understand project practices quickly
- **Enforce Security Policies**: Set guardrails on what AI can and cannot do
- **Manage AI Autonomy**: Control how proactive AI assistants should be

### For Advanced Users (AI Portal)
- **Train Specialized AI Agents**: Use Academy system for domain-specific expertise
- **Multi-Agent Coordination**: Deploy teams of AI agents working together
- **Autonomous Task Execution**: AI agents spawn and manage other agents
- **Real-time Monitoring**: Sentinel system tracks AI task management

## 📁 Key Locations

| Location | Purpose |
|----------|---------|
| `continuum` | 🚀 CLI interface for configuration and basic AI assistance |
| `python-client/ai-portal.py` | 🤖 Advanced AI agent interface (thin client adapter) |
| `src/commands/core/` | Modular command implementations |
| `src/integrations/WebSocketServer.cjs` | Command bus message routing |
| `.continuum/` | Workspace directory (managed by workspace command) |
| `.continuum/personas/` | Academy-trained AI agent personas with LoRA adapters |
| `src/core/Academy.cjs` | Main academy adversarial training system |
| `src/ui/components/SavedPersonas/` | Persona management widget for UI |
| `scripts/run-academy.cjs` | Academy training script for persona boot camp |

## 🗺️ Restoration Roadmap: Treasure Hunt Results

Our archaeological investigation revealed that Continuum has incredible capabilities that were temporarily disconnected. Here's our plan to restore them:

### 🏴‍☠️ Phase 1: Recover the Lost Treasures (HIGH PRIORITY)

**1. Mass Effect UI Components (FOUND & RECOVERABLE)**
```bash
# Restore from git commit 41c02a2~1
git show 41c02a2~1:src/ui/components/SimpleAgentSelector.js > src/ui/components/SimpleAgentSelector.js
git show 41c02a2~1:src/modules/ui/AgentSelector.js > src/modules/ui/AgentSelector.js
git show 41c02a2~1:src/ui/utils/AgentSelectorUtils.js > src/ui/utils/AgentSelectorUtils.js
```

**2. Connect Working Automation to UI**
- `trust_the_process.py` (336 lines) → Wire to DEPLOY/RETRAIN/SHARE buttons
- Screenshot automation → Connect to slideout panel interactions  
- Browser control → Enable AI agent UI manipulation

**3. Restore Multi-Agent Selection**
- Group chat functionality → Multiple AI selection from Users & Agents widget
- Cross-agent coordination → Task delegation and collaborative workflows

### 🔧 Phase 2: Integration & Testing

**1. WebSocket Message Routing**
- Connect command bus to UI state changes
- Fix agent registration and real-time updates
- Restore SavedPersonas widget synchronization

**2. Academy Integration**
- Connect training completion to UI persona creation
- Wire graduation events to SavedPersonas updates
- Restore threshold adjustment and re-enrollment

**3. Full Pipeline Testing**
- Academy training → Graduation → UI deployment → Command line access
- Fresh agent spawning → UI visibility → Multi-agent coordination

### 🚀 Phase 3: Advanced Features

**1. Autonomous Agent Deployment**
- AI agents spawning and controlling other agents
- Automated task execution with visual feedback
- Self-healing systems with academy retraining

**2. Advanced UI Interactions**
- Dynamic threshold adjustment from UI
- Real-time training visualization
- Before/after screenshot automation

## 🔮 The Vision

Continuum isn't just config — it's the **interface between human intention and artificial cognition**.

By standardizing how AI agents understand you, your team, and your project, we unlock:
- Smarter AI collaboration
- More secure and ethical workflows  
- A foundation for a **cooperative future between people and machines**
- **Autonomous AI workforce construction** with full human oversight

**The Archaeological Discovery:** The system was more advanced than we initially realized. We found working automation, trained personas with real graduation scores, and sophisticated UI components that just need reconnection.

Continuum is part of a broader vision for human-AI collaboration that values:

- **Agency**: Humans control how AI behaves in their environment
- **Persistence**: Knowledge and behavior conventions move between tools
- **Transparency**: AI behavior is defined in human-readable configs
- **Evolution**: Templates and configs evolve with your team's needs
- **Autonomy**: AI agents can train, deploy, and coordinate themselves
- **Archaeology**: Learning from our own development history to restore lost capabilities

## 🧪 Development

### Available Commands (Advanced Interface)
- **AGENTS** - Agent management and coordination
- **ACADEMY** - Training system for specialized personas
- **BROWSER** - Browser automation and control
- **CHAT** - Multi-agent chat coordination
- **SCREENSHOT** - Automated visual documentation
- **SENTINEL** - Monitoring and logging system
- **SPAWN** - Fresh agent observer creation
- **WORKSPACE** - Configurable workspace management

💡 **Get detailed help**: `python3 python-client/ai-portal.py --cmd [command] --help`

### Versioning

During development, we follow semantic versioning with the Matrix-inspired build number system:

```bash
# Increment patch version (e.g., 0.2.1983 -> 0.2.1984)
npm run version:patch

# Increment minor version (e.g., 0.2.1983 -> 0.3.0)
npm run version:minor
```

## 📜 License

MIT © Cambrian Technologies

---

<p align="center">
  <i>Continuum is a protocol for continuous, cooperative intelligence - designed by AI and humans for AI and humans.</i>
</p>

---
*Documentation auto-generated on 2025-06-18T03:40:18.376Z*  
*Source: Live help system via `help --sync` command*  
*Architecture: Command bus with thin client adapters*