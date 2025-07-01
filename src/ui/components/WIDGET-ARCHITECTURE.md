# Widget Architecture Documentation

## Modern Server Controls for Web Components

Continuum widgets combine the **declarative simplicity of classic server controls** with **modern web component architecture**. Each widget is a self-contained unit with frontend, backend, assets, and tests co-located in a single directory.

## Widget Directory Structure

```
src/ui/components/ProjectWidget/
├── ProjectWidget.ts          # Frontend TypeScript (optional)
├── ProjectWidget.backend.ts  # Backend handlers (optional)
├── ProjectWidget.html        # HTML template (optional)
├── ProjectWidget.css         # Styles (optional)
├── assets/                   # Widget-specific assets
│   ├── icons/
│   └── videos/
└── test/                     # Widget tests
    ├── unit/
    └── integration/
```

## Asset Declaration System

Widgets declare their asset needs through simple arrays. BaseWidget automatically handles loading, error handling, and fallbacks.

### CSS Assets (Automatic Base CSS)
```typescript
export class ProjectWidget extends BaseWidget {
  static getBasePath(): string {
    return '/src/ui/components/Project';
  }
  
  // BaseWidget.css automatically included
  static getOwnCSS(): string[] {
    return ['ProjectWidget.css', 'project-theme.css'];
  }
}
```

### HTML Templates (Optional)
```typescript
export class ProjectWidget extends BaseWidget {
  // Option 1: Use HTML files
  static getOwnHTML(): string[] {
    return ['ProjectWidget.html', 'project-form.html'];
  }
  
  // Option 2: Use code-based rendering (fallback)
  protected renderOwnContent(): string {
    return `<div class="project">${this.projectData}</div>`;
  }
}
```

## Progressive Complexity

### Minimal Widget (HTML/CSS Only)
```typescript
// Almost no TypeScript needed
export class SimpleWidget extends BaseWidget {
  static getBasePath() { return '/src/ui/components/Simple'; }
  static getOwnHTML() { return ['SimpleWidget.html']; }
  static getOwnCSS() { return ['SimpleWidget.css']; }
}

customElements.define('simple-widget', SimpleWidget);
```

### Enhanced Widget (Custom Behavior)
```typescript
export class ProjectWidget extends BaseWidget {
  static getBasePath() { return '/src/ui/components/Project'; }
  static getOwnHTML() { return ['ProjectWidget.html']; }
  static getOwnCSS() { return ['ProjectWidget.css']; }
  
  // Add behavior only when needed
  setupEventListeners() {
    this.addEventListener('project:save', this.handleSave);
    this.addEventListener('project:delete', this.handleDelete);
  }
  
  async handleSave(event) {
    const result = await this.executeCommand('project:save', event.detail);
    this.updateUI(result);
  }
}
```

## Event-Driven Server Controls

### Server Control Events - Like onclick but for Server Actions
Widgets can trigger server-side actions using simple event dispatch - no complex API calls needed.

```typescript
// Widget triggers server action (like onclick but server-side)
handleScreenshotClick() {
  this.triggerScreenshot({ includeContext: true });
}

// Widget listens for server callback
setupServerControlListeners() {
  this.addEventListener('widget:screenshot-complete', (event) => {
    const { success, result } = event.detail;
    if (success) {
      this.showMessage(`📸 Screenshot: ${result.filename}`);
    }
  });
}
```

### Universal Server Controls Available to All Widgets
- **`triggerScreenshot(options)`** - Take screenshot of widget or page
- **`triggerRefresh(options)`** - Refresh widget from server
- **`triggerExport(format, options)`** - Export widget data  
- **`triggerValidate(options)`** - Validate widget state

### Universal Observation & Orchestration Layer
The event and command subsystem becomes a **universal observation point** where any system component can monitor and respond to widget interactions:

```typescript
// Widget emits server control event
this.triggerScreenshot({ widgetId: 'chat-widget', context: 'user-demo' });

// ↓ Event flows through universal system ↓

// 1. WidgetServerControls routes to command system
// 2. Personas can observe: "User took screenshot of chat"
// 3. Academy system learns: "Screenshot pattern in chat context"
// 4. Monitoring observes: "Widget interaction frequency"
// 5. AI agents react: "Interesting, user exploring chat features"
// 6. Command executes: Screenshot taken with full context
// 7. Results broadcast: All observers get completion notification
```

### Multi-Observer Architecture
```typescript
// Personas observe widget events for learning
PersonaObserver.on('widget:screenshot', (event) => {
  this.learnUserPattern('screenshot', event.detail);
  this.updateUserPreferences(event.widgetId, 'visual-documentation');
});

// Academy system observes for training data
AcademySystem.on('widget:*', (event) => {
  this.recordInteractionPattern(event.type, event.detail);
  this.generateTrainingExample(event);
});

// Monitoring observes for system health
SystemMonitor.on('widget:*', (event) => {
  this.trackWidgetUsage(event.widgetId);
  this.measureResponseTime(event.timestamp);
});

// AI Agents observe for contextual assistance
AIAgent.on('widget:screenshot', (event) => {
  this.offerHelpBasedOnAction('documentation', event.detail);
  this.suggestRelatedFeatures(event.widgetId);
});
```

### Backend Communication (Future)
```typescript
// Frontend → Backend Communication  
this.dispatchEvent(new CustomEvent('project:archive', {
  detail: { projectId: this.projectId, reason: 'user_request' }
}));

// Backend: Handle widget event
// ProjectWidget.backend.ts
export class ProjectWidgetBackend extends BaseWidgetBackend {
  async 'project:archive'(data, context) {
    await this.archiveProject(data.projectId);
    context.notifyWidgets('project:archived', data);
    context.broadcastToTeam('project-archived', data);
    return { success: true };
  }
}
```

### Event Bus Auto-Routing
The RendererDaemon automatically routes widget events:

- **Event**: `'project:archive'` 
- **Routes to**: `ProjectWidgetBackend.handleArchive()`
- **Convention**: `{widget}:{action}` → `{Widget}Backend.handle{Action}()`

### Cross-Widget Communication
```typescript
// Widget A triggers action
this.executeCommand('update_project', { id: this.projectId });

// Widget B receives server update
window.continuum.on('project-updated', (project) => {
  this.refreshProjectDisplay(project);
});
```

## Hot Swapping & Lifecycle

### Dynamic Widget Replacement
```typescript
// Replace widgets on the fly
await continuum.swapWidget('chat-widget', 'advanced-chat-widget');

// Widgets handle their own lifecycle
class ChatWidget extends BaseWidget {
  disconnectedCallback() {
    this.cleanup(); // Widget manages its own teardown
  }
  
  connectedCallback() {
    this.restore(); // Widget manages its own restoration
  }
}
```

### Override Points
Widgets can selectively override BaseWidget functionality:

```typescript
export class AdvancedWidget extends BaseWidget {
  // Override lifecycle
  async connectedCallback() {
    await this.initializeDatabase();
    super.connectedCallback();
  }
  
  // Override rendering
  protected renderBaseHTML(): string {
    return this.useCustomLayout ? this.myCustomLayout() : super.renderBaseHTML();
  }
  
  // Override asset loading
  async loadCSS(): Promise<string> {
    const css = await super.loadCSS();
    return this.injectThemeVariables(css);
  }
}
```

## Testing Architecture

### Universal Asset Testing
BaseWidget provides automatic asset testing for all widgets:

```typescript
// Tests all declared CSS/HTML assets automatically
const results = await testWidgetAssets(ProjectWidget);
// Verifies all files are served correctly by RendererDaemon
```

### Progressive Test Sophistication
- **Default**: BaseWidget tests cover 95% of widget functionality
- **Custom**: Add widget-specific tests only when needed
- **Zero duplication**: Each test layer focuses on its concerns

## Server Control Pattern

### Key Benefits
✅ **Declarative assets** - just declare file arrays  
✅ **Automatic loading** - BaseWidget handles everything  
✅ **Server-side processing** - RendererDaemon compiles and serves  
✅ **Client-side behavior** - TypeScript adds interactivity  
✅ **Event-driven** - custom events trigger server actions  
✅ **Hot swappable** - widgets can be replaced dynamically  
✅ **Progressive complexity** - start simple, add sophistication when needed  

### Modern Advantages
✅ **Web standards** - custom elements, shadow DOM  
✅ **TypeScript** - type safety and modern tooling  
✅ **Modular architecture** - complete separation of concerns  
✅ **Real-time** - WebSocket communication  
✅ **Co-located** - frontend and backend in same directory  

## Development Workflow

1. **Create widget directory** with desired assets
2. **Declare asset arrays** in TypeScript class
3. **Add HTML/CSS files** as needed
4. **Add backend handlers** for server-side logic
5. **BaseWidget handles** loading, rendering, events automatically
6. **Tests verify** all assets are properly served

## Widget Discovery
The renderer automatically finds and serves widgets based on:
- Directory structure in `/src/ui/components/`
- Declared asset arrays (`getOwnCSS()`, `getOwnHTML()`)
- Asset paths resolved relative to widget's `getBasePath()`

The **widget is the unit of development** - everything needed for that component lives together and is managed as a cohesive unit.