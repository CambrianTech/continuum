# JTAG Development Tools

This directory contains development and advanced testing tools for JTAG, organized by functional area. These are **not part of the core JTAG package** but are essential for developing and testing advanced features.

## 🏗️ **Modular Organization Principles**

- **Each module belongs where it makes sense**
- **Every module is independently testable**
- **Clear separation between core package and development tools**
- **Minimal dependencies between modules**

## 📁 **Directory Structure**

```
dev-tools/
├── grid-testing/           # Grid P2P backbone development tools
│   ├── multi-instance/     # Multi-instance Continuum coordination
│   ├── p2p-discovery/      # P2P mesh networking tools  
│   ├── benchmarks/         # Grid performance testing
│   └── tests/              # Grid-specific tests
├── internal-scripts/       # Internal development scripts
├── testing-frameworks/     # Advanced testing utilities
└── README.md              # This file
```

## 🌐 **Grid Testing Module**

The Grid testing tools enable development and validation of the **Grid P2P backbone** - the distributed mesh network that connects multiple Continuum instances with AI personas.

### **Multi-Instance Testing** (`grid-testing/multi-instance/`)
- **Purpose**: Test multiple Continuum instances in parallel for Grid P2P development
- **Module**: Standalone, testable Grid development tool  
- **Usage**: `npm run dev:grid-p2p` (development tool, not end-user feature)
- **Tests**: Located in `grid-testing/tests/multi-instance.test.ts`

**Key Components:**
- `MultiInstanceTestTypes.ts` - Strong typing for multi-instance configurations
- `MultiInstanceTestRunner.ts` - Orchestrates multiple Continuum instances
- `multi-instance-runner.ts` - CLI tool for parameterized testing
- `tests/` - Independent test suite for multi-instance functionality

### **Example Usage (Development Only)**
```bash
# Grid P2P backbone testing with 3 nodes
npm run dev:grid-p2p

# Custom multi-instance test with specific examples
npm run dev:multi-instance -- --instances=test-bench,widget-ui

# Load testing with 4 instances  
npm run dev:multi-instance -- --instances=test-bench,test-bench,widget-ui,test-bench --profile=load-testing
```

## 🧪 **Testing Philosophy**

Each development tool module follows the **testable modules** principle:

1. **Independent Tests**: Each module has its own test directory
2. **Minimal Dependencies**: Modules depend only on core JTAG, not each other
3. **Clear Interfaces**: Well-defined types and APIs
4. **Isolated Functionality**: Can be tested without other dev tools

## 📦 **NPM Package Organization**

**Development Tools vs Core Package:**
- **Core Package**: `src/`, `commands/`, `daemons/`, `system/` - shipped to end users
- **Development Tools**: `dev-tools/` - for JTAG/Grid development, not end users
- **User Examples**: `examples/` - shipped to end users as reference
- **Core Tests**: `tests/` - core package functionality tests only

## 🚀 **Adding New Development Tools**

When adding new development tools:

1. **Create proper module structure** in relevant subdirectory
2. **Write independent tests** in module's `tests/` directory  
3. **Minimize dependencies** - depend only on core JTAG
4. **Document module purpose** and usage
5. **Add npm scripts** prefixed with `dev:` for clarity

## 🔗 **Integration with Core JTAG**

Development tools:
- **Import from core JTAG**: Use standard imports like `import { SystemReadySignaler } from '../../scripts/signaling/server/SystemReadySignaler'`
- **Don't modify core**: Development tools extend and test core functionality, don't modify it
- **Stay modular**: Each tool is independent and testable

This organization ensures the core JTAG package stays clean while providing powerful development tools for advanced features like the Grid P2P backbone.