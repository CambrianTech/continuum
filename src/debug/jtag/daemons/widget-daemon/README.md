# WidgetDaemon - Widget System Integration

## **🎯 Mission**
Bridge between widgets and JTAG command routing system, providing clean interface for widgets to execute commands through powerful JTAG routing while maintaining simple `executeCommand()` API.

## **🏗️ Architecture Pattern**
Follows the **Sparse Override Pattern** with 85% shared logic:

```
daemons/widget-daemon/
├── shared/
│   ├── WidgetDaemon.ts          # Universal interface (85% of logic)
│   ├── WidgetDaemonBase.ts      # Abstract base implementation
│   └── WidgetTypes.ts           # Shared types and contracts
├── browser/
│   └── WidgetDaemonBrowser.ts   # DOM integration (10%)
├── server/
│   └── WidgetDaemonServer.ts    # Command orchestration (5%)
└── README.md                    # This documentation
```

## 🎯 Purpose

**Bridge between widgets and superior JTAG daemon routing system**

The WidgetDaemon provides a clean interface for widgets to execute commands through our powerful JTAG routing system while maintaining the simple `executeCommand()` API that widgets expect.

## 🏗️ Architecture

### **JTAG Integration Flow:**
```
Widget → BaseWidget.executeCommand() 
      → WidgetDaemon.executeCommand() 
      → JTAGRouter.postMessage() 
      → CommandDaemon 
      → Specific Command (screenshot, navigate, proxy-navigate, etc.)
```

### **Key Components:**
- **WidgetDaemon**: Bridge daemon with `executeCommand()` interface
- **BaseWidget**: Updated to use WidgetDaemon instead of legacy continuum API
- **Auto-Discovery**: WidgetDaemon automatically registered in daemon structure
- **Global Access**: `window.widgetDaemon` available to all widgets

## 🚀 Commands Available to Widgets

### **Standard Commands:**
```typescript
// Screenshots (including cross-origin iframe content!)
await this.executeCommand('screenshot', { 
  querySelector: '#proxy-iframe',
  filename: 'training-capture.png' 
});

// Navigation
await this.executeCommand('navigate', { 
  url: 'https://example.com' 
});

// Cross-origin proxy navigation (SOLVES SCREENSHOT PROBLEM!)
await this.executeCommand('proxy-navigate', {
  url: 'https://external-site.com',
  target: 'training-iframe'
});

// File operations
await this.executeCommand('fileSave', { 
  filename: 'data.json', 
  content: JSON.stringify(data)
});

await this.executeCommand('fileLoad', { 
  filename: 'config.json' 
});

// Browser interactions
await this.executeCommand('click', { 
  querySelector: 'button.submit' 
});

await this.executeCommand('type', { 
  querySelector: 'input[name="query"]',
  text: 'search term'
});

// Chat/AI integration
await this.executeCommand('chat', {
  message: 'Analyze this screenshot',
  room: 'training'
});
```

## 🌐 **Cross-Origin Training Solution**

### **THE PROBLEM WE SOLVE:**
- **html2canvas + Cross-Origin Iframe = Broken Screenshots**
- Browser security prevents JavaScript access to cross-origin iframe content
- Training on external websites was impossible

### **OUR SOLUTION:**
```typescript
// 1. Navigate iframe to proxied site (same-origin)
await this.executeCommand('proxy-navigate', {
  url: 'https://training-target.com',
  target: 'training-iframe'
});

// 2. Take screenshots (NOW WORKS!)
await this.executeCommand('screenshot', {
  querySelector: '#training-iframe',
  filename: 'training-step-1.png'
});

// 3. Interact with proxied content
await this.executeCommand('click', { 
  querySelector: '#training-iframe button.submit' 
});

// 4. Capture results
await this.executeCommand('screenshot', {
  querySelector: '#training-iframe',
  filename: 'training-step-2.png'
});
```

## 🏭 Widget Development

### **Creating JTAG-Based Widgets:**

```typescript
// All widgets extend BaseWidget and get JTAG integration automatically
class TrainingWidget extends BaseWidget {
  static get widgetName(): string {
    return 'training-widget';
  }

  protected widgetName = 'TrainingWidget';
  protected widgetIcon = '🧠';
  protected widgetTitle = 'AI Training Interface';

  // Use JTAG commands directly
  async startTraining(url: string) {
    // Navigate to training site via proxy
    await this.executeCommand('proxy-navigate', { url });
    
    // Take initial screenshot
    await this.executeCommand('screenshot', {
      filename: `training-${Date.now()}.png`
    });
    
    // Continue training workflow...
  }
}
```

### **Widget Architecture Principles:**
- **JTAG Command Integration**: All commands route through daemon system
- **Session Awareness**: Full session management and logging
- **Type Safety**: Proper TypeScript integration throughout
- **Error Handling**: Comprehensive error reporting and recovery
- **Testing**: Built-in testing infrastructure

## 📁 File Structure

### **JTAG Widget System:**
```
src/debug/jtag/
├── daemons/
│   ├── widget-daemon/                    # Widget-JTAG bridge
│   │   ├── shared/WidgetDaemon.ts
│   │   ├── browser/WidgetDaemonBrowser.ts
│   │   └── server/WidgetDaemonServer.ts
│   ├── command-daemon/commands/
│   │   ├── proxy-navigate/               # Cross-origin navigation
│   │   ├── screenshot/                   # Works with proxy content!
│   │   ├── navigate/                     # Standard navigation
│   │   └── file/                        # File operations
│   └── proxy-daemon/                     # HTTP proxy for cross-origin
│       ├── shared/ProxyDaemon.ts
│       └── server/ProxyDaemonServer.ts
└── widgets/                              # JTAG-based widgets (future)
    ├── training-widget/
    ├── proxy-widget/
    └── dashboard-widget/
```

## 🔧 Configuration

### **Widget Registration:**
```typescript
// Widgets auto-register through JTAG structure generation
// No manual registration needed!

// Usage in HTML:
<training-widget></training-widget>
<proxy-widget></proxy-widget>
```

### **Command Configuration:**
```typescript
interface WidgetCommandConfig {
  sessionId?: UUID;        // Auto-populated
  timeout?: number;        // Command timeout
  priority?: 'high' | 'normal' | 'low';
  retries?: number;        // Auto-retry on failure
}
```

## 🧪 Testing

### **Widget Testing Pattern:**
```typescript
// Test widget commands through JTAG system
describe('TrainingWidget', () => {
  let widget: TrainingWidget;
  
  beforeEach(async () => {
    widget = new TrainingWidget();
    await widget.connectedCallback();
  });

  it('should take proxy screenshots', async () => {
    // Test cross-origin screenshot capability
    const result = await widget.executeCommand('proxy-navigate', {
      url: 'https://example.com'
    });
    expect(result.success).toBe(true);
    
    const screenshot = await widget.executeCommand('screenshot', {
      querySelector: '#proxy-iframe'
    });
    expect(screenshot.success).toBe(true);
  });
});
```

## 🔍 Debugging

### **Command Debugging:**
```typescript
// Enable debug mode for command tracing
await this.executeCommand('screenshot', {
  querySelector: '#target',
  debug: true  // Shows full command routing
});

// Check widget daemon status
const widgetDaemon = (window as any).widgetDaemon;
console.log('WidgetDaemon connected:', widgetDaemon?.isConnected());

// View available commands
console.log('Available commands:', widgetDaemon?.getAvailableCommands?.());
```

### **Log Analysis:**
- **Browser Console**: Widget interaction and command execution
- **JTAG Logs**: Daemon routing and message flow
- **Session Logs**: Per-session command history
- **Server Logs**: Proxy requests and server-side execution

## 🎯 Migration from Legacy Widget System

### **Before (Legacy):**
```typescript
// Old continuum API - brittle, limited
const continuum = (window as any).continuum;
if (continuum) {
  await continuum.execute('screenshot', params);
}
```

### **After (JTAG):**
```typescript
// New JTAG routing - powerful, reliable
await this.executeCommand('screenshot', params);
// Automatically routes through WidgetDaemon → JTAG → CommandDaemon
```

### **Benefits:**
- **✅ Better Routing**: Full JTAG message routing with correlation
- **✅ Session Management**: Proper session awareness and logging
- **✅ Error Handling**: Comprehensive error reporting and recovery
- **✅ Type Safety**: Full TypeScript integration
- **✅ Testing**: Built-in testing infrastructure
- **✅ Cross-Origin**: Solves iframe screenshot limitations
- **✅ Fallback**: Still works with legacy API if needed

## 🚀 Future Enhancements

### **Planned Features:**
- **Widget Factory**: Auto-generate widgets from command definitions
- **Batch Commands**: Execute multiple commands as transactions
- **Real-time Updates**: WebSocket-based widget state synchronization
- **Advanced Proxy**: WebSocket proxy, caching, content filtering
- **AI Integration**: Direct AI command routing for training

### **Widget Types to Build:**
- **TrainingWidget**: AI training interface with proxy navigation
- **ProxyWidget**: Cross-origin web access and screenshot capture
- **DashboardWidget**: System monitoring and control
- **ChatWidget**: AI interaction and conversation management
- **FileWidget**: File system browser and editor

---

**🎯 WidgetDaemon: Your bridge to the powerful JTAG ecosystem with cross-origin superpowers!**