# Widget Architecture Optimization - Intermediate Base Classes

## 🧅 Optimized Layer Structure with Intermediate Classes

### Layer 1: Core Foundation
```
core/
├── base-widget/           # Universal base
├── widget-system/         # Registration & discovery
└── data-display/          # Generic data patterns
```

### Layer 1.5: Intermediate Base Classes (New!)
```
intermediate/
├── real-time-widget/      # WebSocket + live updates
├── command-widget/        # Command execution patterns  
├── status-widget/         # Status display patterns
├── interactive-widget/    # User interaction patterns
└── data-bound-widget/     # Database integration patterns
```

### Layer 2: Domain-Specific (Optimized)
```
domain/
├── communication/         # Chat, messaging
│   ├── base/             # CommunicationWidget extends RealTimeWidget
│   └── chat/             # ChatWidget (ACTIVE)
├── intelligence/          # AI, Academy, personas
│   ├── base/             # IntelligenceWidget extends StatusWidget + DataBoundWidget  
│   ├── academy/          # AcademyWidget (ACTIVE)
│   └── persona/          # PersonaWidget (ACTIVE)
├── navigation/            # Sidebar, menus
│   ├── base/             # NavigationWidget extends InteractiveWidget
│   └── sidebar/          # SidebarWidget (ACTIVE)
└── system/               # Version, health, logs
    ├── base/             # SystemWidget extends StatusWidget
    ├── version/          # VersionWidget (ACTIVE)
    └── health/           # HealthWidget (DISABLED - until needed)
```

## 🔧 Intermediate Base Classes Design

### RealTimeWidget (Layer 1.5)
```typescript
/**
 * Intermediate: Adds WebSocket and real-time update capabilities
 * Used by: Communication widgets, Intelligence widgets, System monitoring
 */
export abstract class RealTimeWidget extends BaseWidget {
  protected updateInterval: number | null = null;
  protected websocketConnected: boolean = false;
  
  // Real-time update infrastructure
  protected startRealTimeUpdates(intervalMs: number = 5000): void {
    this.updateInterval = window.setInterval(() => {
      this.fetchUpdates();
    }, intervalMs);
  }
  
  protected stopRealTimeUpdates(): void {
    if (this.updateInterval) {
      clearInterval(this.updateInterval);
      this.updateInterval = null;
    }
  }
  
  // WebSocket connection handling
  protected connectWebSocket(endpoint: string): void {
    // Common WebSocket connection logic
  }
  
  protected onWebSocketMessage(handler: (data: any) => void): void {
    // Common message handling
  }
  
  // Abstract methods for subclasses
  protected abstract fetchUpdates(): Promise<void>;
  protected abstract handleRealtimeData(data: any): void;
  
  destroy(): void {
    this.stopRealTimeUpdates();
    super.destroy();
  }
}
```

### CommandWidget (Layer 1.5)
```typescript
/**
 * Intermediate: Adds enhanced command execution with error handling
 * Used by: All widgets that execute complex command workflows
 */
export abstract class CommandWidget extends BaseWidget {
  protected commandHistory: string[] = [];
  protected lastCommandResult: any = null;
  
  // Enhanced command execution with error handling
  protected async executeCommandSafely(command: string, params: any): Promise<{
    success: boolean;
    data?: any;
    error?: string;
  }> {
    try {
      this.commandHistory.push(`${command}(${JSON.stringify(params)})`);
      const result = await this.executeCommand(command, params);
      this.lastCommandResult = result;
      
      if (result.success) {
        this.onCommandSuccess(command, result.data);
        return { success: true, data: result.data };
      } else {
        this.onCommandError(command, result.error);
        return { success: false, error: result.error };
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.onCommandError(command, errorMsg);
      return { success: false, error: errorMsg };
    }
  }
  
  // Command workflow patterns
  protected async executeCommandSequence(commands: Array<{
    command: string;
    params: any;
    onSuccess?: (data: any) => void;
    onError?: (error: string) => void;
  }>): Promise<boolean> {
    for (const cmd of commands) {
      const result = await this.executeCommandSafely(cmd.command, cmd.params);
      if (result.success) {
        cmd.onSuccess?.(result.data);
      } else {
        cmd.onError?.(result.error || 'Unknown error');
        return false; // Stop sequence on first failure
      }
    }
    return true;
  }
  
  // Hooks for subclasses
  protected onCommandSuccess(command: string, data: any): void {
    this.log(`Command ${command} succeeded`, 'info');
  }
  
  protected onCommandError(command: string, error: string): void {
    this.log(`Command ${command} failed: ${error}`, 'error');
  }
  
  // Command history and debugging
  getCommandHistory(): string[] {
    return [...this.commandHistory];
  }
  
  getLastCommandResult(): any {
    return this.lastCommandResult;
  }
}
```

### StatusWidget (Layer 1.5)
```typescript
/**
 * Intermediate: Adds status display and health monitoring patterns
 * Used by: System widgets, Intelligence widgets, any widget showing status
 */
export abstract class StatusWidget extends DataDisplayWidget {
  protected currentStatus: WidgetStatus = 'loading';
  protected statusHistory: StatusEntry[] = [];
  
  // Status management
  protected setStatus(status: WidgetStatus, message?: string): void {
    const previousStatus = this.currentStatus;
    this.currentStatus = status;
    
    this.statusHistory.push({
      status,
      message,
      timestamp: new Date(),
      previousStatus
    });
    
    // Keep only last 50 status entries
    if (this.statusHistory.length > 50) {
      this.statusHistory = this.statusHistory.slice(-50);
    }
    
    this.onStatusChange(status, previousStatus);
    this.updateStatusDisplay();
  }
  
  // Status display patterns
  protected updateStatusDisplay(): void {
    if (!this.container) return;
    
    const statusIndicator = this.container.querySelector('.widget-status');
    if (statusIndicator) {
      statusIndicator.className = `widget-status status-${this.currentStatus}`;
      statusIndicator.textContent = this.getStatusText();
    }
  }
  
  protected getStatusText(): string {
    switch (this.currentStatus) {
      case 'healthy': return '●';
      case 'warning': return '⚠';
      case 'error': return '✕';
      case 'loading': return '⟳';
      default: return '?';
    }
  }
  
  protected getStatusColor(): string {
    switch (this.currentStatus) {
      case 'healthy': return '#22c55e';
      case 'warning': return '#f59e0b';
      case 'error': return '#ef4444';
      case 'loading': return '#6b7280';
      default: return '#9ca3af';
    }
  }
  
  // Health check infrastructure
  protected async performHealthCheck(): Promise<boolean> {
    try {
      const isHealthy = await this.checkHealth();
      this.setStatus(isHealthy ? 'healthy' : 'error');
      return isHealthy;
    } catch (error) {
      this.setStatus('error', error instanceof Error ? error.message : String(error));
      return false;
    }
  }
  
  // Abstract methods for subclasses
  protected abstract checkHealth(): Promise<boolean>;
  protected abstract onStatusChange(newStatus: WidgetStatus, oldStatus: WidgetStatus): void;
  
  // Status history and debugging
  getStatusHistory(): StatusEntry[] {
    return [...this.statusHistory];
  }
  
  getCurrentStatus(): WidgetStatus {
    return this.currentStatus;
  }
}

type WidgetStatus = 'healthy' | 'warning' | 'error' | 'loading' | 'disabled';

interface StatusEntry {
  status: WidgetStatus;
  message?: string;
  timestamp: Date;
  previousStatus: WidgetStatus;
}
```

### InteractiveWidget (Layer 1.5)
```typescript
/**
 * Intermediate: Adds user interaction patterns and event handling
 * Used by: Navigation widgets, control widgets, interactive displays
 */
export abstract class InteractiveWidget extends BaseWidget {
  protected interactions: Map<string, InteractionHandler> = new Map();
  protected keyboardShortcuts: Map<string, () => void> = new Map();
  
  // Interaction handling infrastructure
  protected registerInteraction(selector: string, event: string, handler: InteractionHandler): void {
    const key = `${selector}:${event}`;
    this.interactions.set(key, handler);
    
    // Auto-setup event listeners when widget renders
    this.setupInteractionListeners();
  }
  
  protected setupInteractionListeners(): void {
    if (!this.container) return;
    
    this.interactions.forEach((handler, key) => {
      const [selector, event] = key.split(':');
      const elements = this.container!.querySelectorAll(selector);
      
      elements.forEach(element => {
        element.addEventListener(event, (e) => {
          handler(e as any, element);
        });
      });
    });
  }
  
  // Keyboard shortcut handling
  protected registerKeyboardShortcut(keys: string, handler: () => void): void {
    this.keyboardShortcuts.set(keys, handler);
    
    // Global keyboard listener for this widget
    document.addEventListener('keydown', (e) => {
      if (!this.isWidgetFocused()) return;
      
      const keyCombo = this.getKeyCombo(e);
      const handler = this.keyboardShortcuts.get(keyCombo);
      if (handler) {
        e.preventDefault();
        handler();
      }
    });
  }
  
  private getKeyCombo(e: KeyboardEvent): string {
    const keys: string[] = [];
    if (e.ctrlKey) keys.push('ctrl');
    if (e.altKey) keys.push('alt');
    if (e.shiftKey) keys.push('shift');
    if (e.metaKey) keys.push('meta');
    keys.push(e.key.toLowerCase());
    return keys.join('+');
  }
  
  private isWidgetFocused(): boolean {
    return this.container?.contains(document.activeElement) || false;
  }
  
  // User feedback patterns
  protected showUserFeedback(message: string, type: 'success' | 'error' | 'info' = 'info'): void {
    // Create temporary feedback display
    const feedback = document.createElement('div');
    feedback.className = `user-feedback feedback-${type}`;
    feedback.textContent = message;
    feedback.style.cssText = `
      position: absolute;
      top: 10px;
      right: 10px;
      padding: 8px 12px;
      border-radius: 4px;
      background: ${type === 'success' ? '#22c55e' : type === 'error' ? '#ef4444' : '#3b82f6'};
      color: white;
      font-size: 12px;
      z-index: 1000;
      opacity: 0;
      transition: opacity 0.3s ease;
    `;
    
    this.container?.appendChild(feedback);
    
    // Animate in
    setTimeout(() => feedback.style.opacity = '1', 10);
    
    // Remove after 3 seconds
    setTimeout(() => {
      feedback.style.opacity = '0';
      setTimeout(() => feedback.remove(), 300);
    }, 3000);
  }
  
  // Form handling patterns
  protected collectFormData(formSelector: string): Record<string, any> {
    const form = this.container?.querySelector(formSelector) as HTMLFormElement;
    if (!form) return {};
    
    const formData = new FormData(form);
    const data: Record<string, any> = {};
    
    formData.forEach((value, key) => {
      data[key] = value;
    });
    
    return data;
  }
  
  protected validateForm(formSelector: string, rules: ValidationRules): ValidationResult {
    const data = this.collectFormData(formSelector);
    const errors: string[] = [];
    
    Object.entries(rules).forEach(([field, rule]) => {
      const value = data[field];
      
      if (rule.required && (!value || value.toString().trim() === '')) {
        errors.push(`${field} is required`);
      }
      
      if (rule.minLength && value && value.toString().length < rule.minLength) {
        errors.push(`${field} must be at least ${rule.minLength} characters`);
      }
      
      if (rule.pattern && value && !rule.pattern.test(value.toString())) {
        errors.push(`${field} format is invalid`);
      }
    });
    
    return { valid: errors.length === 0, errors };
  }
}

type InteractionHandler = (event: Event, element: Element) => void;

interface ValidationRules {
  [field: string]: {
    required?: boolean;
    minLength?: number;
    pattern?: RegExp;
  };
}

interface ValidationResult {
  valid: boolean;
  errors: string[];
}
```

### DataBoundWidget (Layer 1.5)
```typescript
/**
 * Intermediate: Adds database integration and data synchronization
 * Used by: Widgets that display and manage persistent data
 */
export abstract class DataBoundWidget extends StatusWidget {
  protected dataCache: Map<string, CachedData> = new Map();
  protected autoRefreshInterval: number | null = null;
  
  // Data loading and caching
  protected async loadData<T>(key: string, loader: () => Promise<T>, ttlMs: number = 30000): Promise<T> {
    const cached = this.dataCache.get(key);
    
    if (cached && Date.now() - cached.timestamp < ttlMs) {
      return cached.data as T;
    }
    
    try {
      this.setStatus('loading', `Loading ${key}...`);
      const data = await loader();
      
      this.dataCache.set(key, {
        data,
        timestamp: Date.now(),
        ttl: ttlMs
      });
      
      this.setStatus('healthy');
      this.onDataLoaded(key, data);
      return data;
    } catch (error) {
      this.setStatus('error', `Failed to load ${key}`);
      this.onDataError(key, error);
      throw error;
    }
  }
  
  // Auto-refresh infrastructure
  protected startAutoRefresh(intervalMs: number = 60000): void {
    this.autoRefreshInterval = window.setInterval(() => {
      this.refreshAllData();
    }, intervalMs);
  }
  
  protected stopAutoRefresh(): void {
    if (this.autoRefreshInterval) {
      clearInterval(this.autoRefreshInterval);
      this.autoRefreshInterval = null;
    }
  }
  
  private async refreshAllData(): Promise<void> {
    const refreshPromises = Array.from(this.dataCache.keys()).map(key => {
      return this.refreshData(key);
    });
    
    await Promise.allSettled(refreshPromises);
  }
  
  protected async refreshData(key: string): Promise<void> {
    const cached = this.dataCache.get(key);
    if (!cached) return;
    
    // Remove from cache to force reload
    this.dataCache.delete(key);
    
    // Trigger refresh through data binding
    this.onDataRefreshNeeded(key);
  }
  
  // Data synchronization patterns
  protected async saveData<T>(key: string, data: T, saver: (data: T) => Promise<void>): Promise<void> {
    try {
      this.setStatus('loading', `Saving ${key}...`);
      await saver(data);
      
      // Update cache with saved data
      this.dataCache.set(key, {
        data,
        timestamp: Date.now(),
        ttl: 30000
      });
      
      this.setStatus('healthy');
      this.onDataSaved(key, data);
    } catch (error) {
      this.setStatus('error', `Failed to save ${key}`);
      this.onDataError(key, error);
      throw error;
    }
  }
  
  // Database integration helpers
  protected async queryDatabase(table: string, filters: any = {}): Promise<any[]> {
    const result = await this.executeCommand('database-query', {
      table,
      filters
    });
    
    if (result.success) {
      return result.data.records || [];
    } else {
      throw new Error(result.error || 'Database query failed');
    }
  }
  
  protected async saveToDatabase(table: string, id: string, data: any): Promise<string> {
    const result = await this.executeCommand('database-save', {
      table,
      id,
      data
    });
    
    if (result.success) {
      return result.data.id;
    } else {
      throw new Error(result.error || 'Database save failed');
    }
  }
  
  // Abstract methods for subclasses
  protected abstract onDataLoaded(key: string, data: any): void;
  protected abstract onDataSaved(key: string, data: any): void;
  protected abstract onDataError(key: string, error: any): void;
  protected abstract onDataRefreshNeeded(key: string): void;
  
  destroy(): void {
    this.stopAutoRefresh();
    this.dataCache.clear();
    super.destroy();
  }
}

interface CachedData {
  data: any;
  timestamp: number;
  ttl: number;
}
```

## 🎯 Widget Optimization Strategy

### Active Widgets (Essential for MVP)
```typescript
// ACTIVE - Core functionality needed
✅ ChatWidget (communication/chat)
✅ AcademyWidget (intelligence/academy)  
✅ PersonaWidget (intelligence/persona)
✅ SidebarWidget (navigation/sidebar)
✅ VersionWidget (system/version)
```

### Disabled Widgets (Defer until needed)
```typescript
// DISABLED - Can implement later
❌ HealthWidget (system/health) - disabled: "health_monitoring_not_mvp"
❌ LogWidget (system/logs) - disabled: "log_display_not_essential"
❌ PerformanceWidget (system/performance) - disabled: "performance_monitoring_later"
❌ SavedPersonasWidget - disabled: "superseded_by_persona_widget"
❌ ActiveProjectsWidget - disabled: "project_management_not_mvp"
❌ UserSelectorWidget - disabled: "single_user_mvp"
```

### Intermediate Base Class Usage
```typescript
// ChatWidget extends RealTimeWidget + CommandWidget  
export class ChatWidget extends RealTimeWidget implements CommandWidget {
  // Gets real-time updates + enhanced command execution
}

// AcademyWidget extends RealTimeWidget + StatusWidget + DataBoundWidget
export class AcademyWidget extends RealTimeWidget implements StatusWidget, DataBoundWidget {
  // Gets real-time updates + status display + data synchronization
}

// SidebarWidget extends InteractiveWidget
export class SidebarWidget extends InteractiveWidget {
  // Gets user interaction patterns + keyboard shortcuts
}
```

This optimization creates brilliant intermediate layers that provide exactly the functionality needed by domain widgets while maintaining clean separation of concerns and enabling selective widget activation.