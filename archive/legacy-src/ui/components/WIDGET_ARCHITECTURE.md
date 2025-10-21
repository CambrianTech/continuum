# Widget Architecture - Middle-Out Design

## 🧅 Widget Layer Architecture

### Layer 1: Core Widget Foundation
```
src/ui/components/core/
├── base-widget/
│   ├── BaseWidget.ts          # Core widget functionality
│   ├── BaseWidget.css         # Universal widget styles
│   ├── package.json           # Core widget dependencies
│   └── test/
├── widget-system/
│   ├── WidgetSystem.ts        # Widget registration and management
│   ├── WidgetDiscovery.ts     # Dynamic widget discovery
│   ├── package.json
│   └── test/
└── data-display/
    ├── DataDisplayWidget.ts   # Generic data display patterns
    ├── package.json
    └── test/
```

### Layer 2: Domain Widget Types
```
src/ui/components/domain/
├── communication/
│   ├── base/
│   │   ├── CommunicationWidget.ts  # Base for chat, messaging
│   │   └── package.json
│   └── chat/
│       ├── ChatWidget.ts           # Specific chat implementation
│       ├── ChatWidget.css
│       └── package.json
├── intelligence/
│   ├── base/
│   │   ├── IntelligenceWidget.ts   # Base for AI-related widgets
│   │   └── package.json
│   ├── academy/
│   │   ├── AcademyWidget.ts        # AI training visualization
│   │   └── package.json
│   └── persona/
│       ├── PersonaWidget.ts        # AI persona display
│       └── package.json
├── navigation/
│   ├── base/
│   │   ├── NavigationWidget.ts     # Base navigation patterns
│   │   └── package.json
│   └── sidebar/
│       ├── SidebarWidget.ts        # Specific sidebar implementation
│       └── package.json
└── system/
    ├── base/
    │   ├── SystemWidget.ts         # Base for system widgets
    │   └── package.json
    ├── version/
    │   ├── VersionWidget.ts        # Version display
    │   └── package.json
    └── projects/
        ├── ActiveProjectsWidget.ts # Project management
        └── package.json
```

### Layer 3: Composite Widgets
```
src/ui/components/composite/
├── dashboard/
│   ├── DashboardWidget.ts     # Combines multiple widgets
│   └── package.json
├── workspace/
│   ├── WorkspaceWidget.ts     # Project-focused widget combinations
│   └── package.json
└── portal/
    ├── PortalWidget.ts        # Full portal interface
    └── package.json
```

## 🔧 Widget Dependency Rules

### Layer 1 → No Dependencies (Foundation)
- BaseWidget has no widget dependencies
- WidgetSystem only depends on BaseWidget
- DataDisplayWidget extends BaseWidget

### Layer 2 → Can Use Layer 1 Only
- CommunicationWidget extends BaseWidget
- ChatWidget extends CommunicationWidget  
- AcademyWidget extends IntelligenceWidget extends BaseWidget
- PersonaWidget extends IntelligenceWidget

### Layer 3 → Can Use Layers 1 & 2
- DashboardWidget composes multiple Layer 2 widgets
- WorkspaceWidget combines domain widgets for specific workflows
- PortalWidget is the full application interface

## 📦 Module Structure Pattern

### Every Widget Module Contains:
```
widget-name/
├── package.json           # Dependencies, main entry, metadata
├── WidgetName.ts         # Main widget implementation  
├── WidgetName.css        # Widget-specific styles (optional)
├── README.md            # Widget documentation and usage
├── test/
│   ├── unit/
│   │   └── WidgetName.test.ts
│   └── integration/
│       └── WidgetName.integration.test.ts
└── assets/              # Widget-specific resources (optional)
```

## 🎯 Widget Interface Contracts

### Layer 1: BaseWidget Interface
```typescript
interface BaseWidgetInterface {
  // Core lifecycle
  initialize(): Promise<void>;
  render(): void;
  destroy(): void;
  
  // Widget metadata
  widgetName: string;
  widgetIcon: string;
  widgetTitle: string;
  
  // Communication
  executeCommand(command: string, params: any): Promise<any>;
  log(message: string, level?: 'info' | 'warn' | 'error'): void;
}
```

### Layer 2: Domain Widget Interfaces
```typescript
interface CommunicationWidgetInterface extends BaseWidgetInterface {
  // Communication-specific methods
  sendMessage(message: any): Promise<void>;
  onMessageReceived(callback: (message: any) => void): void;
}

interface IntelligenceWidgetInterface extends BaseWidgetInterface {
  // AI-specific methods
  executeAICommand(command: string, params: any): Promise<any>;
  onAIStatusChange(callback: (status: any) => void): void;
}

interface NavigationWidgetInterface extends BaseWidgetInterface {
  // Navigation-specific methods
  navigateTo(route: string): void;
  onRouteChange(callback: (route: string) => void): void;
}
```

## 🔄 Widget Communication Patterns

### Command Delegation (All Layers)
```typescript
// All widgets can execute commands through BaseWidget
await this.executeCommand('academy-status', { detail_level: 'summary' });
await this.executeCommand('chat', { message: 'Hello', room: 'general' });
```

### Widget-to-Widget Communication (Layer 3 Only)
```typescript
// Composite widgets orchestrate domain widgets
class DashboardWidget extends BaseWidget {
  private academyWidget: AcademyWidget;
  private chatWidget: ChatWidget;
  
  private setupWidgetCommunication(): void {
    // Academy widget notifies chat when persona becomes available
    this.academyWidget.onPersonaSpawned((persona) => {
      this.chatWidget.addAvailablePersona(persona);
    });
    
    // Chat widget can trigger Academy training
    this.chatWidget.onTrainingRequest((persona, domain) => {
      this.academyWidget.startTraining(persona, domain);
    });
  }
}
```

### Event Bus Pattern (System-Wide)
```typescript
// Widgets can publish/subscribe to system events
class WidgetEventBus {
  static publish(event: string, data: any): void;
  static subscribe(event: string, handler: (data: any) => void): void;
}

// Usage in widgets
WidgetEventBus.publish('persona.spawned', { persona_id: 'abc123' });
WidgetEventBus.subscribe('training.completed', (data) => {
  this.updatePersonaStatus(data.persona_id);
});
```

## 🧪 Testing Strategy

### Layer 1 Tests (Foundation)
- Unit tests for BaseWidget core functionality
- Integration tests for WidgetSystem registration
- No dependency on higher layers

### Layer 2 Tests (Domain)
- Unit tests for domain-specific functionality  
- Integration tests with Layer 1 components
- Mock any command system dependencies

### Layer 3 Tests (Composite)
- Integration tests for widget composition
- End-to-end tests for complete workflows
- Tests for inter-widget communication

## 📊 Implementation Priority

### Phase 1: Layer 1 Foundation ✅
- [x] BaseWidget core functionality
- [x] WidgetSystem and discovery
- [x] Basic styling patterns

### Phase 2: Layer 2 Domain Widgets 🔄
- [ ] Refactor existing widgets to domain structure
- [ ] Create proper base classes for each domain
- [ ] Implement consistent interfaces

### Phase 3: Layer 3 Composition 📋
- [ ] Dashboard widget combining multiple domains
- [ ] Workspace widget for project-focused workflows
- [ ] Portal widget as full application interface

## 🎨 Style Architecture

### Layer 1 Styles (Universal)
```css
/* BaseWidget.css - Universal widget patterns */
:host {
  /* Core widget container styles */
}
.widget-header { /* Standard header pattern */ }
.widget-content { /* Standard content area */ }
.widget-actions { /* Standard action buttons */ }
```

### Layer 2 Styles (Domain-Specific)
```css
/* CommunicationWidget.css - Communication domain styles */
.communication-widget {
  /* Communication-specific patterns */
}

/* IntelligenceWidget.css - AI domain styles */  
.intelligence-widget {
  /* AI-specific patterns like progress bars, metrics */
}
```

### Layer 3 Styles (Composition)
```css
/* DashboardWidget.css - Layout and composition */
.dashboard-layout {
  /* Grid/flex layouts for multiple widgets */
}
```

This middle-out widget architecture creates clean separation of concerns, proper dependency management, and enables systematic testing while maintaining the flexibility for widgets to compose into more complex interfaces.