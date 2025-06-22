# Continuum Documentation Navigation

## 📚 Markdown Documentation Quick Reference

> **Navigation aid for Continuum's 155+ markdown documents**  
> **Complements**: [FILES.md](FILES.md) (comprehensive file tracking with emoji categorization)  
> **Purpose**: Quick access to documentation via portal/dashboard commands  
> **Last Updated**: 2025-06-22

## 🔗 Integration with Existing Systems

- **[FILES.md](FILES.md)** (288KB) - Complete file inventory with 🧹🌀🔥📦🎯 emoji categorization
- **Portal Commands** - `python ai-portal.py --docs`, `--cmd help <topic>`
- **Auto-Sync** - `--cmd docs --sync` updates FILES.md from current state

---

## 🎯 Documentation by Purpose

### 🚀 **Getting Started** (New Contributors)
| Document | Portal Access | Purpose |
|----------|---------------|---------|
| [README.md](README.md) | Default entry | Project overview, installation |
| [CLAUDE.md](CLAUDE.md) | `--cmd help claude` | **Master guide** - Development principles, AI onboarding |
| [docs/CONTINUUM_PROCESS.md](docs/CONTINUUM_PROCESS.md) | `--cmd help process` | Baby steps methodology, 8-step cycle |

### 🏗️ **Architecture & Design** (System Understanding)
| Document | Portal Access | Purpose |
|----------|---------------|---------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | `--cmd help architecture` | System architecture overview |
| [ROADMAP.md](ROADMAP.md) | `--cmd roadmap` | **47KB roadmap** - Phase-based development plan |
| [docs/PHASE_OMEGA.md](docs/PHASE_OMEGA.md) | `--cmd help omega` | Constitutional framework, Pattern of Care |
| [docs/ACADEMY_ARCHITECTURE.md](docs/ACADEMY_ARCHITECTURE.md) | `--cmd academy --help` | AI training, adversarial learning |

### 🛠️ **Development Process** (Day-to-Day Work)
| Document | Portal Access | Purpose |
|----------|---------------|---------|
| [process.md](process.md) | `--cmd tests` | Development process documentation |
| [docs/AGENT_DEVELOPMENT_GUIDE.md](docs/AGENT_DEVELOPMENT_GUIDE.md) | `--cmd help agents` | AI agent workflows, best practices |
| [RESTORATION-STRATEGY.md](RESTORATION-STRATEGY.md) | `--cmd restore` | Git recovery for lost Mass Effect UI |
| [DEVTOOLS_INTEGRATION_PLAN.md](DEVTOOLS_INTEGRATION_PLAN.md) | `--cmd devtools` | Browser automation, screenshot integration |

### 🎯 **Portal & Commands** (Daily Operations)
| Document | Portal Access | Purpose |
|----------|---------------|---------|
| [python-client/README.md](python-client/README.md) | `python ai-portal.py --help` | Python portal architecture |
| [docs/AI_PORTAL_ARCHITECTURE.md](docs/AI_PORTAL_ARCHITECTURE.md) | Portal: `--help` | Portal system design |
| [src/commands/*/README.md](src/commands/README.md) | `--cmd help <command>` | **30+ command docs** - Self-documenting modules |

### 📊 **Status & Tracking** (System Health)
| Document | Portal Access | Purpose |
|----------|---------------|---------|
| [FILES.md](FILES.md) | `--cmd docs --sync` | **288KB** - Complete file tracking with emoji issues |
| [INTEGRATION_STATUS.md](INTEGRATION_STATUS.md) | `--dashboard` | Current system connectivity status |
| [CHECKIN_SUMMARY.md](CHECKIN_SUMMARY.md) | Portal: `--broken` | Development progress, milestone updates |

---

## 🧭 **Quick Portal Commands**

### Essential Dashboard Commands
```bash
# System Overview
python ai-portal.py --dashboard      # Main dashboard with doc links
python ai-portal.py --broken         # Issues needing attention
python ai-portal.py --roadmap        # Development roadmap view

# Documentation Management  
python ai-portal.py --cmd docs --sync    # Update FILES.md tracking
python ai-portal.py --cmd help <topic>   # Rich help from READMEs

# DevTools & Screenshots
python ai-portal.py --devtools       # DevTools system integration  
python ai-portal.py --cmd screenshot # Screenshot capture system
```

## 📋 **Command Documentation (Self-Documenting Modules)**

> **Every command has a README with `--cmd help <command>` integration**

### Core Commands
- `help` - Dynamic help system with filtering and verbosity
- `info` - System information and status  
- `restart` - Full restart with version bump
- `workspace` - Workspace management

### Browser & Automation  
- `browser` - Browser control and automation
- `browserjs` - JavaScript execution in browser
- `screenshot` - Screenshot capture and routing

### Development & CI
- `ci` - Continuous integration workflows
- `issues` - GitHub issue management  
- `validatecode` - Code validation and linting
- `validatejs` - JavaScript validation

### Input & UI
- `cursor` - Cursor movement and positioning
- `type` - Text typing simulation  
- `emotion` - Continuon emotion system
- `clear` - Clear input and reset state

### Monitoring & Analysis
- `agents` - Agent monitoring and status
- `diagnostics` - System diagnostics  
- `sentinel` - Sentinel AI monitoring
- `analyze` - System analysis and insights

---

## 📂 **Key Documentation Areas**

### 🏗️ **Architecture & Core Docs** 
- **[docs/](docs/)** - Main documentation directory (12 files)
  - `PHASE_OMEGA.md` - Constitutional framework, Pattern of Care
  - `CONTINUUM_PROCESS.md` - Baby steps methodology, 8-step cycle  
  - `ACADEMY_ARCHITECTURE.md` - AI training, adversarial learning
  - `AGENT_DEVELOPMENT_GUIDE.md` - AI workflows, best practices

### 📁 **Archive & Reference**
- **[archive/docs/](archive/docs/)** - Legacy documentation (13 files)
- **[.continuum/shared/](/.continuum/shared/)** - Agent communication logs
- **[.continuum/shared/aria-conversations/](/.continuum/shared/aria-conversations/)** - Conversation archives

### 🧪 **Testing & Development**
- **[__tests__/](/__tests__/)** - Test documentation and strategies
- **[python-client/](python-client/)** - Python portal documentation (7 files)
- **[verification/](verification/)** - Git hook verification system

---

## 🔗 **Integration with FILES.md System**

This navigation index **complements** the comprehensive [FILES.md](FILES.md) system:

- **FILES.md** (288KB): Complete file inventory with 🧹🌀🔥📦🎯 emoji issue tracking
- **DOCUMENTATION_INDEX.md** (this file): Quick navigation focused on documentation
- **Portal Integration**: Both sync via `--cmd docs --sync`

### Emoji Categories in FILES.md:
- 🧹 **Cleanup needed** - Files requiring attention
- 🌀 **Suspicious code** - Needs investigation  
- 🔥 **Test failures** - Broken tests to fix
- 📦 **Architecture** - Refactoring needed
- 🎯 **Enhancement** - Feature improvements

---

## 🚀 **Quick Start for New Contributors**

1. **Read**: [CLAUDE.md](CLAUDE.md) - Master development guidelines
2. **Process**: [docs/CONTINUUM_PROCESS.md](docs/CONTINUUM_PROCESS.md) - Baby steps methodology
3. **Portal**: `python ai-portal.py --dashboard` - System overview
4. **Commands**: `--cmd help <topic>` - Rich help for any command
5. **Tracking**: [FILES.md](FILES.md) - Complete file inventory and issues

---

**This documentation navigation system integrates with Continuum's portal commands and maintains live sync with the FILES.md tracking system from the archaeological dig recovery process.**

*Navigation Index | Complements FILES.md | Portal Integrated | Auto-Sync Ready*