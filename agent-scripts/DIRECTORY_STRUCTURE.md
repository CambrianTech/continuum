# Agent Scripts Directory Structure

## Current Organization

```
agent-scripts/
├── 📚 README.md                    # Main documentation and quick start
├── 🔧 requirements.txt             # Python dependencies  
├── 🚀 activate-env.sh              # Environment activation script
│
├── 🛠️ tools/                        # Core automation tools
│   ├── python/                     # Python-based tools
│   │   ├── js-send.py             # 🛰️ Main probe communication tool
│   │   ├── heal.py                # 🏥 Universal healing system
│   │   ├── health-monitor.py      # 📊 System health monitoring
│   │   ├── probe-safe.py          # 🛡️ Safe probe operations
│   │   └── setup.py               # ⚙️ Environment setup
│   └── javascript/                # 🌐 Future: JavaScript tools
│
├── 📁 examples/                     # Example scripts by category
│   ├── jokes/                     # 🎭 Fun demonstration scripts
│   │   ├── ai-joke.js             # AI therapy humor
│   │   ├── css-joke.js            # CSS relationship problems  
│   │   └── tooth-joke.js          # Dental root directory pun
│   ├── diagnostics/               # 🔍 System analysis tools
│   │   ├── console-probe.js       # Console state inspection
│   │   ├── error-capture.js       # Error/warning capture
│   │   ├── probe-test.js          # Basic connectivity testing
│   │   └── test-script.js         # General testing template
│   └── fixes/                     # 🔧 Browser/console fixes
│       ├── comprehensive-fix.js   # Multi-layered console cleanup
│       └── websocket-fix.js       # WebSocket stabilization
│
├── 🏃 bin/                          # Executable wrappers
│   ├── js-send                    # Auto-venv wrapper for js-send.py
│   ├── heal                       # Auto-venv wrapper for heal.py
│   ├── probe                      # Auto-venv wrapper for probe-safe.py
│   └── run-with-venv.py           # Virtual environment manager
│
└── 📖 docs/                         # Documentation
    ├── ARCHITECTURE.md            # System design and concepts
    ├── EXAMPLES.md                # Usage examples and templates
    └── CONTRIBUTING.md            # Guidelines for contributors
```

## Tool Categories

### 🛠️ Core Tools (`tools/python/`)
- **Production-ready automation tools**
- **Click-based CLI interfaces** 
- **Auto-healing capabilities**
- **Comprehensive error handling**

### 📁 Examples (`examples/`)
- **Learning materials and templates**
- **Organized by use case**
- **Safe for experimentation**
- **Well-documented with comments**

### 🏃 Executables (`bin/`)
- **Zero-setup wrappers**
- **Automatic virtual environment management**
- **Cross-platform compatibility**
- **Simple command names**

### 📖 Documentation (`docs/`)
- **Architecture explanations**
- **Usage guides and examples**
- **Contributing guidelines**
- **Best practices**

## Virtual Environment Structure

```
.continuum/venv/agents/             # Shared Python environment
├── bin/                           # Python executables
├── lib/python3.x/site-packages/   # Installed packages
└── pyvenv.cfg                     # Environment configuration
```

## Future Expansion Plans

### Planned Tool Categories
- **tools/javascript/** - Browser-side utilities and libraries
- **tools/bash/** - Shell scripting utilities  
- **tools/testing/** - Automated testing frameworks
- **tools/monitoring/** - Performance and health monitoring

### Planned Example Categories
- **examples/automation/** - UI automation and testing
- **examples/performance/** - Performance monitoring
- **examples/accessibility/** - A11y testing tools
- **examples/security/** - Security scanning
- **examples/development/** - Developer productivity

### Integration Points
- **Visual Studio Code extension** - IDE integration
- **GitHub Actions workflows** - CI/CD automation
- **Docker containers** - Containerized agent environments
- **REST API** - Programmatic access to agent tools

This structure supports both immediate productivity and long-term scalability of the agent automation ecosystem.