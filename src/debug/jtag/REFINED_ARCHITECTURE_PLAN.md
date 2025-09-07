# REFINED ARCHITECTURE PLAN - Post-Learning

## **🎯 WHAT I LEARNED FROM MY MISTAKES**

### **Critical Failures:**
1. **Coded without research** - Didn't understand existing JTAGClient architecture
2. **Jammed everything into BaseWidget** - Should be at different layers 
3. **Created duplicate transport systems** - Instead of using existing system/transports/
4. **Broke layered architecture** - Mixed concerns that belong at API/service/transport layers
5. **Failed to test incrementally** - Broke working UI with reckless changes

### **Key Realizations:**
- **BaseWidget should be NAIVE** - Generic operations, not specific implementations
- **Existing JTAGClient pattern works** - `window.jtag.commands.screenshot()` etc.
- **Transport layer exists** - Comprehensive system at `system/transports/shared/`  
- **User hierarchy is domain API** - Belongs in `api/types/` not just `shared/`

---

## **🏗️ CORRECT LAYERED ARCHITECTURE**

### **Layer 1: API (Public Interface)**
```
api/                           # Consumer-first design
├── client/
│   ├── JTAGClient.ts         # Main client interface (already exists!)
│   └── index.ts              # Client exports
├── types/                    
│   ├── User.ts               # BaseUser, HumanUser, PersonaUser, AgentUser  
│   ├── Commands.ts           # Command parameter/result types
│   └── System.ts             # System types
├── commands/                 # Command interfaces (not implementations)
│   ├── screenshot/           # Screenshot command types
│   ├── chat/                 # Chat command types
│   └── exec/                 # Exec command types
└── index.ts                  # Main barrel export
```

**What goes here:** Types and interfaces that external consumers import

### **Layer 2: Services (Business Logic)**
```
services/                     # Business logic services
├── chat/
│   ├── ChatService.ts        # Business operations: joinRoom, sendMessage
│   ├── UserService.ts        # User operations: authenticate, getProfile
│   └── RoomService.ts        # Room operations: create, list, manage
├── content/
│   ├── FileService.ts        # File operations: save, load, organize
│   └── ThemeService.ts       # Theme operations: discover, load, apply
└── ai/
    ├── PersonaService.ts     # Persona management
    └── ConversationService.ts # AI conversation management
```

**What goes here:** Business logic that operates on domain objects, uses transport layer

### **Layer 3: Transport (Communication)**
```
system/transports/            # Already exists! Don't duplicate
├── shared/
│   ├── TransportBase.ts      # Existing transport infrastructure
│   ├── TransportTypes.ts     # Transport interfaces  
│   └── JTAGTransport.ts      # Main transport implementation
├── browser/                  # Browser-specific transports
└── server/                   # Server-specific transports
```

**What goes here:** Communication protocols, message routing, connection management

### **Layer 4: Widgets (UI Components)**
```
widgets/                      
├── shared/
│   └── BaseWidget.ts         # NAIVE - only generic operations
├── chat/
│   ├── ChatWidget.ts         # Uses ChatService, not transport directly
│   └── UserListWidget.ts     # Uses UserService
└── continuum/
    └── ContinuumWidget.ts    # Uses multiple services
```

**What goes here:** UI components that compose services, minimal business logic

---

## **🔧 BASEWIDGET REDESIGN - NAIVE ABSTRACTIONS**

### **Current Problem (What I Did Wrong):**
```typescript
// WRONG - BaseWidget knows about specific implementations
class BaseWidget {
  async takeScreenshot() { /* screenshot-specific logic */ }
  async saveFile() { /* file-specific logic */ }  
  async queryAI() { /* AI-specific logic */ }
  async jtagOperation() { /* transport-specific logic */ }
}
```

### **Correct Design - Naive BaseWidget:**
```typescript
// RIGHT - BaseWidget is naive, works with generic interfaces
abstract class BaseWidget extends HTMLElement {
  // Generic service injection - naive about what services exist
  protected services: IServiceRegistry;
  protected client: IJTAGClient;  // Uses existing JTAGClient interface
  
  // Generic operations - naive about specific implementations
  protected async callService<T>(serviceName: string, method: string, params?: unknown): Promise<T> {
    const service = this.services.get(serviceName);
    return await service[method](params);
  }
  
  protected async executeCommand<T>(command: string, params?: unknown): Promise<T> {
    return await this.client.commands[command](params);
  }
  
  // Abstract methods - subclasses provide specifics
  abstract render(): Promise<void>;
  abstract initialize(): Promise<void>;
}
```

### **Specific Widgets Use Services:**
```typescript
class ChatWidget extends BaseWidget {
  private chatService: ChatService;
  private userService: UserService;
  
  async initialize() {
    // Get business services (not transport details)
    this.chatService = this.services.get('chat');
    this.userService = this.services.get('user');
  }
  
  async sendMessage(content: string) {
    // Use business service, not direct transport calls
    await this.chatService.sendMessage({
      content,
      roomId: this.currentRoom,
      sender: this.currentUser  // BaseUser interface
    });
  }
}
```

---

## **📋 IMPLEMENTATION ROADMAP**

### **Phase 1: API Layer (Public Interface)**
1. **Move user types to api/types/User.ts** - They are domain concepts
2. **Create api/commands/** - Extract command parameter/result types  
3. **Create api/client/** - Expose existing JTAGClient properly
4. **Create barrel exports** - Single entry point for consumers

### **Phase 2: Service Layer (Business Logic)**  
1. **Create ChatService** - Business operations for chat functionality
2. **Create UserService** - User authentication, profiles, permissions
3. **Create FileService** - File operations, theme loading, content management
4. **Create AIService** - Persona management, conversation handling

### **Phase 3: Widget Refactoring (UI Components)**
1. **Make BaseWidget naive** - Remove specific implementations 
2. **Add service injection** - Generic IServiceRegistry interface
3. **Update specific widgets** - Use services instead of direct transport calls
4. **Remove transport coupling** - Widgets shouldn't know about transport details

### **Phase 4: Integration (Clean Boundaries)**
1. **Service registration** - How widgets get access to services
2. **Dependency injection** - Clean service composition  
3. **Testing strategy** - Mock services for widget tests
4. **Documentation** - Clear layer boundaries and responsibilities

---

## **🎯 SUCCESS CRITERIA**

### **Clean Abstractions:**
- ✅ BaseWidget has no specific imports (screenshot, file, AI types)
- ✅ Chat module only imports BaseUser, never HumanUser/PersonaUser  
- ✅ Services handle business logic, widgets handle UI
- ✅ Transport layer stays isolated in system/transports/

### **Maintainability:**  
- ✅ Adding new user type doesn't require changing existing widgets
- ✅ Adding new command doesn't require changing BaseWidget
- ✅ Each layer can be tested independently
- ✅ Clear separation of concerns across layers

### **Developer Experience:**
- ✅ External consumers import from single `api/` entry point
- ✅ Widget developers work with business services, not transport
- ✅ Each module is self-contained and understandable
- ✅ AI assistance works better with clear, naive abstractions

---

## **🚨 CRITICAL PRINCIPLES TO FOLLOW**

1. **Research First** - Understand existing architecture before coding
2. **Layer Properly** - API → Services → Transport → Widgets  
3. **Test Incrementally** - Deploy and test after each small change
4. **Respect Existing Code** - Extend, don't duplicate or break
5. **Naive Abstractions** - Each layer naive about layers below it

**This plan transforms the architecture from "everything mixed together" to clean, layered, maintainable design where each component has a single, clear responsibility.**