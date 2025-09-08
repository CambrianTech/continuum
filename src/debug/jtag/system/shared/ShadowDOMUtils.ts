/**
 * SHADOW DOM UTILITIES
 * 
 * Universal utilities for traversing Shadow DOM in browser environments.
 * These utilities are integrated into commands like get-text, screenshot, etc.
 * to handle Shadow DOM widgets seamlessly.
 * 
 * PURE TYPESCRIPT - No eval() or inline code generation
 */

export interface ShadowDOMSearchResult {
  found: boolean;
  elements: ShadowDOMElement[];
  mainDocumentMatches?: any[];
  totalShadowRoots: number;
  searchPath: string;
  options?: ShadowDOMQueryOptions;
}

export interface ShadowDOMElement {
  hostTag: string;
  hostId?: string;
  hostClass?: string;
  shadowContent?: string;
  matchingElements: {
    tag: string;
    id?: string;
    class?: string;
    text?: string;
    innerHTML?: string;
    attributes?: Record<string, string>;
  }[];
}

export interface ShadowDOMQueryOptions {
  /**
   * Standard CSS selector - will be applied inside Shadow DOM
   */
  querySelector?: string;
  
  /**
   * Text content to search for
   */
  textContent?: string;
  
  /**
   * Attribute to search for
   */
  attribute?: { name: string; value?: string };
  
  /**
   * Whether to include shadow DOM content in results
   */
  includeShadowContent?: boolean;
  
  /**
   * Maximum depth to traverse Shadow DOM trees
   */
  maxDepth?: number;
  
  /**
   * Whether to search in main document as well
   */
  includeMainDocument?: boolean;
}

/**
 * SHADOW DOM QUERY BUILDER - TYPE-SAFE APPROACH
 * 
 * Creates query options for Shadow DOM traversal without eval() or string injection.
 * Used by browser-side implementations for actual DOM manipulation.
 */
export function createShadowDOMQueryOptions(selector: string, options: Partial<ShadowDOMQueryOptions> = {}): ShadowDOMQueryOptions {
  return {
    querySelector: selector,
    includeMainDocument: options.includeMainDocument ?? true,
    includeShadowContent: options.includeShadowContent ?? false,
    maxDepth: options.maxDepth ?? 10,
    ...options
  };
}

/**
 * WIDGET INTERACTION DEBUGGING - Find and interact with widgets in Shadow DOM
 * 
 * Enhanced debugging utilities for finding and interacting with widgets that are
 * visually present but not responding to interactions.
 */

export interface WidgetInteractionResult {
  found: boolean;
  widgetElement: Element | null;
  shadowRoot: ShadowRoot | null;
  interactiveElements: Element[];
  eventListeners: string[];
  jtagConnection: any;
  renderingState: {
    isVisible: boolean;
    hasContent: boolean;
    boundsRect: DOMRect | null;
  };
}

/**
 * Deep inspect a widget element to understand its interaction state
 */
export function inspectWidgetInteractions(selector: string): WidgetInteractionResult {
  console.log(`🔍 ShadowDOMUtils: Deep inspecting widget interactions for: ${selector}`);
  
  // Find the widget element
  const widgetElement = document.querySelector(selector);
  
  if (!widgetElement) {
    console.log(`❌ ShadowDOMUtils: Widget element not found: ${selector}`);
    return {
      found: false,
      widgetElement: null,
      shadowRoot: null,
      interactiveElements: [],
      eventListeners: [],
      jtagConnection: null,
      renderingState: {
        isVisible: false,
        hasContent: false,
        boundsRect: null
      }
    };
  }
  
  console.log(`✅ ShadowDOMUtils: Widget element found:`, widgetElement);
  
  // Get shadow root
  const shadowRoot = widgetElement.shadowRoot;
  console.log(`🌒 ShadowDOMUtils: Shadow root:`, shadowRoot);
  
  // Check rendering state
  const bounds = widgetElement.getBoundingClientRect();
  const renderingState = {
    isVisible: bounds.width > 0 && bounds.height > 0,
    hasContent: (widgetElement.textContent || '').trim().length > 0,
    boundsRect: bounds
  };
  
  console.log(`📐 ShadowDOMUtils: Rendering state:`, renderingState);
  
  // Find interactive elements in shadow DOM
  let interactiveElements: Element[] = [];
  if (shadowRoot) {
    interactiveElements = Array.from(shadowRoot.querySelectorAll(
      'button, input, textarea, select, a[href], [onclick], [tabindex], [role="button"]'
    ));
    console.log(`🎯 ShadowDOMUtils: Found ${interactiveElements.length} interactive elements:`, interactiveElements);
  }
  
  // Check for event listeners (basic check - more advanced would need getEventListeners)
  const eventListeners: string[] = [];
  if (shadowRoot) {
    // Look for common event attributes
    const allElements = shadowRoot.querySelectorAll('*');
    allElements.forEach(el => {
      const attributes = Array.from(el.attributes);
      attributes.forEach(attr => {
        if (attr.name.startsWith('on')) {
          eventListeners.push(`${el.tagName.toLowerCase()}.${attr.name}`);
        }
      });
    });
  }
  
  // Check JTAG connection state
  let jtagConnection = null;
  if ((window as any).jtag) {
    jtagConnection = (window as any).jtag;
    console.log(`🔌 ShadowDOMUtils: JTAG connection found:`, jtagConnection);
  } else if ((window as any).widgetDaemon) {
    jtagConnection = (window as any).widgetDaemon;
    console.log(`🔌 ShadowDOMUtils: WidgetDaemon connection found:`, jtagConnection);
  } else {
    console.log(`❌ ShadowDOMUtils: No JTAG/Widget connection found on window`);
  }
  
  return {
    found: true,
    widgetElement,
    shadowRoot,
    interactiveElements,
    eventListeners,
    jtagConnection,
    renderingState
  };
}

/**
 * Test widget method accessibility - check if widget has working methods
 */
export function testWidgetMethodAccessibility(selector: string): {
  widgetInstance: any;
  availableMethods: string[];
  jtagMethods: string[];
  errors: string[];
} {
  console.log(`🧪 ShadowDOMUtils: Testing widget method accessibility for: ${selector}`);
  
  const result = {
    widgetInstance: null,
    availableMethods: [] as string[],
    jtagMethods: [] as string[],
    errors: [] as string[]
  };
  
  try {
    // Find widget element
    const widgetElement = document.querySelector(selector) as any;
    if (!widgetElement) {
      result.errors.push(`Widget element not found: ${selector}`);
      return result;
    }
    
    result.widgetInstance = widgetElement;
    
    // Get available methods on widget
    const proto = Object.getPrototypeOf(widgetElement);
    if (proto) {
      result.availableMethods = Object.getOwnPropertyNames(proto)
        .filter(name => typeof widgetElement[name] === 'function' && !name.startsWith('_'))
        .filter(name => !['constructor', 'connectedCallback', 'disconnectedCallback'].includes(name));
    }
    
    console.log(`📋 ShadowDOMUtils: Available widget methods:`, result.availableMethods);
    
    // Check JTAG methods
    if (widgetElement.jtagClient?.commands) {
      result.jtagMethods = Object.keys(widgetElement.jtagClient.commands);
      console.log(`⚡ ShadowDOMUtils: JTAG command methods:`, result.jtagMethods);
    } else {
      result.errors.push('Widget has no jtagClient.commands');
    }
    
    // Test basic widget state
    if (widgetElement.state) {
      console.log(`📊 ShadowDOMUtils: Widget state:`, widgetElement.state);
    } else {
      result.errors.push('Widget has no state property');
    }
    
  } catch (error) {
    result.errors.push(`Error testing widget: ${error}`);
    console.error(`❌ ShadowDOMUtils: Widget test error:`, error);
  }
  
  return result;
}

/**
 * UNIVERSAL WIDGET FINDER - Get any widget by name across nested Shadow DOMs
 * 
 * This utility traverses all shadow DOM trees to find widgets by name,
 * making it easy to access widgets regardless of nesting depth.
 */
export function getWidgetByName(widgetName: string): Element | null {
  console.log(`🔍 ShadowDOMUtils: Searching for widget: ${widgetName}`);
  
  function searchInElement(element: Element, depth: number = 0, maxDepth: number = 5): Element | null {
    if (depth > maxDepth) return null;
    
    // Check if current element matches
    if (element.tagName.toLowerCase() === widgetName.toLowerCase()) {
      console.log(`✅ ShadowDOMUtils: Found ${widgetName} at depth ${depth}`);
      return element;
    }
    
    // Search in shadow DOM
    if (element.shadowRoot) {
      const shadowElements = element.shadowRoot.querySelectorAll('*');
      for (const shadowEl of shadowElements) {
        const found = searchInElement(shadowEl, depth + 1, maxDepth);
        if (found) return found;
      }
    }
    
    return null;
  }
  
  // Search starting from document
  const allElements = document.querySelectorAll('*');
  for (const element of allElements) {
    const found = searchInElement(element);
    if (found) return found;
  }
  
  console.log(`❌ ShadowDOMUtils: Widget ${widgetName} not found`);
  return null;
}

/**
 * GET ALL WIDGETS - Find all widgets in the page with their paths
 */
export function getAllWidgets(): Array<{
  name: string;
  element: Element;
  path: string;
  hasShadow: boolean;
  methods: string[];
}> {
  console.log(`🔍 ShadowDOMUtils: Finding all widgets on page`);
  
  const widgets: Array<{
    name: string;
    element: Element;
    path: string;
    hasShadow: boolean;
    methods: string[];
  }> = [];
  
  function searchInElement(element: Element, path: string = '', depth: number = 0): void {
    if (depth > 5) return;
    
    // Check if element is a widget (contains hyphen, typical for custom elements)
    if (element.tagName.includes('-')) {
      const methods = Object.getOwnPropertyNames(Object.getPrototypeOf(element))
        .filter(name => typeof (element as any)[name] === 'function')
        .filter(name => !['constructor', 'connectedCallback', 'disconnectedCallback'].includes(name));
      
      widgets.push({
        name: element.tagName.toLowerCase(),
        element,
        path: path + element.tagName.toLowerCase(),
        hasShadow: !!element.shadowRoot,
        methods
      });
    }
    
    // Search in shadow DOM
    if (element.shadowRoot) {
      const shadowElements = element.shadowRoot.querySelectorAll('*');
      for (const shadowEl of shadowElements) {
        searchInElement(shadowEl, path + element.tagName.toLowerCase() + ' → ', depth + 1);
      }
    }
  }
  
  // Search starting from document
  const allElements = document.querySelectorAll('*');
  for (const element of allElements) {
    searchInElement(element);
  }
  
  console.log(`✅ ShadowDOMUtils: Found ${widgets.length} widgets:`, widgets.map(w => w.name));
  return widgets;
}

/**
 * WIDGET METHOD TESTER - Test if a widget has and can execute specific methods
 */
export function testWidgetMethods(widgetName: string, methodsToTest: string[]): {
  widget: Element | null;
  results: Record<string, { exists: boolean; executable: boolean; error?: string }>;
} {
  console.log(`🧪 ShadowDOMUtils: Testing methods on widget: ${widgetName}`);
  
  const widget = getWidgetByName(widgetName);
  const results: Record<string, { exists: boolean; executable: boolean; error?: string }> = {};
  
  if (!widget) {
    methodsToTest.forEach(method => {
      results[method] = { exists: false, executable: false, error: 'Widget not found' };
    });
    return { widget: null, results };
  }
  
  for (const methodName of methodsToTest) {
    try {
      const method = (widget as any)[methodName];
      const exists = typeof method === 'function';
      
      results[methodName] = {
        exists,
        executable: exists,
      };
      
      console.log(`${exists ? '✅' : '❌'} ShadowDOMUtils: Method ${methodName} ${exists ? 'exists' : 'missing'} on ${widgetName}`);
      
    } catch (error) {
      results[methodName] = {
        exists: false,
        executable: false,
        error: String(error)
      };
    }
  }
  
  return { widget, results };
}

/**
 * Force widget to attempt JTAG connection using available connection methods
 */
export async function forceWidgetJTAGConnection(selector: string): Promise<{
  success: boolean;
  connectionMethod: string;
  error?: string;
  client?: any;
}> {
  console.log(`🔌 ShadowDOMUtils: Forcing JTAG connection for widget: ${selector}`);
  
  const widgetElement = document.querySelector(selector) as any;
  if (!widgetElement) {
    return {
      success: false,
      connectionMethod: 'none',
      error: 'Widget element not found'
    };
  }
  
  // Try different connection methods
  try {
    // Method 1: window.jtag.connect()
    if ((window as any).jtag?.connect) {
      console.log(`🔄 ShadowDOMUtils: Trying window.jtag.connect()`);
      const jtagSystem = await (window as any).jtag.connect();
      if (jtagSystem?.client) {
        widgetElement.jtagClient = jtagSystem.client;
        return {
          success: true,
          connectionMethod: 'window.jtag.connect',
          client: jtagSystem.client
        };
      }
    }
    
    // Method 2: Use widgetDaemon directly
    if ((window as any).widgetDaemon) {
      console.log(`🔄 ShadowDOMUtils: Trying widgetDaemon direct connection`);
      const daemon = (window as any).widgetDaemon;
      if (daemon.router) {
        widgetElement.jtagClient = {
          commands: daemon.router.commands || {},
          events: daemon.router.events || { on: () => {}, off: () => {}, emit: () => {} }
        };
        return {
          success: true,
          connectionMethod: 'widgetDaemon.router',
          client: widgetElement.jtagClient
        };
      }
    }
    
    // Method 3: Manual JTAG client creation
    console.log(`🔄 ShadowDOMUtils: Trying manual JTAG client creation`);
    if ((window as any).JTAGClientFactory) {
      const factory = (window as any).JTAGClientFactory.getInstance();
      const connection = await factory.createClient({ timeout: 10000 });
      widgetElement.jtagClient = connection.client;
      return {
        success: true,
        connectionMethod: 'JTAGClientFactory',
        client: connection.client
      };
    }
    
    return {
      success: false,
      connectionMethod: 'none',
      error: 'No valid connection methods available'
    };
    
  } catch (error) {
    console.error(`❌ ShadowDOMUtils: Connection attempt failed:`, error);
    return {
      success: false,
      connectionMethod: 'error',
      error: String(error)
    };
  }
}

/**
 * ENHANCED SELECTOR - Combines standard and Shadow DOM selectors
 * 
 * Usage: "button" -> searches both main document and Shadow DOM
 * Usage: "shadow:button" -> searches only Shadow DOM
 * Usage: "main:button" -> searches only main document
 */
export function parseEnhancedSelector(selector: string): {
  actualSelector: string;
  searchTarget: 'both' | 'shadow' | 'main';
  queryOptions: ShadowDOMQueryOptions;
} {
  let searchTarget: 'both' | 'shadow' | 'main' = 'both';
  let actualSelector = selector;
  
  if (selector.startsWith('shadow:')) {
    searchTarget = 'shadow';
    actualSelector = selector.substring(7);
  } else if (selector.startsWith('main:')) {
    searchTarget = 'main';
    actualSelector = selector.substring(5);
  }
  
  const queryOptions: ShadowDOMQueryOptions = {
    querySelector: actualSelector,
    includeMainDocument: searchTarget === 'both' || searchTarget === 'main',
    includeShadowContent: true,
    maxDepth: 10
  };
  
  return {
    actualSelector,
    searchTarget,
    queryOptions
  };
}