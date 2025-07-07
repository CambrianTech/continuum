# 🎛️ Continuum Widget System

> **⚠️ DOCUMENTATION MOVED**  
> **This README contains outdated architecture patterns.**  
> **For current widget development, see:** **[middle-out/development/widget-architecture.md](../../middle-out/development/widget-architecture.md)**

## 🚀 New Architecture (2025-07-07)

The widget system has been completely redesigned with **layered abstract classes** and **CSS inheritance**:

- **Minimal Burden Principle** - Subclasses provide just `widgetName`, everything else has smart defaults
- **CSS Inheritance Hierarchy** - BaseWidget.css → SidebarWidget.css → UserSelectorWidget.css  
- **Declarative Asset System** - Package.json drives HTML/CSS loading automatically
- **RendererDaemon Efficiency** - Smart caching and optimization

**Simple widget now requires just:**
```typescript
export class MyWidget extends SidebarWidget {
  protected readonly widgetName = 'My';
  // Everything else works automatically!
}
```

---

## 🏗️ Widget Architecture

### **Core Components**

#### **BaseWidget.ts** - Foundation Class
```typescript
export abstract class BaseWidget extends HTMLElement {
  // Standardized properties
  protected widgetName: string;
  protected widgetIcon: string;
  protected widgetTitle: string;
  protected cssPath: string;
  
  // Lifecycle methods
  abstract renderContent(): string;
  abstract setupEventListeners(): void;
  protected async initializeWidget(): Promise<void>;
  
  // CSS management
  async loadCSS(): Promise<string>;
  
  // DOM lifecycle
  connectedCallback(): void;
  disconnectedCallback(): void;
}
```

#### **WidgetSystem.ts** - Management API
```typescript
export class WidgetSystem {
  // Widget registration
  static register(name: string, widget: WidgetInterface): void;
  static get(name: string): WidgetInterface | undefined;
  
  // Event management
  static changeRoom(room: string): void;
  static initialize(): void;
}

// System events
export const WidgetEvents = {
  ROOM_CHANGED: 'continuum:room-changed',
  WIDGET_READY: 'continuum:widget-ready',
  BASE_READY: 'continuum:base-ready'
} as const;
```

---

## 🎯 Available Widgets

### **1. SidebarWidget** - Main Application Sidebar
**Path:** `src/ui/components/Sidebar/SidebarWidget.ts`

**Purpose:** Main navigation and container for other widgets

**Features:**
- Discord/IDE-style draggable resize
- Room tab switching (General/Academy/Projects)
- Child widget containers
- Event broadcasting for room changes

```typescript
class SidebarWidget extends BaseWidget {
  // Room management
  private switchRoom(room: string): void;
  
  // Resize functionality
  private setupResizeHandlers(): void;
  
  // Child widget loading
  private loadChildWidgets(): void;
}
```

### **2. SavedPersonasWidget** - Persona Management
**Path:** `src/ui/components/SavedPersonas/SavedPersonasWidget.ts`

**Purpose:** AI persona creation, editing, and management

**Features:**
- Persona CRUD operations
- Threshold sliders for persona traits
- Real-time persona updates
- Drag-and-drop reordering

```typescript
interface PersonaData {
  id: string;
  name: string;
  description: string;
  thresholds: {
    creativity: number;
    accuracy: number;
    speed: number;
  };
  created: Date;
  lastUsed?: Date;
}
```

### **3. ChatWidget** - Real-time Chat Interface
**Path:** `src/ui/components/Chat/ChatWidget.ts`

**Purpose:** Multi-room chat with AI agents and users

**Features:**
- Room-based messaging
- Message history persistence
- Real-time message updates
- Typing indicators

```typescript
interface ChatMessage {
  id: string;
  room: string;
  sender: string;
  content: string;
  timestamp: Date;
  type: 'user' | 'assistant' | 'system';
}
```

### **4. ActiveProjectsWidget** - Project Management
**Path:** `src/ui/components/ActiveProjects/ActiveProjects.js` (Legacy)

**Purpose:** Workspace and project management

**Features:**
- Project filtering (all, active, completed, archived)
- Working directory display
- Project selection and actions

### **5. UserSelectorWidget** - Agent Selection
**Path:** `src/ui/components/UserSelector/UserSelector.js` (Legacy)

**Purpose:** AI agent and user selection interface

**Features:**
- Multi-agent selection
- Agent discovery and favorites
- Glass submenu animations
- Agent metrics and validation

---

## 🛠️ Creating New Widgets

### **1. Widget Structure**
```
src/ui/components/MyWidget/
├── MyWidget.ts              # Widget implementation
├── MyWidget.css             # Widget styles
├── package.json             # Widget metadata
├── README.md                # Widget documentation
└── test/
    └── MyWidget.test.ts     # Widget tests
```

### **2. Widget Implementation**
```typescript
// src/ui/components/MyWidget/MyWidget.ts
import { BaseWidget } from '../shared/BaseWidget.js';
import { WidgetSystem, WidgetEvents } from '../shared/WidgetSystem.js';

export class MyWidget extends BaseWidget {
  // Widget data
  private data: MyWidgetData[] = [];
  private selectedItem: string | null = null;

  constructor() {
    super();
    
    // Widget metadata
    this.widgetName = 'MyWidget';
    this.widgetIcon = '🎯';
    this.widgetTitle = 'My Custom Widget';
    this.cssPath = '/src/ui/components/MyWidget/MyWidget.css';
  }

  protected async initializeWidget(): Promise<void> {
    // Load initial data
    await this.loadData();
    
    // Setup widget-specific event listeners
    this.setupEventListeners();
    
    // Register with widget system
    WidgetSystem.register(this.widgetName, this);
  }

  setupEventListeners(): void {
    // Listen for system events
    document.addEventListener(WidgetEvents.ROOM_CHANGED, (e: CustomEvent) => {
      this.handleRoomChange(e.detail.room);
    });

    // Widget-specific events
    this.addEventListener('click', this.handleClick.bind(this));
    this.addEventListener('change', this.handleChange.bind(this));
  }

  renderContent(): string {
    return `
      <div class="widget-container">
        <div class="widget-header">
          <div class="header-title">
            <span>${this.widgetIcon}</span>
            <span>${this.widgetTitle}</span>
            <span class="item-count">${this.data.length}</span>
          </div>
          <div class="header-actions">
            <button class="add-btn" title="Add Item">+</button>
          </div>
        </div>
        
        <div class="widget-content">
          ${this.renderItems()}
        </div>
        
        <div class="widget-footer">
          <div class="status-info">
            ${this.selectedItem ? `Selected: ${this.selectedItem}` : 'No selection'}
          </div>
        </div>
      </div>
    `;
  }

  private renderItems(): string {
    if (this.data.length === 0) {
      return `
        <div class="empty-state">
          <div class="empty-icon">📭</div>
          <div class="empty-message">No items yet</div>
        </div>
      `;
    }

    return `
      <div class="item-list">
        ${this.data.map(item => this.renderItem(item)).join('')}
      </div>
    `;
  }

  private renderItem(item: MyWidgetData): string {
    const isSelected = this.selectedItem === item.id;
    
    return `
      <div class="item ${isSelected ? 'selected' : ''}" data-id="${item.id}">
        <div class="item-icon">${item.icon}</div>
        <div class="item-content">
          <div class="item-title">${item.title}</div>
          <div class="item-description">${item.description}</div>
        </div>
        <div class="item-actions">
          <button class="edit-btn" title="Edit">✏️</button>
          <button class="delete-btn" title="Delete">🗑️</button>
        </div>
      </div>
    `;
  }

  // Event handlers
  private handleClick(event: Event): void {
    const target = event.target as HTMLElement;
    const itemElement = target.closest('.item');
    
    if (target.classList.contains('add-btn')) {
      this.addItem();
    } else if (target.classList.contains('edit-btn') && itemElement) {
      this.editItem(itemElement.dataset.id!);
    } else if (target.classList.contains('delete-btn') && itemElement) {
      this.deleteItem(itemElement.dataset.id!);
    } else if (itemElement) {
      this.selectItem(itemElement.dataset.id!);
    }
  }

  private handleRoomChange(room: string): void {
    // Handle room-specific logic
    console.log(`🎯 ${this.widgetName}: Room changed to ${room}`);
    this.loadRoomData(room);
  }

  // Data management
  private async loadData(): Promise<void> {
    try {
      // Load data via Continuum API
      const response = await continuum.execute('mywidget_data');
      this.data = response.data || [];
      this.render();
    } catch (error) {
      console.error(`❌ ${this.widgetName}: Failed to load data:`, error);
    }
  }

  private async addItem(): Promise<void> {
    const newItem = await this.showAddDialog();
    if (newItem) {
      this.data.push(newItem);
      await this.saveData();
      this.render();
    }
  }

  private async editItem(id: string): Promise<void> {
    const item = this.data.find(item => item.id === id);
    if (item) {
      const updatedItem = await this.showEditDialog(item);
      if (updatedItem) {
        Object.assign(item, updatedItem);
        await this.saveData();
        this.render();
      }
    }
  }

  private async deleteItem(id: string): Promise<void> {
    if (confirm('Are you sure you want to delete this item?')) {
      this.data = this.data.filter(item => item.id !== id);
      if (this.selectedItem === id) {
        this.selectedItem = null;
      }
      await this.saveData();
      this.render();
    }
  }

  private selectItem(id: string): void {
    this.selectedItem = id;
    this.render();
    
    // Emit selection event
    this.dispatchEvent(new CustomEvent('item-selected', {
      detail: { id, item: this.data.find(item => item.id === id) },
      bubbles: true
    }));
  }

  private async saveData(): Promise<void> {
    try {
      await continuum.execute('save_mywidget_data', { data: this.data });
    } catch (error) {
      console.error(`❌ ${this.widgetName}: Failed to save data:`, error);
    }
  }
}

// Self-register the widget
customElements.define('my-widget', MyWidget);

// TypeScript interfaces
interface MyWidgetData {
  id: string;
  title: string;
  description: string;
  icon: string;
  created: Date;
  modified?: Date;
}
```

### **3. Widget Styles**
```css
/* src/ui/components/MyWidget/MyWidget.css */
:host {
  display: block;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 12px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  margin-bottom: 15px;
  overflow: hidden;
}

.widget-container {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.widget-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.03);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  color: #e0e6ed;
}

.item-count {
  background: rgba(79, 195, 247, 0.2);
  color: #4FC3F7;
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 12px;
  font-weight: 500;
}

.header-actions button {
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.2);
  color: #e0e6ed;
  border-radius: 6px;
  padding: 4px 8px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.header-actions button:hover {
  background: rgba(79, 195, 247, 0.2);
  border-color: #4FC3F7;
}

.widget-content {
  flex: 1;
  padding: 8px;
  overflow-y: auto;
  max-height: 300px;
}

.empty-state {
  text-align: center;
  padding: 40px 20px;
  color: #8a92a5;
}

.empty-icon {
  font-size: 48px;
  margin-bottom: 16px;
  opacity: 0.5;
}

.item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s ease;
  border: 1px solid transparent;
}

.item:hover {
  background: rgba(255, 255, 255, 0.05);
  border-color: rgba(255, 255, 255, 0.1);
}

.item.selected {
  background: rgba(79, 195, 247, 0.1);
  border-color: rgba(79, 195, 247, 0.3);
}

.item-icon {
  font-size: 20px;
  width: 24px;
  text-align: center;
}

.item-content {
  flex: 1;
}

.item-title {
  font-weight: 500;
  color: #e0e6ed;
  margin-bottom: 4px;
}

.item-description {
  font-size: 12px;
  color: #8a92a5;
  line-height: 1.3;
}

.item-actions {
  display: flex;
  gap: 4px;
  opacity: 0;
  transition: opacity 0.2s ease;
}

.item:hover .item-actions {
  opacity: 1;
}

.item-actions button {
  background: transparent;
  border: none;
  padding: 4px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  transition: background 0.2s ease;
}

.item-actions button:hover {
  background: rgba(255, 255, 255, 0.1);
}

.widget-footer {
  padding: 8px 16px;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(255, 255, 255, 0.02);
}

.status-info {
  font-size: 12px;
  color: #8a92a5;
}
```

### **4. Widget Package.json**
```json
{
  "name": "@continuum/my-widget",
  "version": "1.0.0",
  "description": "My custom Continuum widget",
  "main": "MyWidget.ts",
  "type": "module",
  "continuum": {
    "type": "widget",
    "category": "ui",
    "capabilities": ["display", "interaction", "data-management"],
    "dependencies": ["BaseWidget", "WidgetSystem"],
    "selector": "my-widget",
    "events": {
      "emits": ["item-selected", "data-changed"],
      "listens": ["continuum:room-changed"]
    },
    "api": {
      "commands": ["mywidget_data", "save_mywidget_data"],
      "endpoints": ["/api/mywidget"]
    }
  },
  "scripts": {
    "build": "tsc",
    "test": "jest",
    "lint": "eslint *.ts"
  },
  "devDependencies": {
    "typescript": "^5.0.0",
    "@types/jest": "^29.0.0",
    "jest": "^29.0.0",
    "eslint": "^8.0.0"
  }
}
```

### **5. Widget Tests**
```typescript
// src/ui/components/MyWidget/test/MyWidget.test.ts
import { MyWidget } from '../MyWidget.js';

describe('MyWidget', () => {
  let widget: MyWidget;
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    
    widget = new MyWidget();
    container.appendChild(widget);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  test('should render correctly', () => {
    expect(widget.shadowRoot).toBeTruthy();
    expect(widget.shadowRoot!.querySelector('.widget-container')).toBeTruthy();
  });

  test('should handle item selection', () => {
    const selectSpy = jest.fn();
    widget.addEventListener('item-selected', selectSpy);
    
    // Simulate adding data
    widget['data'] = [
      { id: '1', title: 'Test Item', description: 'Test', icon: '📝', created: new Date() }
    ];
    widget.render();
    
    // Click item
    const item = widget.shadowRoot!.querySelector('.item');
    item?.dispatchEvent(new Event('click', { bubbles: true }));
    
    expect(selectSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        detail: expect.objectContaining({ id: '1' })
      })
    );
  });

  test('should respond to room changes', () => {
    const roomChangeEvent = new CustomEvent('continuum:room-changed', {
      detail: { room: 'academy' }
    });
    
    document.dispatchEvent(roomChangeEvent);
    
    // Verify room change handling
    expect(widget['currentRoom']).toBe('academy');
  });
});
```

---

## 🔄 Widget Lifecycle

### **1. Registration & Loading**
```typescript
// Widgets are loaded via app.js
import('/dist/ui/components/shared/BaseWidget.js');
import('/dist/ui/components/MyWidget/MyWidget.js');

// Widget self-registers when module loads
customElements.define('my-widget', MyWidget);
WidgetSystem.register('MyWidget', new MyWidget());
```

### **2. Initialization Sequence**
```typescript
// 1. Constructor called
constructor() {
  super();
  this.widgetName = 'MyWidget';
  // ... setup
}

// 2. connectedCallback when added to DOM
connectedCallback() {
  this.initializeWidget();
}

// 3. initializeWidget for custom setup
protected async initializeWidget() {
  await this.loadCSS();
  await this.loadData();
  this.setupEventListeners();
  this.render();
}

// 4. setupEventListeners for event handling
setupEventListeners() {
  // Event listeners
}

// 5. renderContent for UI rendering
renderContent() {
  return `<div>...</div>`;
}
```

### **3. Event Communication**
```typescript
// System-wide events
WidgetSystem.changeRoom('academy');  // Triggers room change

// Widget-specific events
this.dispatchEvent(new CustomEvent('data-changed', {
  detail: { data: this.data },
  bubbles: true
}));

// Cross-widget communication
document.addEventListener('my-custom-event', (e) => {
  this.handleCustomEvent(e.detail);
});
```

---

## 🎨 Styling Guidelines

### **1. CSS Architecture**
- **Shadow DOM** - Styles encapsulated per widget
- **CSS Custom Properties** - Use for theming
- **Consistent Spacing** - Follow 8px grid
- **Color Palette** - Use Continuum color scheme

### **2. Color Scheme**
```css
:host {
  /* Background colors */
  --bg-primary: rgba(20, 25, 35, 0.95);
  --bg-secondary: rgba(255, 255, 255, 0.05);
  --bg-tertiary: rgba(255, 255, 255, 0.03);
  
  /* Text colors */
  --text-primary: #e0e6ed;
  --text-secondary: #8a92a5;
  --text-accent: #4FC3F7;
  
  /* Border colors */
  --border-primary: rgba(255, 255, 255, 0.1);
  --border-accent: rgba(79, 195, 247, 0.3);
}
```

### **3. Responsive Design**
```css
/* Base styles for desktop */
.widget-container {
  padding: 16px;
}

/* Tablet adjustments */
@media (max-width: 768px) {
  .widget-container {
    padding: 12px;
  }
}

/* Mobile adjustments */
@media (max-width: 480px) {
  .widget-container {
    padding: 8px;
  }
  
  .item {
    padding: 8px;
  }
}
```

---

## 🧪 Testing Widgets

### **1. Unit Testing**
```bash
# Run widget tests
npm run test:widgets

# Test specific widget
npm run test -- MyWidget

# Watch mode
npm run test:watch
```

### **2. Visual Testing**
```bash
# Screenshot testing
python python-client/ai-portal.py --cmd screenshot --selector "my-widget"

# Interactive testing
python python-client/ai-portal.py --cmd browser_js --script "
  const widget = document.querySelector('my-widget');
  widget.addItem({ title: 'Test', description: 'Testing' });
"
```

### **3. Integration Testing**
```typescript
// Test widget in full UI context
describe('MyWidget Integration', () => {
  test('should integrate with sidebar', async () => {
    // Load full UI
    await import('/src/ui/app.js');
    
    // Find widget in sidebar
    const sidebar = document.querySelector('continuum-sidebar');
    const widget = sidebar?.shadowRoot?.querySelector('my-widget');
    
    expect(widget).toBeTruthy();
  });
});
```

---

## 🚨 Troubleshooting

### **Common Issues**

1. **Widget Not Appearing**
   ```typescript
   // Check registration
   console.log(customElements.get('my-widget')); // Should not be undefined
   
   // Check DOM
   console.log(document.querySelector('my-widget')); // Should exist
   ```

2. **Styles Not Loading**
   ```typescript
   // Check CSS path
   console.log(widget.cssPath); // Should be correct path
   
   // Check CSS loading
   widget.loadCSS().then(css => console.log(css));
   ```

3. **Events Not Working**
   ```typescript
   // Check event listeners
   console.log(widget.getEventListeners?.());
   
   // Test event dispatch
   widget.dispatchEvent(new CustomEvent('test'));
   ```

### **Debug Commands**
```bash
# Widget system status
python python-client/ai-portal.py --cmd debug_widgets

# Widget performance
python python-client/ai-portal.py --cmd widget_metrics

# Widget tree
python python-client/ai-portal.py --cmd browser_js --script "
  console.log(Array.from(document.querySelectorAll('*')).filter(el => el.tagName.includes('-')));
"
```

This widget system provides a robust foundation for building sophisticated, reusable UI components with TypeScript, proper encapsulation, and standardized patterns.