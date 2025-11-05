# Widget Migration Plan - Middle-Out Architecture

## 🔄 Current State Analysis

### Existing Widgets and Their Layer Assignments

#### Layer 1 (Core) - ✅ Foundation Ready
- **BaseWidget.ts** → `core/base-widget/BaseWidget.ts`
- **WidgetSystem.ts** → `core/widget-system/WidgetSystem.ts` 
- **WidgetDiscovery.ts** → `core/widget-system/WidgetDiscovery.ts`
- **DataDisplayWidget.ts** → `core/data-display/DataDisplayWidget.ts`

#### Layer 2 (Domain) - 🔄 Needs Refactoring
- **ChatWidget.ts** → `domain/communication/chat/ChatWidget.ts`
  - Extends: `domain/communication/base/CommunicationWidget.ts`
  - Domain: Message display, real-time communication
  
- **AcademyWidget.ts** → `domain/intelligence/academy/AcademyWidget.ts`
  - Extends: `domain/intelligence/base/IntelligenceWidget.ts`  
  - Domain: AI training visualization, persona evolution
  
- **PersonaWidget.ts** → `domain/intelligence/persona/PersonaWidget.ts`
  - Extends: `domain/intelligence/base/IntelligenceWidget.ts`
  - Domain: Individual AI persona display
  
- **SidebarWidget.ts** → `domain/navigation/sidebar/SidebarWidget.ts`
  - Extends: `domain/navigation/base/NavigationWidget.ts`
  - Domain: Application navigation and widget organization
  
- **VersionWidget.ts** → `domain/system/version/VersionWidget.ts`
  - Extends: `domain/system/base/SystemWidget.ts`
  - Domain: System version and status information

#### Layer 2 (Domain) - 🆕 New Base Classes Needed
- **CommunicationWidget.ts** (New) → Base for chat and messaging
- **IntelligenceWidget.ts** (New) → Base for AI and Academy widgets  
- **NavigationWidget.ts** (New) → Base for sidebar and navigation
- **SystemWidget.ts** (New) → Base for system and monitoring widgets

#### Layer 3 (Composite) - 📋 Future Development
- **DashboardWidget.ts** (New) → Combines Academy + Chat + System widgets
- **WorkspaceWidget.ts** (New) → Project-focused widget composition
- **PortalWidget.ts** (New) → Full application interface

## 📋 Migration Steps

### Phase 1: Layer 1 Foundation (✅ Ready)
1. Move existing core widgets to proper layer structure
2. Ensure no dependencies between Layer 1 components
3. Create comprehensive tests for foundation layer
4. Document Layer 1 interfaces and contracts

### Phase 2: Layer 2 Domain Refactoring (Current Priority)

#### Step 1: Create Domain Base Classes
```typescript
// domain/communication/base/CommunicationWidget.ts
export abstract class CommunicationWidget extends BaseWidget {
  protected abstract sendMessage(message: any): Promise<void>;
  protected abstract onMessageReceived(callback: (msg: any) => void): void;
  protected displayMessage(message: any): void { /* common implementation */ }
  protected formatTimestamp(date: Date): string { /* common implementation */ }
}

// domain/intelligence/base/IntelligenceWidget.ts  
export abstract class IntelligenceWidget extends BaseWidget {
  protected abstract executeAICommand(cmd: string, params: any): Promise<any>;
  protected abstract onAIStatusChange(callback: (status: any) => void): void;
  protected displayMetrics(metrics: any): void { /* common implementation */ }
  protected renderProgressBar(progress: number): string { /* common implementation */ }
}
```

#### Step 2: Refactor Existing Widgets
```typescript
// Before (current)
export class ChatWidget extends BaseWidget {
  // All implementation mixed together
}

// After (domain-based)
export class ChatWidget extends CommunicationWidget {
  protected sendMessage(message: any): Promise<void> {
    return this.executeCommand('chat', { message, room: this.currentRoom });
  }
  
  protected onMessageReceived(callback: (msg: any) => void): void {
    // WebSocket message handling specific to chat
  }
  
  // Chat-specific implementation only
}
```

#### Step 3: Update Dependencies
```typescript
// Old imports
import { BaseWidget } from '../shared/BaseWidget';

// New imports  
import { CommunicationWidget } from '../domain/communication/base/CommunicationWidget';
import { IntelligenceWidget } from '../domain/intelligence/base/IntelligenceWidget';
```

### Phase 3: Layer 3 Composition (Future)

#### Dashboard Widget Example
```typescript
export class DashboardWidget extends BaseWidget {
  private academyWidget: AcademyWidget;
  private chatWidget: ChatWidget;
  private systemWidget: SystemWidget;
  
  protected async initializeWidget(): Promise<void> {
    // Initialize constituent widgets
    this.academyWidget = new AcademyWidget();
    this.chatWidget = new ChatWidget();
    this.systemWidget = new SystemWidget();
    
    // Set up inter-widget communication
    this.setupWidgetCommunication();
    
    // Render composite layout
    this.renderDashboard();
  }
  
  private setupWidgetCommunication(): void {
    // Academy persona spawning updates chat
    this.academyWidget.onPersonaSpawned((persona) => {
      this.chatWidget.addAvailablePersona(persona);
    });
    
    // Chat activity feeds Academy training data
    this.chatWidget.onMessageSent((message) => {
      this.academyWidget.recordTrainingData(message);
    });
  }
}
```

## 🔧 Implementation Strategy

### Backwards Compatibility
```typescript
// Create compatibility layer during migration
export { ChatWidget } from './domain/communication/chat/ChatWidget';
export { AcademyWidget } from './domain/intelligence/academy/AcademyWidget';
// ... other exports for existing code
```

### Testing Strategy
```typescript
// Layer 1 tests - no higher layer dependencies
describe('BaseWidget', () => {
  it('should initialize without dependencies', () => {
    // Test core functionality only
  });
});

// Layer 2 tests - can use Layer 1
describe('CommunicationWidget', () => {
  it('should extend BaseWidget correctly', () => {
    // Test domain functionality with Layer 1 base
  });
});

// Layer 3 tests - can use Layer 1 & 2
describe('DashboardWidget', () => {
  it('should compose multiple domain widgets', () => {
    // Test widget composition and communication
  });
});
```

### Migration Verification
```typescript
// Automated migration verification
class WidgetArchitectureValidator {
  validateLayerDependencies(): boolean {
    // Ensure Layer 1 has no widget dependencies
    // Ensure Layer 2 only uses Layer 1
    // Ensure Layer 3 only uses Layer 1 & 2
  }
  
  validateDomainSeparation(): boolean {
    // Ensure communication widgets don't import intelligence concepts
    // Ensure intelligence widgets don't import navigation concepts
    // etc.
  }
}
```

## 📊 Benefits of Middle-Out Widget Architecture

### 1. **Clear Dependency Management**
- Layer 1 = Foundation (no dependencies)
- Layer 2 = Domain-specific (Layer 1 only)
- Layer 3 = Composition (Layer 1 & 2)

### 2. **Domain Separation**
- Communication widgets handle messaging and real-time updates
- Intelligence widgets handle AI, training, and persona functionality
- Navigation widgets handle routing and application organization
- System widgets handle monitoring and diagnostics

### 3. **Testable Architecture**
- Each layer can be tested independently
- Domain concepts isolated for focused testing
- Composition logic separated from core functionality

### 4. **Scalable Development**
- New widgets follow clear patterns
- Domain bases provide consistent interfaces
- Composition enables complex applications

### 5. **Code Reuse**
- Common patterns implemented once in domain bases
- Widgets share styling and behavior patterns
- Testing utilities shared across domains

This migration creates a solid foundation for widget development while maintaining backwards compatibility and enabling systematic testing of the entire widget ecosystem.