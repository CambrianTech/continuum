# Widget Architecture - Native Web Components + Shadow DOM

## Table of Contents
- [Web Component Architecture](#web-component-architecture)
- [Shadow DOM CSS System](#shadow-dom-css-system)
- [Modular CSS Architecture](#modular-css-architecture)
- [Widget Base Classes](#widget-base-classes)
- [CSS Custom Properties Theming](#css-custom-properties-theming)
- [Widget File Structure](#widget-file-structure)
- [Template & Style Loading](#template--style-loading)
- [Component Registration System](#component-registration-system)

## Web Component Architecture

### **1. Native Web Components with Shadow DOM**
```typescript
class SessionCostsWidget extends HTMLElement {
  private shadow: ShadowRoot;
  
  constructor() {
    super();
    this.shadow = this.attachShadow({ mode: 'open' });
  }
  
  connectedCallback() {
    this.render();
  }
}

customElements.define('session-costs-widget', SessionCostsWidget);
```

### **2. True Style Encapsulation**
```css
/* Inside Shadow DOM - completely isolated */
:host {
  display: block;
  background: var(--surface-color);
  border-radius: 8px;
}

/* No CSS conflicts possible - Shadow DOM boundary */
.widget-header { /* only affects this component */ }
.status-badge { /* completely scoped */ }
```

### **3. CSS Custom Properties Inheritance**
```css
/* Global theme (document level) */
:root {
  --theme-primary: #007acc;
  --theme-surface: #1e1e1e;
}

/* Shadow DOM inherits custom properties automatically */
:host {
  background: var(--theme-surface); /* Inherits from document */
  color: var(--theme-on-surface);
}
```

## Shadow DOM CSS System

### **Perfect Style Isolation**
```typescript
// Each widget has completely isolated styles
class WidgetBase extends HTMLElement {
  protected shadow: ShadowRoot;
  
  constructor() {
    super();
    this.shadow = this.attachShadow({ mode: 'open' });
  }
  
  protected render() {
    this.shadow.innerHTML = `
      <style>
        /* These styles ONLY affect this component */
        /* No global CSS pollution possible */
        :host {
          display: block;
          font-family: var(--theme-font-family);
        }
        
        .widget-content {
          padding: 16px;
          background: var(--theme-surface);
        }
      </style>
      
      <div class="widget-content">
        ${this.renderContent()}
      </div>
    `;
  }
}
```

### **CSS Custom Properties Theming**
```css
/* Global theme variables (inherit into Shadow DOM) */
html {
  --theme-primary: #007acc;
  --theme-surface: #1e1e1e;
  --theme-on-surface: #e2e8f0;
  --theme-surface-variant: #2d3748;
  --theme-success: #48bb78;
  --theme-spacing: 8px;
  --theme-border-radius: 8px;
  --theme-font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}

/* Dark theme toggle */
[data-theme="dark"] {
  --theme-surface: #1a202c;
  --theme-surface-variant: #2d3748;
}

[data-theme="light"] {
  --theme-surface: #ffffff;
  --theme-surface-variant: #f7fafc;
}
```

## Modular CSS Architecture

### **Base Widget Classes**
```typescript
// widgets/shared/WidgetBase.ts
export abstract class WidgetBase extends HTMLElement {
  protected shadow: ShadowRoot;
  protected state: any = {};
  
  constructor() {
    super();
    this.shadow = this.attachShadow({ mode: 'open' });
  }
  
  connectedCallback() {
    this.loadAndRender();
  }
  
  protected async loadAndRender() {
    const css = await this.loadCSS();
    const html = await this.loadTemplate();
    
    this.shadow.innerHTML = `
      <style>${css}</style>
      ${html}
    `;
    
    this.bindEvents();
    this.bindData();
  }
  
  // Load CSS from public directory
  protected async loadCSS(): Promise<string> {
    const widgetName = this.constructor.name.toLowerCase().replace('widget', '');
    try {
      const response = await fetch(`/widgets/${widgetName}/public/${widgetName}.css`);
      return await response.text();
    } catch {
      return this.getFallbackCSS();
    }
  }
  
  // Load HTML template from public directory  
  protected async loadTemplate(): Promise<string> {
    const widgetName = this.constructor.name.toLowerCase().replace('widget', '');
    try {
      const response = await fetch(`/widgets/${widgetName}/public/${widgetName}.html`);
      return await response.text();
    } catch {
      return this.getFallbackHTML();
    }
  }
  
  // Fallbacks for simple widgets
  protected getFallbackCSS(): string { return ''; }
  protected getFallbackHTML(): string { return '<div>Widget loading...</div>'; }
  
  // Abstract methods
  abstract bindEvents(): void;
  abstract bindData(): void;
}
```

### **Sidebar Widget Base**
```typescript
// widgets/shared/SidebarWidget.ts  
export abstract class SidebarWidget extends WidgetBase {
  protected collapsed = false;
  
  protected getFallbackCSS(): string {
    return `
      :host {
        display: block;
        background: var(--theme-surface);
        border-radius: var(--theme-border-radius);
        padding: var(--theme-spacing);
        margin-bottom: var(--theme-spacing);
        color: var(--theme-on-surface);
      }
      
      .widget-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        cursor: pointer;
        user-select: none;
        margin-bottom: var(--theme-spacing);
      }
      
      .widget-title {
        font-weight: 600;
        font-size: 14px;
      }
      
      .collapse-icon {
        transition: transform 0.2s ease;
      }
      
      :host(.collapsed) .collapse-icon {
        transform: rotate(-90deg);
      }
      
      .widget-content {
        transition: max-height 0.2s ease;
        overflow: hidden;
      }
      
      :host(.collapsed) .widget-content {
        max-height: 0;
      }
    `;
  }
  
  bindEvents() {
    const header = this.shadow.querySelector('.widget-header');
    header?.addEventListener('click', () => this.toggle());
  }
  
  private toggle() {
    this.collapsed = !this.collapsed;
    this.classList.toggle('collapsed', this.collapsed);
  }
}
```

## Widget File Structure

### **Complete Web Component Directory**
```
widgets/session-costs/
├── package.json
├── manifest.json
├── shared/
│   ├── SessionCostsWidget.ts     # Web Component class
│   └── SessionCostsTypes.ts      # TypeScript types
├── public/
│   ├── session-costs.html        # HTML template
│   ├── session-costs.css         # Scoped CSS
│   └── assets/
│       └── icons/
└── test/
    └── SessionCostsWidget.test.ts
```

### **HTML Template (Loaded into Shadow DOM)**
```html
<!-- widgets/session-costs/public/session-costs.html -->
<div class="widget-header">
  <div class="widget-title">
    <span class="title-icon">💰</span>
    <span class="title-text">SESSION COSTS</span>
  </div>
  <span class="status-badge" data-status="active">ACTIVE</span>
</div>

<div class="metrics-container">
  <div class="metric-row">
    <span class="metric-label">Requests</span>
    <span class="metric-value" data-bind="requests">47</span>
  </div>
  <div class="metric-row">
    <span class="metric-label">Cost</span>
    <span class="metric-value cost" data-bind="cost">$0.0000</span>
  </div>
</div>

<div class="action-buttons">
  <button class="btn" data-action="refresh">
    <span class="btn-icon">🔄</span>
    <span class="btn-text">Refresh</span>
  </button>
  <button class="btn" data-action="export">
    <span class="btn-icon">📊</span>
    <span class="btn-text">Export</span>
  </button>
</div>
```

### **Scoped CSS (Shadow DOM Isolated)**
```css
/* widgets/session-costs/public/session-costs.css */
:host {
  display: block;
  background: var(--theme-surface);
  border-radius: var(--theme-border-radius);
  padding: calc(var(--theme-spacing) * 2);
  margin-bottom: var(--theme-spacing);
  color: var(--theme-on-surface);
  font-family: var(--theme-font-family);
}

.widget-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: calc(var(--theme-spacing) * 2);
}

.widget-title {
  display: flex;
  align-items: center;
  font-weight: 600;
  font-size: 14px;
}

.title-icon {
  margin-right: calc(var(--theme-spacing) / 2);
}

.status-badge {
  background: var(--theme-success);
  color: white;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
}

.status-badge[data-status="inactive"] {
  background: var(--theme-warning);
}

.metrics-container {
  margin-bottom: calc(var(--theme-spacing) * 2);
}

.metric-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--theme-spacing);
}

.metric-label {
  color: var(--theme-on-surface-variant, rgba(255,255,255,0.7));
  font-size: 14px;
}

.metric-value {
  font-weight: 600;
  font-size: 16px;
}

.metric-value.cost {
  color: var(--theme-success);
}

.action-buttons {
  display: flex;
  gap: var(--theme-spacing);
}

.btn {
  flex: 1;
  background: var(--theme-surface-variant);
  border: none;
  border-radius: 6px;
  padding: 8px 12px;
  color: var(--theme-on-surface);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  font-size: 12px;
  transition: background 0.2s ease;
}

.btn:hover {
  background: var(--theme-primary);
  color: var(--theme-on-primary);
}
```

## Theme-Based Design

### **CSS Custom Properties System**
```css
/* theme/variables.css */
:root {
  /* Material Design 3 inspired color system */
  --theme-primary: #007acc;
  --theme-on-primary: #ffffff;
  --theme-surface: #1e1e1e;
  --theme-surface-variant: #2d3748;
  --theme-on-surface: #e2e8f0;
  --theme-on-surface-variant: #a0aec0;
  
  /* Semantic colors */
  --theme-success: #48bb78;
  --theme-warning: #ed8936;
  --theme-error: #f56565;
  --theme-info: #4299e1;
  
  /* Typography */
  --theme-font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  --theme-font-size-sm: 12px;
  --theme-font-size-base: 14px;
  --theme-font-size-lg: 16px;
  
  /* Spacing system */
  --theme-space-xs: 4px;
  --theme-space-sm: 8px;
  --theme-space-md: 16px;
  --theme-space-lg: 24px;
  --theme-space-xl: 32px;
}

/* Dark theme overrides */
[data-theme="dark"] {
  --theme-surface: #1a202c;
  --theme-surface-variant: #2d3748;
  --theme-on-surface: #e2e8f0;
}

/* Light theme overrides */  
[data-theme="light"] {
  --theme-surface: #ffffff;
  --theme-surface-variant: #f7fafc;
  --theme-on-surface: #2d3748;
}
```

## Template System

### **HTML Template Files**
```html
<!-- widgets/session-costs/public/session-costs.html -->
<div class="session-costs-widget">
  <div class="widget-header">
    <div class="widget-title">
      <span class="title-icon">💰</span>
      <span class="title-text">SESSION COSTS</span>
    </div>
    <span class="status-badge" data-status="{{status}}">{{statusText}}</span>
  </div>
  
  <div class="metrics-container">
    <div class="metric-row">
      <span class="metric-label">Requests</span>
      <span class="metric-value" data-bind="requests">{{requests}}</span>
    </div>
    <div class="metric-row">
      <span class="metric-label">Cost</span>
      <span class="metric-value cost" data-bind="cost">{{costFormatted}}</span>
    </div>
  </div>
  
  <div class="action-buttons">
    <button class="btn btn-secondary" data-action="refresh">
      <span class="btn-icon">🔄</span>
      <span class="btn-text">Refresh</span>
    </button>
    <button class="btn btn-secondary" data-action="export">
      <span class="btn-icon">📊</span>
      <span class="btn-text">Export</span>
    </button>
  </div>
</div>
```

## Widget File Structure

### **Complete Widget Directory Example**
```
widgets/session-costs/
├── package.json                    # Widget metadata
├── manifest.json                   # Widget registration
├── shared/
│   ├── SessionCostsWidget.ts      # TypeScript logic
│   └── SessionCostsTypes.ts       # Type definitions
├── public/
│   ├── session-costs.html         # HTML template
│   ├── session-costs.css          # Widget-specific styles
│   ├── session-costs.scss         # SCSS source (compiles to CSS)
│   └── assets/
│       └── icons/
└── test/
    └── SessionCostsWidget.test.ts
```

### **SCSS Build Pipeline**
```scss
// widgets/session-costs/public/session-costs.scss
@import '../../../shared/public/sidebar-widget';

.session-costs-widget {
  @extend .sidebar-widget;
  
  .status-badge {
    background: var(--theme-success);
    color: var(--theme-on-primary);
    padding: var(--theme-space-xs) var(--theme-space-sm);
    border-radius: 4px;
    font-size: var(--theme-font-size-sm);
    font-weight: 600;
    
    &[data-status="inactive"] {
      background: var(--theme-warning);
    }
    
    &[data-status="error"] {
      background: var(--theme-error);
    }
  }
  
  .metrics-container {
    margin-bottom: var(--theme-space-md);
  }
  
  .metric-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--theme-space-sm);
    
    .metric-label {
      color: var(--theme-on-surface-variant);
      font-size: var(--theme-font-size-base);
    }
    
    .metric-value {
      font-weight: 600;
      font-size: var(--theme-font-size-lg);
      
      &.cost {
        color: var(--theme-success);
      }
    }
  }
  
  .action-buttons {
    display: flex;
    gap: var(--theme-space-sm);
  }
}
```

## CSS Inheritance Pattern

### **Widget-Specific TypeScript (Pure Logic)**
```typescript
// widgets/session-costs/shared/SessionCostsWidget.ts
export class SessionCostsWidget extends SidebarWidget {
  static get widgetName() { return 'session-costs'; }
  
  protected state = {
    requests: 47,
    cost: 0.0000,
    status: 'active'
  };

  // Pure logic - no styling
  async connectedCallback() {
    await this.loadTemplate('session-costs.html');
    await this.loadStyles('session-costs.css');
    this.bindData();
    this.setupEventHandlers();
  }
  
  private bindData() {
    // Data binding to template
    this.updateElement('[data-bind="requests"]', this.state.requests);
    this.updateElement('[data-bind="cost"]', `$${this.state.cost.toFixed(4)}`);
    this.updateAttribute('[data-status]', 'data-status', this.state.status);
  }
  
  protected onAction(action: string) {
    switch (action) {
      case 'refresh':
        this.executeCommand('session:refresh-costs');
        break;
      case 'export':
        this.executeCommand('session:export-costs');
        break;
    }
  }
}
```

## Theme Customization

### **Dynamic Theme Switching**
```typescript
// system/theme/ThemeManager.ts
export class ThemeManager {
  static setTheme(themeName: 'dark' | 'light' | 'auto') {
    document.documentElement.setAttribute('data-theme', themeName);
    this.loadThemeCSS(themeName);
  }
  
  static async loadThemeCSS(themeName: string) {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = `/theme/${themeName}-theme.css`;
    document.head.appendChild(link);
  }
  
  static customizeTheme(customProperties: Record<string, string>) {
    const root = document.documentElement;
    Object.entries(customProperties).forEach(([property, value]) => {
      root.style.setProperty(property, value);
    });
  }
}
```

This modular approach provides proper separation, inheritance, and theme customization while maintaining clean abstractions at every level.

### **Widget Base Classes**

#### **WidgetBase (Foundation)**
```typescript
// widgets/shared/WidgetBase.ts
export abstract class WidgetBase extends HTMLElement {
  protected shadowRoot: ShadowRoot;
  protected state: any = {};
  protected context: WidgetContext;

  // Abstract methods - must implement
  static get widgetName(): string;
  abstract render(): string | Promise<string>;

  // Lifecycle methods (React-like)
  connectedCallback() { this.mount(); }
  disconnectedCallback() { this.cleanup(); }
  attributeChangedCallback() { this.onPropsChanged(); }

  // State management (React-like)
  protected setState(newState: Partial<any>) {
    const prevState = { ...this.state };
    this.state = { ...this.state, ...newState };
    this.onStateChanged(prevState, this.state);
    this.rerender();
  }

  // Rendering system
  private async rerender() {
    const content = await this.render();
    if (typeof content === 'string') {
      this.shadowRoot.innerHTML = content;
    }
  }

  // Static asset loading
  protected async loadFromPublic(): Promise<void> {
    const widgetName = this.constructor.widgetName;
    const publicPath = `/widgets/${widgetName}/public/`;
    
    // Load HTML template if exists
    try {
      const html = await fetch(`${publicPath}${widgetName}.html`);
      if (html.ok) this.templateHTML = await html.text();
    } catch {}
    
    // Load CSS if exists
    try {
      const css = await fetch(`${publicPath}${widgetName}.css`);
      if (css.ok) this.templateCSS = await css.text();
    } catch {}
  }
}
```

#### **ContentWidget (Main Panel Base)**
```typescript
// widgets/shared/ContentWidget.ts
export abstract class ContentWidget extends WidgetBase {
  protected contentType: 'chat' | 'code' | 'web' | 'academy';
  
  // Content widgets manage tabs
  protected tabInfo = {
    title: '',
    icon: '',
    modified: false,
    closeable: true
  };

  // Content-specific event handling
  protected onContentFocus() { this.emit('content:focused', this.tabInfo); }
  protected onContentBlur() { this.emit('content:blurred', this.tabInfo); }
  
  // Tab management
  protected updateTab(info: Partial<TabInfo>) {
    this.tabInfo = { ...this.tabInfo, ...info };
    this.emit('tab:updated', this.tabInfo);
  }
}
```

#### **SidebarWidget (Sidebar Base)**
```typescript
// widgets/shared/SidebarWidget.ts
export abstract class SidebarWidget extends WidgetBase {
  protected collapsible = true;
  protected collapsed = false;
  protected contexts: string[] = ['general']; // Which contexts show this widget
  
  // Sidebar-specific rendering
  render(): string {
    return `
      <div class="sidebar-widget ${this.collapsed ? 'collapsed' : ''}">
        <div class="widget-header" onclick="this.toggleCollapse()">
          <span class="widget-title">${this.getTitle()}</span>
          <span class="collapse-icon">${this.collapsed ? '▶' : '▼'}</span>
        </div>
        <div class="widget-content ${this.collapsed ? 'hidden' : 'visible'}">
          ${this.renderContent()}
        </div>
      </div>
    `;
  }
  
  // Sidebar widgets filter by context
  shouldShowInContext(context: string): boolean {
    return this.contexts.includes(context) || this.contexts.includes('*');
  }
  
  abstract getTitle(): string;
  abstract renderContent(): string;
}
```

## Content Widget Patterns

### **ChatWidget (General Chat)**
```typescript
// widgets/chat/shared/ChatWidget.ts
export class ChatWidget extends ContentWidget {
  static get widgetName() { return 'chat'; }
  
  protected state = {
    messages: [],
    currentRoom: 'general',
    typingUsers: [],
    scrollPosition: 'bottom'
  };

  constructor() {
    super();
    this.contentType = 'chat';
    this.tabInfo = {
      title: 'General Chat',
      icon: '💬',
      modified: false,
      closeable: false
    };
  }

  render(): string {
    return `
      <style>
        ${this.getChatStyles()}
      </style>
      
      <div class="chat-container">
        <div class="chat-header">
          <h3>${this.state.currentRoom} Chat</h3>
          <div class="chat-status">Connected</div>
        </div>
        
        <div class="messages-container" id="messages">
          ${this.renderMessages()}
        </div>
        
        <div class="typing-indicators">
          ${this.renderTypingIndicators()}
        </div>
        
        <div class="input-area">
          ${this.renderInputArea()}
        </div>
      </div>
    `;
  }

  private renderMessages(): string {
    return this.state.messages.map(msg => this.renderMessage(msg)).join('');
  }

  private renderMessage(message: ChatMessage): string {
    const isUser = message.sender === 'user';
    const timestamp = this.formatTimestamp(message.timestamp);
    
    return `
      <div class="message ${isUser ? 'user-message' : 'ai-message'}" data-id="${message.id}">
        <div class="message-header">
          <div class="sender-info">
            <img class="avatar" src="${message.senderAvatar}" alt="${message.sender}">
            <span class="sender-name">${message.senderDisplay}</span>
          </div>
          <div class="message-time">${timestamp}</div>
        </div>
        
        <div class="message-content">
          ${this.renderMessageContent(message)}
        </div>
        
        <div class="message-actions">
          ${this.renderMessageActions(message)}
        </div>
      </div>
    `;
  }

  private renderMessageContent(message: ChatMessage): string {
    switch (message.type) {
      case 'text':
        return `<p>${this.escapeHTML(message.content)}</p>`;
      case 'code':
        return `<pre><code class="language-${message.language}">${message.content}</code></pre>`;
      case 'image':
        return `<img src="${message.content}" alt="User image" class="message-image">`;
      case 'file':
        return `<div class="file-attachment">${message.filename}</div>`;
      default:
        return message.content;
    }
  }
}
```

### **AcademyWidget (Training Chat)**
```typescript
// widgets/academy/shared/AcademyWidget.ts
export class AcademyWidget extends ChatWidget {
  static get widgetName() { return 'academy'; }
  
  protected state = {
    ...super.state,
    currentPersona: 'claude-code',
    trainingMode: true,
    loraSettings: {},
    thresholds: {}
  };

  constructor() {
    super();
    this.tabInfo = {
      title: 'Academy Training',
      icon: '🎓',
      modified: false,
      closeable: true
    };
  }

  render(): string {
    // Extend base chat with training features
    const baseRender = super.render();
    
    return baseRender.replace(
      '<div class="chat-header">',
      `<div class="chat-header academy-header">
        <div class="training-status">
          <span class="persona-indicator">Training: ${this.state.currentPersona}</span>
          <span class="mode-indicator ${this.state.trainingMode ? 'active' : 'inactive'}">
            ${this.state.trainingMode ? 'Training Mode' : 'Chat Mode'}
          </span>
        </div>`
    );
  }

  // Academy-specific message rendering
  protected renderMessage(message: ChatMessage): string {
    const baseMessage = super.renderMessage(message);
    
    // Add training-specific elements
    if (message.trainingData) {
      const trainingInfo = `
        <div class="training-metadata">
          <span class="confidence">Confidence: ${message.trainingData.confidence}%</span>
          <span class="improvement">Improvement: ${message.trainingData.improvement}</span>
        </div>
      `;
      return baseMessage.replace('</div>', trainingInfo + '</div>');
    }
    
    return baseMessage;
  }
}
```

## Sidebar Widget Patterns

### **UsersAgentsWidget**
```typescript
// widgets/users-agents/shared/UsersAgentsWidget.ts
export class UsersAgentsWidget extends SidebarWidget {
  static get widgetName() { return 'users-agents'; }
  
  protected contexts = ['general', 'academy']; // Show in both contexts
  protected state = {
    entities: [], // Users, AIs, personas - all same base type
    filter: 'all',
    sortBy: 'activity'
  };

  getTitle(): string {
    return 'USERS & AGENTS';
  }

  renderContent(): string {
    return `
      <div class="search-container">
        <input type="text" placeholder="Search agents..." class="search-input">
      </div>
      
      <div class="entities-list">
        ${this.state.entities.map(entity => this.renderEntity(entity)).join('')}
      </div>
      
      <div class="entity-actions">
        <button class="add-agent-btn">+ Add Agent</button>
        <button class="manage-btn">⚙ Manage</button>
      </div>
    `;
  }

  private renderEntity(entity: UserAgentEntity): string {
    const statusColor = this.getStatusColor(entity.status);
    const entityIcon = this.getEntityIcon(entity.type);
    
    return `
      <div class="entity-card ${entity.type}" data-id="${entity.id}">
        <div class="entity-header">
          <div class="entity-avatar">
            <img src="${entity.avatar}" alt="${entity.name}">
            <span class="entity-icon">${entityIcon}</span>
          </div>
          <div class="entity-info">
            <div class="entity-name">${entity.displayName}</div>
            <div class="entity-subtitle">${entity.subtitle}</div>
          </div>
          <div class="entity-status">
            <span class="status-dot" style="background: ${statusColor}"></span>
            <span class="entity-actions">
              <button class="star-btn ${entity.starred ? 'starred' : ''}">⭐</button>
              <button class="more-btn">⋯</button>
            </span>
          </div>
        </div>
        
        <div class="entity-metrics">
          <span class="metric">${entity.metrics.primary}</span>
          <span class="last-active">Last active: ${entity.lastActive}</span>
        </div>
      </div>
    `;
  }

  private getEntityIcon(type: string): string {
    const icons = {
      'ai': '🤖',
      'user': '👤', 
      'persona': '🎭'
    };
    return icons[type] || '👤';
  }
}
```

### **SessionCostsWidget (Matches Screenshot Exactly)**
```typescript
// widgets/session-costs/shared/SessionCostsWidget.ts
export class SessionCostsWidget extends SidebarWidget {
  static get widgetName() { return 'session-costs'; }
  
  protected contexts = ['*']; // Show in all contexts
  protected state = {
    requests: 47,
    cost: 0.0000,
    status: 'active'
  };

  getTitle(): string {
    return 'SESSION COSTS';
  }

  renderContent(): string {
    return `
      <style>
        :host {
          display: block;
          background: #2d3748;
          border-radius: 8px;
          padding: 16px;
          margin-bottom: 16px;
        }
        
        .session-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 16px;
        }
        
        .session-title {
          display: flex;
          align-items: center;
          color: #e2e8f0;
          font-weight: 600;
          font-size: 14px;
        }
        
        .money-icon {
          margin-right: 8px;
          font-size: 16px;
        }
        
        .status-badge {
          background: #48bb78;
          color: white;
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 11px;
          font-weight: 600;
        }
        
        .metrics-container {
          margin-bottom: 16px;
        }
        
        .metric-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 12px;
        }
        
        .metric-label {
          color: #a0aec0;
          font-size: 14px;
        }
        
        .metric-value {
          color: #e2e8f0;
          font-weight: 600;
          font-size: 16px;
        }
        
        .metric-value.cost {
          color: #48bb78;
        }
        
        .action-buttons {
          display: flex;
          gap: 8px;
        }
        
        .action-btn {
          flex: 1;
          background: #4a5568;
          border: none;
          border-radius: 6px;
          padding: 8px 12px;
          color: #e2e8f0;
          font-size: 12px;
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 4px;
        }
        
        .action-btn:hover {
          background: #5a6578;
        }
      </style>
      
      <div class="session-header">
        <div class="session-title">
          <span class="money-icon">💰</span>
          SESSION COSTS
        </div>
        <span class="status-badge">ACTIVE</span>
      </div>
      
      <div class="metrics-container">
        <div class="metric-row">
          <span class="metric-label">Requests</span>
          <span class="metric-value">${this.state.requests}</span>
        </div>
        <div class="metric-row">
          <span class="metric-label">Cost</span>
          <span class="metric-value cost">$${this.state.cost.toFixed(4)}</span>
        </div>
      </div>
      
      <div class="action-buttons">
        <button class="action-btn refresh-btn" onclick="this.refreshCosts()">
          🔄 Refresh
        </button>
        <button class="action-btn export-btn" onclick="this.exportCosts()">
          📊 Export
        </button>
      </div>
    `;
  }

  private refreshCosts() {
    this.executeCommand('session:refresh-costs');
  }

  private exportCosts() {
    this.executeCommand('session:export-costs');
  }
}
```

### **AcademyControlsWidget**
```typescript
// widgets/academy-controls/shared/AcademyControlsWidget.ts
export class AcademyControlsWidget extends SidebarWidget {
  static get widgetName() { return 'academy-controls'; }
  
  protected contexts = ['academy']; // Only show in academy context
  protected state = {
    currentPersona: 'claude-code',
    trainingMode: true,
    loraSettings: {
      alpha: 0.8,
      rank: 16
    },
    thresholds: {
      confidence: 0.75,
      improvement: 0.1
    }
  };

  getTitle(): string {
    return 'ACADEMY CONTROLS';
  }

  renderContent(): string {
    return `
      <div class="persona-section">
        <h4>Active Persona</h4>
        <select class="persona-select">
          <option value="claude-code" ${this.state.currentPersona === 'claude-code' ? 'selected' : ''}>
            Claude Code
          </option>
          <option value="custom">Custom Persona</option>
        </select>
      </div>
      
      <div class="lora-section">
        <h4>LoRA Settings</h4>
        <div class="slider-control">
          <label>Alpha: ${this.state.loraSettings.alpha}</label>
          <input type="range" min="0" max="1" step="0.1" value="${this.state.loraSettings.alpha}">
        </div>
        <div class="slider-control">
          <label>Rank: ${this.state.loraSettings.rank}</label>
          <input type="range" min="1" max="64" value="${this.state.loraSettings.rank}">
        </div>
      </div>
      
      <div class="threshold-section">
        <h4>Training Thresholds</h4>
        <div class="slider-control">
          <label>Confidence: ${this.state.thresholds.confidence}</label>
          <input type="range" min="0" max="1" step="0.05" value="${this.state.thresholds.confidence}">
        </div>
      </div>
      
      <div class="training-actions">
        <button class="start-training-btn ${this.state.trainingMode ? 'active' : ''}">
          ${this.state.trainingMode ? 'Stop Training' : 'Start Training'}
        </button>
        <button class="save-settings-btn">💾 Save Settings</button>
      </div>
    `;
  }
}
```

## Dynamic Rendering System

### **Render Strategy Selection**
```typescript
// widgets/shared/RenderStrategy.ts
export class RenderStrategy {
  static selectStrategy(widget: WidgetBase, content: any): 'template' | 'direct' | 'hybrid' {
    // Large content → use template files
    if (content.length > 1000 || content.includes('<style>')) {
      return 'template';
    }
    
    // Simple content → direct render
    if (content.length < 100) {
      return 'direct';
    }
    
    // Mixed content → hybrid approach
    return 'hybrid';
  }
  
  static async applyStrategy(widget: WidgetBase, strategy: string): Promise<string> {
    switch (strategy) {
      case 'template':
        return await widget.loadFromPublic();
      case 'direct':
        return widget.render();
      case 'hybrid':
        const template = await widget.loadFromPublic();
        const dynamic = await widget.render();
        return widget.mergeTemplateAndDynamic(template, dynamic);
    }
  }
}
```

## Message UX Design

### **iMessage/Discord-Style Message System**
```typescript
// Message display patterns
interface MessageDisplayPattern {
  // Grouping messages from same sender
  groupConsecutive: boolean; // Like iMessage
  
  // Timestamp display
  timestampStrategy: 'hover' | 'always' | 'smart'; // Smart = show on time gaps
  
  // Status indicators
  showTyping: boolean;
  showDelivered: boolean;
  showRead: boolean;
  
  // Rich content support
  supportedTypes: ['text', 'image', 'video', 'file', 'code', 'ai-response'];
  
  // Per-message actions
  actions: ['react', 'reply', 'copy', 'delete', 'edit'];
}
```

### **Message Rendering Patterns**
```typescript
class MessageRenderer {
  renderMessageGroup(messages: ChatMessage[]): string {
    const sender = messages[0].sender;
    const timeGap = this.hasTimeGap(messages[0]);
    
    return `
      <div class="message-group ${sender}-messages">
        ${timeGap ? `<div class="time-separator">${this.formatTime(messages[0].timestamp)}</div>` : ''}
        
        <div class="sender-info">
          <img class="sender-avatar" src="${messages[0].senderAvatar}">
          <span class="sender-name">${messages[0].senderDisplay}</span>
          <span class="message-timestamp">${this.formatTimestamp(messages[0].timestamp)}</span>
        </div>
        
        <div class="message-list">
          ${messages.map(msg => this.renderSingleMessage(msg, false)).join('')}
        </div>
      </div>
    `;
  }
  
  renderTypingIndicator(users: string[]): string {
    if (users.length === 0) return '';
    
    const userText = users.length === 1 
      ? `${users[0]} is typing` 
      : `${users.slice(0, -1).join(', ')} and ${users[users.length - 1]} are typing`;
    
    return `
      <div class="typing-indicator">
        <div class="typing-dots">
          <span></span><span></span><span></span>
        </div>
        <span class="typing-text">${userText}...</span>
      </div>
    `;
  }
}
```

## Context-Driven Widget Loading

### **Context Manager**
```typescript
// system/context/ContextManager.ts
export class ContextManager {
  private contexts: Record<string, ContextDefinition> = {
    general: {
      contentWidgets: ['chat'],
      sidebarWidgets: ['session-costs', 'users-agents', 'current-projects'],
      defaultContent: 'chat'
    },
    
    academy: {
      contentWidgets: ['academy', 'chat'],
      sidebarWidgets: ['session-costs', 'users-agents', 'academy-controls', 'current-projects'],
      defaultContent: 'academy'
    }
  };

  async switchContext(contextName: string) {
    const context = this.contexts[contextName];
    
    // Load content widgets
    await this.loadContentWidgets(context.contentWidgets);
    
    // Filter and load sidebar widgets
    await this.loadSidebarWidgets(context.sidebarWidgets, contextName);
    
    // Set default active content
    await this.setActiveContent(context.defaultContent);
    
    // Emit context change event
    this.emit('context:changed', { from: this.currentContext, to: contextName });
  }
  
  private async loadSidebarWidgets(widgets: string[], context: string) {
    const sidebarContainer = document.querySelector('.sidebar-panel');
    sidebarContainer.innerHTML = ''; // Clear current widgets
    
    for (const widgetName of widgets) {
      const widgetElement = document.createElement(`${widgetName}-widget`);
      
      // Only add if widget should show in this context
      const widget = customElements.get(`${widgetName}-widget`);
      if (widget && widget.prototype.shouldShowInContext(context)) {
        sidebarContainer.appendChild(widgetElement);
      }
    }
  }
}
```

This comprehensive class structure provides the foundation for building Discord/VSCode-style widgets with React-like patterns, intelligent rendering, and AI-native event handling.