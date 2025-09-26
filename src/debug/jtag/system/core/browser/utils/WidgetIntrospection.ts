/**
 * Widget Introspection Utilities
 *
 * Reusable browser utilities for widget discovery, state analysis, and DOM traversal
 * Used by debug commands, tests, and general widget system utilities
 */

export interface WidgetReference {
  element: HTMLElement;
  path: string;
  type: string;
  shadowRoot: ShadowRoot | null;
}

export interface WidgetState {
  properties: Record<string, unknown>;
  methods: string[];
  messageCount?: number;
  entityCount?: number;
  currentRoomId?: string;
}

export interface EventSystemInfo {
  hasEventEmitter: boolean;
  eventEmitterSize: number;
  eventTypes: string[];
  eventListeners: Array<{
    event: string;
    handlerCount: number;
  }>;
}

/**
 * Core widget discovery utilities
 */
export class WidgetDiscovery {
  private static readonly WIDGET_CONTAINERS = ['sidebar-widget', 'main-widget', 'theme-widget'];

  /**
   * Get all available widget containers with their shadow roots
   */
  static getWidgetContainers(): Array<{ name: string; shadowRoot: ShadowRoot }> {
    const continuumWidget = document.querySelector('continuum-widget');
    if (!continuumWidget?.shadowRoot) return [];

    return this.WIDGET_CONTAINERS
      .map(containerName => ({
        name: containerName,
        container: continuumWidget.shadowRoot!.querySelector(containerName) as HTMLElement
      }))
      .filter(({ container }) => container?.shadowRoot)
      .map(({ name, container }) => ({ name, shadowRoot: container.shadowRoot! }));
  }

  /**
   * Find widget by selector using proper DOM traversal
   */
  static findWidget(selector: string): WidgetReference | null {
    try {
      const containers = this.getWidgetContainers();

      for (const { name: containerName, shadowRoot } of containers) {
        const targetWidget = shadowRoot.querySelector(selector) as HTMLElement;
        if (targetWidget) {
          return {
            element: targetWidget,
            path: `continuum-widget -> ${containerName} -> ${selector}`,
            type: selector,
            shadowRoot: targetWidget.shadowRoot
          };
        }
      }

      return null;
    } catch (error) {
      console.error(`Widget discovery failed for ${selector}:`, error);
      return null;
    }
  }

  /**
   * Find all widgets of a given type
   */
  static findAllWidgets(pattern: string): WidgetReference[] {
    const widgets: WidgetReference[] = [];

    try {
      const containers = this.getWidgetContainers();
      let globalIndex = 0;

      for (const { name: containerName, shadowRoot } of containers) {
        const elements = shadowRoot.querySelectorAll(pattern);
        elements.forEach((element) => {
          widgets.push({
            element: element as HTMLElement,
            path: `continuum-widget -> ${containerName} -> ${pattern}[${globalIndex}]`,
            type: pattern,
            shadowRoot: (element as HTMLElement).shadowRoot
          });
          globalIndex++;
        });
      }
    } catch (error) {
      console.error(`Widget discovery failed for pattern ${pattern}:`, error);
    }

    return widgets;
  }
}

/**
 * Widget state analysis utilities
 */
export class WidgetAnalyzer {
  /**
   * Extract widget state safely
   */
  static analyzeState(widget: HTMLElement): WidgetState {
    const state: WidgetState = {
      properties: {},
      methods: []
    };

    try {
      // Get methods from prototype chain
      const proto = Object.getPrototypeOf(widget);
      const descriptors = Object.getOwnPropertyDescriptors(proto);

      Object.keys(descriptors).forEach(key => {
        const descriptor = descriptors[key];
        if (typeof descriptor.value === 'function') {
          state.methods.push(key);
        }
      });

      // Get safe properties (avoiding functions and private fields)
      const widgetObj = widget as unknown as Record<string, unknown>;
      Object.keys(widgetObj).forEach(key => {
        if (key.startsWith('_')) return; // Skip private

        const value = widgetObj[key];
        if (typeof value === 'function') return; // Skip functions

        try {
          state.properties[key] = this.serializeSafely(value);
        } catch {
          state.properties[key] = '[Unserializable]';
        }
      });

      // Extract common widget properties

      if (Array.isArray(widgetObj.messages)) {
        state.messageCount = widgetObj.messages.length;
      }

      if (typeof widgetObj.currentRoomId === 'string') {
        state.currentRoomId = widgetObj.currentRoomId;
      }

      // Check for entity collections (room/user lists)
      if (widgetObj.roomScroller && typeof (widgetObj.roomScroller as Record<string, unknown>).entities === 'function') {
        try {
          const entities = ((widgetObj.roomScroller as Record<string, unknown>).entities as () => unknown[])();
          state.entityCount = Array.isArray(entities) ? entities.length : 0;
        } catch {
          // Ignore entity count extraction failure
        }
      }

    } catch (error) {
      console.error('Widget state analysis failed:', error);
    }

    return state;
  }

  /**
   * Analyze widget event system
   */
  static analyzeEventSystem(widget: HTMLElement): EventSystemInfo {
    const info: EventSystemInfo = {
      hasEventEmitter: false,
      eventEmitterSize: 0,
      eventTypes: [],
      eventListeners: []
    };

    try {
      const widgetObj = widget as unknown as Record<string, unknown>;

      if (widgetObj.eventEmitter instanceof Map) {
        info.hasEventEmitter = true;
        info.eventEmitterSize = widgetObj.eventEmitter.size;
        info.eventTypes = Array.from(widgetObj.eventEmitter.keys()).map(String);

        info.eventListeners = info.eventTypes.map(event => ({
          event,
          handlerCount: 1 // Simplified - could be enhanced to count actual handlers
        }));
      }
    } catch (error) {
      console.error('Event system analysis failed:', error);
    }

    return info;
  }

  /**
   * Safe serialization for complex objects
   */
  private static serializeSafely(value: unknown): unknown {
    if (value === null || value === undefined) {
      return value;
    }

    if (typeof value === 'object') {
      if (Array.isArray(value)) {
        return `Array(${value.length})`;
      } else if (value instanceof Map) {
        return `Map(${value.size})`;
      } else if (value instanceof Set) {
        return `Set(${value.size})`;
      } else if (value instanceof Date) {
        return value.toISOString();
      } else {
        return `Object(${Object.keys(value).length} keys)`;
      }
    }

    return value;
  }
}

/**
 * DOM utilities for widget inspection
 */
export class WidgetDOMAnalyzer {
  /**
   * Analyze widget DOM structure
   */
  static analyzeDOMStructure(widget: HTMLElement) {
    try {
      const shadowRoot = widget.shadowRoot;
      const elements = shadowRoot ? shadowRoot.querySelectorAll('*') : [];
      const visibleText = widget.textContent?.slice(0, 200) || '';
      const cssClasses = Array.from(widget.classList || []);

      return {
        elementCount: elements.length,
        visibleText,
        cssClasses,
        hasShadowRoot: !!shadowRoot
      };
    } catch (error) {
      console.error('DOM analysis failed:', error);
      return {
        elementCount: 0,
        visibleText: '',
        cssClasses: [],
        hasShadowRoot: false
      };
    }
  }

  /**
   * Deep query selector - finds elements in nested shadow DOM with specific chat-widget handling
   */
  static deepQuerySelector(selector: string): HTMLElement | null {
    try {
      // Special handling for chat-widget which is in main-widget, not sidebar-widget
      if (selector === 'chat-widget') {
        const continuumWidget = document.querySelector('continuum-widget');
        const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
        const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
        return chatWidget as HTMLElement || null;
      }

      // Use normal container discovery for other widgets
      const containers = WidgetDiscovery.getWidgetContainers();

      for (const { shadowRoot } of containers) {
        const element = shadowRoot.querySelector(selector) as HTMLElement;
        if (element) return element;
      }

      return null;
    } catch (error) {
      console.error(`Deep query selector failed for ${selector}:`, error);
      return null;
    }
  }

  /**
   * Deep query all - finds all matching elements in nested shadow DOM
   */
  static deepQuerySelectorAll(selector: string): HTMLElement[] {
    try {
      const containers = WidgetDiscovery.getWidgetContainers();
      const results: HTMLElement[] = [];

      for (const { shadowRoot } of containers) {
        const elements = shadowRoot.querySelectorAll(selector);
        results.push(...Array.from(elements) as HTMLElement[]);
      }

      return results;
    } catch (error) {
      console.error(`Deep query selector all failed for ${selector}:`, error);
      return [];
    }
  }

  /**
   * Extract row data from widget - explicit handling for each widget type
   */
  static extractRowData(widgetSelector: string, rowSelector?: string) {
    try {
      let widget: HTMLElement | null = null;

      // Explicit widget path handling
      if (widgetSelector === 'chat-widget') {
        const continuumWidget = document.querySelector('continuum-widget');
        const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
        widget = mainWidget?.shadowRoot?.querySelector('chat-widget') as HTMLElement;
      } else {
        widget = this.deepQuerySelector(widgetSelector);
      }

      if (!widget?.shadowRoot) return [];

      // If specific selector provided, use it
      if (rowSelector) {
        const rows = Array.from(widget.shadowRoot.querySelectorAll(rowSelector));
        return rows.map((row, index) => this.createRowData(row as HTMLElement, index));
      }

      // Widget-specific selectors that we know work
      let selectors: string[] = [];

      if (widgetSelector === 'chat-widget') {
        selectors = ['.message-row[data-message-id]', '.message-row', '#messages > div'];
      } else if (widgetSelector === 'room-list-widget') {
        selectors = ['.room-item[data-entity-id]', '.room-item', '.entity-list-body > *'];
      } else if (widgetSelector === 'user-list-widget') {
        selectors = ['.user-item[data-entity-id]', '.user-item', '.entity-list-body > *'];
      } else {
        // Generic fallback
        selectors = ['[data-entity-id]', '[data-message-id]', '[data-room-id]', '[data-user-id]'];
      }

      for (const selector of selectors) {
        const rows = Array.from(widget.shadowRoot.querySelectorAll(selector));
        if (rows.length > 0) {
          return rows.map((row, index) => this.createRowData(row as HTMLElement, index));
        }
      }

      return [];
    } catch (error) {
      console.error(`Row data extraction failed for ${widgetSelector}:`, error);
      return [];
    }
  }

  /**
   * Create standardized row data object
   */
  private static createRowData(row: HTMLElement, index: number) {
    return {
      index,
      element: row,
      id: row.getAttribute('data-entity-id') ||
          row.getAttribute('data-message-id') ||
          row.getAttribute('data-room-id') ||
          row.getAttribute('data-user-id') ||
          row.id,
      textContent: row.textContent?.trim() || '',
      attributes: this.extractDataAttributes(row),
      classes: Array.from(row.classList)
    };
  }

  /**
   * Smart row selector detection - simplified and more robust
   */
  static detectRowSelector(shadowRoot: ShadowRoot, widgetSelector: string): string {
    // Generic selectors that work for any widget structure
    const genericSelectors = [
      '[data-entity-id]',           // Any element with entity ID (most reliable)
      '[data-message-id]',          // Chat messages
      '[data-room-id]',             // Room items
      '[data-user-id]',             // User items
      '.entity-list-body > *',      // EntityScroller children
      '.message-row',               // Chat message rows
      '.room-item',                 // Room list items
      '.user-item'                  // User list items
    ];

    // Test each selector to see which finds elements
    for (const selector of genericSelectors) {
      const elements = shadowRoot.querySelectorAll(selector);
      if (elements.length > 0) {
        return selector;
      }
    }

    // Ultimate fallback - any div with some content
    return 'div';
  }

  /**
   * Extract all data-* attributes from an element
   */
  static extractDataAttributes(element: HTMLElement): Record<string, string> {
    const dataAttrs: Record<string, string> = {};
    for (const attr of element.attributes) {
      if (attr.name.startsWith('data-')) {
        const key = attr.name.substring(5); // Remove 'data-' prefix
        dataAttrs[key] = attr.value;
      }
    }
    return dataAttrs;
  }

  /**
   * Monitor row changes - useful for detecting real-time updates
   */
  static monitorRowChanges(widgetSelector: string, callback: (changes: any) => void, options: {
    rowSelector?: string;
    attributes?: boolean;
    childList?: boolean;
    subtree?: boolean;
  } = {}) {
    try {
      const widget = this.deepQuerySelector(widgetSelector);
      if (!widget?.shadowRoot) {
        console.warn(`Cannot monitor ${widgetSelector} - widget not found`);
        return null;
      }

      const target = options.subtree ? widget.shadowRoot : widget.shadowRoot.querySelector('.room-list, .user-list, .message-list') || widget.shadowRoot;

      const observer = new MutationObserver((mutations) => {
        const changes = {
          timestamp: Date.now(),
          mutations: mutations.map(mutation => ({
            type: mutation.type,
            addedNodes: Array.from(mutation.addedNodes).map(node => ({
              nodeName: node.nodeName,
              textContent: node.textContent?.trim(),
              attributes: node.nodeType === 1 ? this.extractDataAttributes(node as HTMLElement) : {}
            })),
            removedNodes: Array.from(mutation.removedNodes).map(node => ({
              nodeName: node.nodeName,
              textContent: node.textContent?.trim()
            }))
          }))
        };
        callback(changes);
      });

      observer.observe(target, {
        childList: options.childList !== false,
        attributes: options.attributes || false,
        subtree: options.subtree !== false,
        ...options
      });

      return observer;
    } catch (error) {
      console.error(`Row change monitoring failed for ${widgetSelector}:`, error);
      return null;
    }
  }
}

/**
 * Connectivity testing utilities
 */
export class WidgetConnectivityTester {
  /**
   * Test widget's data connectivity capabilities
   */
  static testDataConnectivity(widget: HTMLElement) {
    const widgetObj = widget as unknown as Record<string, unknown>;

    const hasJtagOperation = 'jtagOperation' in widgetObj;
    const hasExecuteCommand = 'executeCommand' in widgetObj;
    const canExecuteCommands = hasJtagOperation || hasExecuteCommand;

    return {
      hasJtagOperation,
      hasExecuteCommand,
      canExecuteCommands,
      windowJtag: 'jtag' in window,
      windowWidgetDaemon: 'widgetDaemon' in window
    };
  }
}