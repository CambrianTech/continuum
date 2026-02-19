# JTAG Widget System Architecture

## 🏗️ **Core Architectural Vision**

### **Dynamic Desktop Interface**
```
┌─────────────────────────────────────────────────────────┐
│ sidebar-panel  │ draggable │  main-panel  │ draggable │ │
│               │    bar    │              │    bar    │ │
├───────────────┼───────────┼──────────────┼───────────┤ │
│ continuum-    │           │ content-tabs │           │ │
│ emoter        │           │ version-info │           │ │
├───────────────┤           │ status-btns  │           │ │
│ status-view   │           ├──────────────┤           │ │
├───────────────┤           │              │           │ │
│ dynamic-list  │           │ content-view │           │ │
│ • academy     │           │ [WIDGET]     │           │ │
│ • general     │           │              │           │ │
│ • community   │           │              │           │ │
│ • sharing     │           │              │           │ │
│ ...           │           │              │           │ │
└───────────────┴───────────┴──────────────┴───────────┘ │
```

**Fundamentally Different**: The HTML page itself is mostly empty. Widgets dynamically populate both sidebar content AND main content view.

### **No Static UI Principle**
```typescript
// ❌ WRONG: Static HTML definitions
<div class="chat-widget">
  <div class="messages">...</div>
  <input class="message-input" />
</div>

// ✅ RIGHT: Dynamic widget delivery
interface WidgetManifest {
  html: string;        // Widget's own HTML structure  
  css: string;         // Widget's own styling
  javascript: string;  // Widget's own behavior (compiled TypeScript)
  dependencies?: string[]; // Other widgets this depends on
}
```

## 🎯 **Widget Architecture Patterns**

### **1. Self-Contained Portals**
Each widget is a complete, independent portal:

```
widgets/chat-widget/
├── package.json           # Widget metadata & dependencies
├── manifest.json          # Widget registration info
├── public/               # 🔑 Static assets served by daemon
│   ├── chat-widget.html  # Widget's HTML structure
│   ├── chat-widget.css   # Widget's styling
│   └── chat-widget.js    # Compiled TypeScript behavior
├── shared/
│   ├── ChatWidget.ts     # Core widget logic
│   └── ChatTypes.ts      # Widget-specific types
├── browser/
│   └── ChatWidgetBrowser.ts  # Browser-specific logic
├── server/
│   └── ChatWidgetServer.ts   # Server-specific logic
└── README.md
```

### **2. Dynamic Generation - No Switch Statements**
```typescript
// ❌ WRONG: God object with switch statements
class WidgetRenderer {
  render(type: string) {
    switch (type) {
      case 'chat': return new ChatWidget();
      case 'academy': return new AcademyWidget();
      // ... endless switches
    }
  }
}

// ✅ RIGHT: Dynamic discovery and generation
class WidgetRegistry {
  private widgets = new Map<string, WidgetManifest>();
  
  async discoverWidgets(): Promise<void> {
    // Auto-discover widgets by scanning directories
    const widgetDirs = await fs.readdir('widgets');
    for (const dir of widgetDirs) {
      const manifest = await this.loadManifest(dir);
      this.widgets.set(manifest.name, manifest);
    }
  }
  
  async loadWidget(name: string, container: HTMLElement): Promise<void> {
    const manifest = this.widgets.get(name);
    // Dynamically inject widget's HTML, CSS, JS
    await this.injectWidget(manifest, container);
  }
}
```

### **3. Page-Driven Context System**
```typescript
interface PageContext {
  type: 'chat' | 'academy' | 'code-editor' | 'browser' | 'arcade';
  contentWidget: string;        // Main content area widget
  sidebarWidgets: string[];     // Dynamic sidebar widgets for this context
  data?: any;                   // Page-specific data
}

// Different page types load different widget combinations
const PAGE_CONTEXTS: Record<string, PageContext> = {
  academy: {
    type: 'academy',
    contentWidget: 'academy-trainer',
    sidebarWidgets: ['academy-controls', 'lora-manager', 'persona-list', 'thresholds']
  },
  chat: {
    type: 'chat', 
    contentWidget: 'chat-widget',
    sidebarWidgets: ['room-list', 'participants', 'chat-settings']
  },
  code: {
    type: 'code-editor',
    contentWidget: 'code-editor',
    sidebarWidgets: ['file-tree', 'git-status', 'debug-panel']
  }
};
```

## 🚀 **Dynamic Widget Loading System**

### **Widget Daemon Architecture**
```typescript
class WidgetDaemon extends DaemonBase {
  private widgetRegistry = new WidgetRegistry();
  private renderEngine = new WidgetRenderEngine();
  
  // Core widget operations
  async loadWidget(name: string, container: string): Promise<WidgetResult> {
    const manifest = await this.widgetRegistry.getManifest(name);
    return await this.renderEngine.render(manifest, container);
  }
  
  async unloadWidget(name: string): Promise<void> {
    await this.renderEngine.cleanup(name);
  }
  
  async updateWidget(name: string, data: any): Promise<void> {
    await this.renderEngine.update(name, data);
  }
}
```

### **Widget Render Engine**
```typescript
class WidgetRenderEngine {
  async render(manifest: WidgetManifest, containerId: string): Promise<void> {
    const container = document.getElementById(containerId);
    
    // 1. Inject widget's HTML structure
    container.innerHTML = manifest.html;
    
    // 2. Inject widget's CSS (scoped to avoid conflicts)
    const style = document.createElement('style');
    style.textContent = this.scopeCSS(manifest.css, manifest.name);
    document.head.appendChild(style);
    
    // 3. Load and execute widget's JavaScript
    await this.loadWidgetScript(manifest.javascript, manifest.name);
    
    // 4. Initialize widget with JTAG command interface
    const widget = window[`${manifest.name}Widget`];
    if (widget && typeof widget.initialize === 'function') {
      await widget.initialize(this.createWidgetContext());
    }
  }
  
  private createWidgetContext(): WidgetContext {
    return {
      executeCommand: (cmd, params) => this.widgetDaemon.executeCommand(cmd, params),
      addEventListener: (event, handler) => this.eventSystem.subscribe(event, handler),
      emit: (event, data) => this.eventSystem.emit(event, data),
      getPageContext: () => this.pageManager.getCurrentContext()
    };
  }
}
```

## 🔄 **Event-Driven Communication**

### **Cross-Environment Event System**
```typescript
interface WidgetEventSystem {
  // Widget-to-widget communication
  subscribe(event: string, handler: (data: any) => void): void;
  emit(event: string, data: any): void;
  
  // Cross-environment events (browser ↔ server)
  subscribeRemote(event: string, handler: (data: any) => void): void;
  emitRemote(event: string, data: any): void;
}

// Example: Chat widget responding to academy events
class ChatWidget {
  initialize(context: WidgetContext) {
    // Listen for academy training events
    context.subscribeRemote('academy:training-complete', (data) => {
      this.displayTrainingResult(data);
    });
    
    // Listen for room changes
    context.subscribe('chat:room-changed', (roomData) => {
      this.switchToRoom(roomData.roomId);
    });
  }
  
  async sendMessage(message: string) {
    // Send through JTAG command system
    await this.context.executeCommand('chat', { message });
    
    // Emit local event for other widgets
    this.context.emit('chat:message-sent', { message });
  }
}
```

### **Smart Event Routing**
```typescript
class WidgetEventRouter {
  route(event: string, data: any, source: WidgetContext) {
    // Local events - same browser context
    if (event.startsWith('local:')) {
      this.routeLocal(event, data);
    }
    
    // Cross-environment events - browser ↔ server
    else if (event.startsWith('remote:')) {
      this.routeRemote(event, data, source);
    }
    
    // Auto-route based on event pattern
    else {
      this.autoRoute(event, data, source);
    }
  }
  
  private async routeRemote(event: string, data: any, source: WidgetContext) {
    // Route through JTAG message system to server
    const message = JTAGMessageFactory.createEvent(
      source.context,
      'widget-daemon',
      event,
      data
    );
    
    await source.router.postMessage(message);
  }
}
```

## 📁 **Widget File Structure**

### **Public Directory Pattern**
```typescript
// Each widget serves its own static files
widgets/academy-trainer/
├── public/                    # 🔑 Served by HTTP daemon
│   ├── academy-trainer.html   # Widget HTML template
│   ├── academy-trainer.css    # Widget styles (or .scss)
│   ├── academy-trainer.js     # Compiled TypeScript
│   ├── assets/
│   │   ├── icons/
│   │   └── sounds/
│   └── manifest.json          # Widget metadata
├── shared/
│   ├── AcademyTrainer.ts      # Core widget logic
│   └── AcademyTypes.ts
├── browser/
│   └── AcademyTrainerBrowser.ts
└── server/
    └── AcademyTrainerServer.ts
```

### **Widget Manifest System**
```json
{
  "name": "academy-trainer",
  "version": "1.0.0",
  "displayName": "Academy Trainer",
  "description": "AI training interface with LoRA management",
  "type": "content-widget",
  "contexts": ["academy"],
  "dependencies": ["persona-manager", "threshold-controls"],
  "assets": {
    "html": "academy-trainer.html",
    "css": "academy-trainer.css", 
    "javascript": "academy-trainer.js"
  },
  "permissions": [
    "file-access",
    "chat-integration", 
    "screenshot-capture"
  ]
}
```

## 🎨 **Modern Build Pipeline**

### **TypeScript to JavaScript Compilation**
```typescript
// Build process for each widget
class WidgetBuilder {
  async buildWidget(widgetPath: string): Promise<BuildResult> {
    // 1. Compile TypeScript to JavaScript
    const tsResult = await this.compileTypeScript(
      path.join(widgetPath, 'shared/*.ts'),
      path.join(widgetPath, 'browser/*.ts')
    );
    
    // 2. Process SCSS to CSS (if present)
    const cssResult = await this.processSCSS(
      path.join(widgetPath, 'styles/*.scss')
    );
    
    // 3. Bundle assets and copy to public/
    await this.bundleAssets(widgetPath, tsResult, cssResult);
    
    // 4. Generate widget manifest
    await this.generateManifest(widgetPath);
    
    return { success: true, publicPath: path.join(widgetPath, 'public') };
  }
}
```

### **Hot Reloading for Development**
```typescript
class WidgetDevServer {
  watchForChanges(widgetPath: string) {
    // Watch .ts files
    fs.watch(path.join(widgetPath, 'shared'), () => {
      this.rebuildAndReload(widgetPath);
    });
    
    // Watch .scss files  
    fs.watch(path.join(widgetPath, 'styles'), () => {
      this.rebuildStylesAndReload(widgetPath);
    });
  }
  
  async rebuildAndReload(widgetPath: string) {
    await this.buildWidget(widgetPath);
    
    // Notify browser to reload widget
    this.notifyWidgetReload(path.basename(widgetPath));
  }
}
```

## 🔧 **Integration with JTAG System**

### **Widget-Command Bridge**
```typescript
class WidgetCommandBridge {
  // Widgets execute commands through clean interface
  async executeCommand(command: string, params: any): Promise<CommandResult> {
    // Route through JTAG command daemon
    return await this.widgetDaemon.executeCommand(command, params);
  }
  
  // Commands can trigger widget updates  
  async notifyWidgets(event: string, data: any): Promise<void> {
    const activeWidgets = this.widgetManager.getActiveWidgets();
    
    for (const widget of activeWidgets) {
      if (widget.supportsEvent(event)) {
        await widget.handleEvent(event, data);
      }
    }
  }
}
```

### **Transport Integration**
```typescript
// Widgets integrate seamlessly with JTAG transport system
class WidgetTransportAdapter {
  async sendToServer(widget: string, data: any): Promise<void> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'widget-daemon',
      `widgets/${widget}`,
      data
    );
    
    await this.router.postMessage(message);
  }
  
  async broadcastToWidgets(event: string, data: any): Promise<void> {
    // Use JTAG event system for widget communication
    const eventMessage = JTAGMessageFactory.createEvent(
      this.context,
      'widget-daemon',
      event,
      data
    );
    
    await this.eventSystem.broadcast(eventMessage);
  }
}
```

## 🧪 **Testing Strategy**

### **Widget Testing Framework**
```typescript
class WidgetTestFramework {
  async testWidget(widgetName: string): Promise<TestResult> {
    // 1. Load widget in test environment
    const widget = await this.loadTestWidget(widgetName);
    
    // 2. Test widget initialization
    await widget.initialize(this.createMockContext());
    
    // 3. Test command execution
    const commandResult = await widget.executeCommand('ping', {});
    expect(commandResult.success).toBe(true);
    
    // 4. Test event handling
    const eventPromise = widget.waitForEvent('test:event');
    widget.handleEvent('test:event', { data: 'test' });
    await eventPromise;
    
    // 5. Test cleanup
    await widget.cleanup();
    
    return { success: true, widgetName };
  }
}
```

### **Integration Testing**
```typescript
// Test widget interaction with JTAG system
describe('Widget System Integration', () => {
  it('should load and render widgets dynamically', async () => {
    const widgetDaemon = new WidgetDaemon(context, router);
    
    // Test widget loading
    const result = await widgetDaemon.loadWidget('chat-widget', 'main-content');
    expect(result.success).toBe(true);
    
    // Test widget command execution
    const chatResult = await widgetDaemon.executeCommand('chat', { 
      message: 'Hello from test' 
    });
    expect(chatResult.success).toBe(true);
    
    // Test widget cleanup
    await widgetDaemon.unloadWidget('chat-widget');
  });
});
```

## 🚀 **Migration Strategy**

### **Phase 1: Foundation**
1. **Create WidgetRegistry system** - Dynamic widget discovery
2. **Build WidgetRenderEngine** - HTML/CSS/JS injection
3. **Implement basic event system** - Widget-to-widget communication
4. **Create first modern widget** - Chat widget as reference implementation

### **Phase 2: Content Delivery**
1. **Implement `/public` directory serving** - Static asset delivery
2. **Build TypeScript compilation pipeline** - .ts → .js for widgets
3. **Add SCSS support** - Modern styling capabilities  
4. **Create widget manifest system** - Metadata and dependency management

### **Phase 3: Advanced Features**
1. **Cross-environment events** - Browser ↔ server communication
2. **Hot reloading system** - Development productivity
3. **Widget dependency resolution** - Complex widget relationships
4. **Performance optimization** - Lazy loading, caching, bundling

### **Phase 4: Ecosystem**
1. **Widget development toolkit** - CLI tools for widget creation
2. **Community widget system** - NPM-like distribution
3. **Advanced widget types** - 3D widgets, AI-controlled layouts
4. **Production deployment** - CDN integration, optimization

## 🎯 **Success Metrics**

- **Zero hardcoded UI**: Main HTML should be <100 lines
- **Dynamic widget loading**: Any widget loadable at runtime
- **Cross-environment events**: Perfect browser ↔ server communication  
- **Modern development**: TypeScript + SCSS compilation working
- **Clean separation**: No widget-specific code in system core
- **Elegant architecture**: No switch statements or god objects

---

This architecture transforms the static HTML approach into a truly dynamic, widget-driven system where the desktop adapts to content rather than forcing content into rigid containers.