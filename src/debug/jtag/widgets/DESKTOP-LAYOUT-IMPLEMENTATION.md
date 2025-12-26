# Desktop Layout Implementation - VSCode/Discord Style

## 🎯 **SIMPLIFIED APPROACH**

Leave the existing `<chat-widget></chat-widget>` in the HTML exactly as it is. Build the main desktop layout around it piece by piece, starting with the foundational structure.

## 🏗️ **DESKTOP LAYOUT ARCHITECTURE**

### **Target Layout Structure**
```
┌─sidebar-panel─┬─draggable─┬──main-panel──┬─draggable─┬─sidebar-panel─┐
│ continuum-    │    bar    │ content-tabs │    bar    │ (collapsible) │
│ emoter        │           │ version-info │           │               │
├───────────────┤           │ status-btns  │           │               │
│ status-view   │           ├──────────────┤           │               │
├───────────────┤           │              │           │               │
│ dynamic-list  │           │ content-view │           │               │
│ • academy     │           │ [WIDGETS]    │           │               │
│ • general     │           │ chat-widget  │           │               │
│ • community   │           │              │           │               │
│ ...           │           │              │           │               │
└───────────────┴───────────┴──────────────┴───────────┴───────────────┘
```

## 📁 **IMPLEMENTATION PLAN**

### **Phase 1: Basic Desktop Structure**
Build the main layout skeleton first, piece by piece:

```html
<!-- examples/widget-ui/index.html - Enhanced version -->
<body>
    <div class="desktop-container">
        <!-- Left Sidebar -->
        <aside class="sidebar-panel left-sidebar">
            <div class="continuum-emoter">🟢</div>
            <div class="status-view">Status Panel</div>
            <div class="dynamic-list">
                <div class="list-item">• academy</div>
                <div class="list-item">• general</div>
                <div class="list-item">• community</div>
            </div>
        </aside>
        
        <!-- Draggable Separator -->
        <div class="draggable-separator left-sep"></div>
        
        <!-- Main Content Area -->
        <main class="main-panel">
            <div class="content-tabs">
                <div class="tab active">General Chat</div>
                <div class="tab">Academy</div>
            </div>
            <div class="status-bar">
                <div class="version-info">v1.0.464</div>
                <div class="status-buttons">
                    <button class="status-btn">🔄 Refresh</button>
                    <button class="status-btn">📊 Export</button>
                </div>
            </div>
            <div class="content-view">
                <!-- Existing chat widget stays exactly here -->
                <chat-widget></chat-widget>
            </div>
        </main>
        
        <!-- Right Draggable Separator -->
        <div class="draggable-separator right-sep"></div>
        
        <!-- Right Sidebar (initially hidden/collapsible) -->
        <aside class="sidebar-panel right-sidebar collapsed">
            <div class="sidebar-content">Right Panel Content</div>
        </aside>
    </div>

    <!-- Existing script loading -->
    <script type="module" src="/dist/browser-index.js"></script>
</body>
```

### **Phase 2: CSS Grid Layout System**
```css
/* examples/widget-ui/public/desktop.css */
.desktop-container {
  display: grid;
  grid-template-columns: 
    250px                    /* left sidebar */
    4px                      /* left separator */
    1fr                      /* main content */
    4px                      /* right separator */
    0px;                     /* right sidebar (collapsed) */
  grid-template-rows: 100vh;
  height: 100vh;
  overflow: hidden;
}

.sidebar-panel {
  background: var(--color-surface-panel);
  border-right: 1px solid var(--color-border-subtle);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.left-sidebar {
  grid-column: 1;
}

.right-sidebar {
  grid-column: 5;
  border-left: 1px solid var(--color-border-subtle);
  border-right: none;
  transition: all 0.3s ease;
}

.right-sidebar.collapsed {
  width: 0;
  min-width: 0;
  opacity: 0;
}

.right-sidebar.expanded {
  width: 250px;
  opacity: 1;
}

.draggable-separator {
  background: var(--color-border-subtle);
  cursor: ew-resize;
  transition: background-color 0.2s;
}

.draggable-separator:hover {
  background: var(--color-primary-500);
}

.main-panel {
  display: flex;
  flex-direction: column;
  background: var(--color-surface-background);
}

.content-tabs {
  display: flex;
  background: var(--color-surface-panel);
  border-bottom: 1px solid var(--color-border-subtle);
  padding: 0 16px;
}

.tab {
  padding: 12px 16px;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: all 0.2s;
}

.tab.active {
  border-bottom-color: var(--color-primary-500);
  color: var(--color-primary-500);
}

.status-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 16px;
  background: var(--color-surface-panel);
  border-bottom: 1px solid var(--color-border-subtle);
  font-size: 12px;
}

.content-view {
  flex: 1;
  padding: 16px;
  overflow-y: auto;
}

/* Chat widget stays exactly as it was */
chat-widget {
  display: block;
  height: 100%;
  max-height: 500px;
}
```

### **Phase 3: Component Implementation**

#### **3.1 Continuum Emoter Component**
```typescript
// widgets/desktop/components/ContinuumEmoter.ts
export class ContinuumEmoter extends HTMLElement {
  connectedCallback() {
    this.innerHTML = `
      <div class="emoter-status">
        <div class="status-dot"></div>
        <span class="status-text">continuum</span>
      </div>
    `;
    
    this.updateStatus();
    setInterval(() => this.updateStatus(), 5000);
  }
  
  private updateStatus() {
    const dot = this.querySelector('.status-dot') as HTMLElement;
    const text = this.querySelector('.status-text') as HTMLElement;
    
    if (dot && text) {
      // Simple status indication
      dot.style.backgroundColor = '#00d4ff';
      text.textContent = 'continuum';
    }
  }
}

customElements.define('continuum-emoter', ContinuumEmoter);
```

#### **3.2 Dynamic List Component**
```typescript
// widgets/desktop/components/DynamicList.ts
export class DynamicList extends HTMLElement {
  private items = ['academy', 'general', 'community', 'sharing'];
  
  connectedCallback() {
    this.render();
  }
  
  private render() {
    this.innerHTML = `
      <div class="list-header">
        <h3>Contexts</h3>
      </div>
      <div class="list-items">
        ${this.items.map(item => `
          <div class="list-item ${item === 'general' ? 'active' : ''}" data-context="${item}">
            <span class="item-bullet">•</span>
            <span class="item-name">${item}</span>
          </div>
        `).join('')}
      </div>
    `;
    
    this.setupEventListeners();
  }
  
  private setupEventListeners() {
    this.querySelectorAll('.list-item').forEach(item => {
      item.addEventListener('click', (e) => {
        const context = (e.currentTarget as HTMLElement).dataset.context;
        this.switchContext(context);
      });
    });
  }
  
  private switchContext(context: string) {
    // Remove active from all items
    this.querySelectorAll('.list-item').forEach(item => {
      item.classList.remove('active');
    });
    
    // Add active to selected
    const selectedItem = this.querySelector(`[data-context="${context}"]`);
    selectedItem?.classList.add('active');
    
    // Emit event for main content to switch
    this.dispatchEvent(new CustomEvent('context-switch', {
      detail: { context },
      bubbles: true
    }));
  }
}

customElements.define('dynamic-list', DynamicList);
```

#### **3.3 Draggable Separator Implementation**
```typescript
// widgets/desktop/components/DraggableSeparator.ts
export class DraggableSeparator extends HTMLElement {
  private isDragging = false;
  private startX = 0;
  private startWidth = 0;
  
  connectedCallback() {
    this.addEventListener('mousedown', this.handleMouseDown.bind(this));
    document.addEventListener('mousemove', this.handleMouseMove.bind(this));
    document.addEventListener('mouseup', this.handleMouseUp.bind(this));
  }
  
  private handleMouseDown(e: MouseEvent) {
    this.isDragging = true;
    this.startX = e.clientX;
    
    // Get the current sidebar width
    const sidebar = this.previousElementSibling as HTMLElement;
    this.startWidth = sidebar.offsetWidth;
    
    document.body.style.cursor = 'ew-resize';
    document.body.style.userSelect = 'none';
  }
  
  private handleMouseMove(e: MouseEvent) {
    if (!this.isDragging) return;
    
    const deltaX = e.clientX - this.startX;
    const newWidth = this.startWidth + deltaX;
    
    // Constrain width
    const minWidth = 200;
    const maxWidth = 400;
    const constrainedWidth = Math.max(minWidth, Math.min(maxWidth, newWidth));
    
    // Update sidebar width
    const sidebar = this.previousElementSibling as HTMLElement;
    sidebar.style.width = `${constrainedWidth}px`;
  }
  
  private handleMouseUp() {
    this.isDragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }
}

customElements.define('draggable-separator', DraggableSeparator);
```

## 🔧 **INTEGRATION STEPS**

### **Step 1: Update widget-ui HTML**
```html
<!-- examples/widget-ui/index.html -->
<!-- Replace the simple widget-area with desktop layout -->
<div class="desktop-container">
    <aside class="sidebar-panel left-sidebar">
        <continuum-emoter></continuum-emoter>
        <div class="status-view">
            <div id="connection-status" class="status">Initializing...</div>
        </div>
        <dynamic-list></dynamic-list>
    </aside>
    
    <draggable-separator class="left-sep"></draggable-separator>
    
    <main class="main-panel">
        <div class="content-tabs">
            <div class="tab active">General Chat</div>
            <div class="tab">Academy</div>
        </div>
        <div class="status-bar">
            <div class="version-info">JTAG Widget System v1.0.464</div>
            <div class="status-buttons">
                <button onclick="takeScreenshot()">📸 Screenshot</button>
                <button onclick="clearWidgetLog()">🧹 Clear Log</button>
            </div>
        </div>
        <div class="content-view">
            <!-- EXISTING CHAT WIDGET STAYS HERE -->
            <chat-widget></chat-widget>
        </div>
    </main>
    
    <draggable-separator class="right-sep"></draggable-separator>
    <aside class="sidebar-panel right-sidebar collapsed"></aside>
</div>
```

### **Step 2: Load Desktop Components**
```typescript
// examples/widget-ui/src/index.ts - Add after existing code
console.log('🏗️ Loading desktop components...');

// Import desktop components
import '../widgets/desktop/components/ContinuumEmoter';
import '../widgets/desktop/components/DynamicList';
import '../widgets/desktop/components/DraggableSeparator';

console.log('✅ Desktop layout components loaded');
```

### **Step 3: Add CSS Styling**
```html
<!-- examples/widget-ui/index.html - Add CSS link -->
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>JTAG Widget System Demo</title>
    <link rel="stylesheet" href="/public/desktop.css">
</head>
```

## 🧪 **TESTING THE DESKTOP LAYOUT**

### **Visual Development Workflow**
```bash
# Start the system
JTAG_WORKING_DIR="examples/widget-ui" npm start

# Capture the new desktop layout
./jtag interface/screenshot --querySelector=".desktop-container" --filename="desktop-layout-v1.png"

# Test individual components
./jtag interface/screenshot --querySelector=".left-sidebar" --filename="left-sidebar.png"
./jtag interface/screenshot --querySelector=".main-panel" --filename="main-panel.png"

# Test that existing chat widget still works
./jtag interface/screenshot --querySelector="chat-widget" --filename="chat-widget-in-desktop.png"
```

### **Interactive Testing**
```bash
# Test draggable separators
./jtag exec --code="
  const sep = document.querySelector('.draggable-separator');
  sep.dispatchEvent(new MouseEvent('mousedown', { clientX: 250 }));
  document.dispatchEvent(new MouseEvent('mousemove', { clientX: 300 }));
  document.dispatchEvent(new MouseEvent('mouseup'));
  console.log('✅ Separator drag simulation complete');
" --environment="browser"

# Test context switching
./jtag exec --code="
  const academyItem = document.querySelector('[data-context=\"academy\"]');
  academyItem.click();
  console.log('✅ Context switch to academy');
" --environment="browser"
```

## 🎯 **ITERATIVE IMPROVEMENT**

Build piece by piece:

1. **✅ Basic grid layout** - Get the structure working
2. **✅ Sidebar components** - Emoter, status, list
3. **✅ Main panel structure** - Tabs, status bar, content
4. **✅ Draggable separators** - Interactive resizing
5. **🎯 Polish animations** - Smooth transitions
6. **🎯 Theme integration** - CSS custom properties
7. **🎯 Widget integration** - Load widgets dynamically

The existing `<chat-widget>` stays exactly where it is and keeps working while we build the desktop around it. Simple, clean, and no breaking changes!