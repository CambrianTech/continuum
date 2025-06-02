# Continuum - AI Command & Control System

## 🚀 Quick Start

```bash
npm install
node continuum.cjs
# Open http://localhost:5555
```

## 📁 Project Structure

```
continuum/
├── continuum.cjs              # Main entry point
├── src/
│   ├── core/                  # Core system logic
│   ├── agents/                # AI agent types (GeneralAI, CodeAI)
│   ├── services/              # Typed services (ModelDiscovery)
│   ├── docs/                  # Protocol and command docs
│   ├── tests/                 # All test files
│   └── tools/                 # Command execution tools
├── packages/                  # TypeScript packages
├── .continuum/               # Runtime config and formulas
└── archive/                  # Old experiments and legacy code
```

## 🎯 Key Features

- **AI Command Protocol**: Uses `[CMD:ACTION]` format for intelligent command execution
- **Formula System**: Proven patterns in `.continuum/formulas/` for common tasks
- **Model Discovery**: Dynamic detection of available AI models
- **Typed Architecture**: Clean OOP with TypeScript services
- **Multi-Agent Support**: GeneralAI, CodeAI, PlannerAI coordination

## 🧪 Testing

```bash
npm test                    # Run all tests
node src/tests/run-all-tests.cjs   # Custom test runner
```

## 📖 Documentation

- [Commands](src/docs/COMMANDS.md) - AI command protocol
- [Formulas](.continuum/formulas/) - Task patterns
- [Architecture](docs/SYSTEM_ARCHITECTURE.md) - System design