/**
 * Shared DOM Test Utilities - Eliminates test setup duplication
 * 
 * Provides consistent mock DOM environment for testing browser-specific code
 * without repeating the setup patterns.
 */

export interface MockDOMRect {
  x: number;
  y: number;
  width: number;
  height: number;
  left: number;
  top: number;
  right: number;
  bottom: number;
  toJSON: () => any;
}

export interface MockElement {
  getBoundingClientRect: () => MockDOMRect;
  scrollWidth: number;
  scrollHeight: number;
  tagName: string;
  className: string;
  classList: { length: number; [index: number]: string };
  id: string;
}

/**
 * Create a mock DOMRect with proper interface compliance
 */
export function createMockDOMRect(
  x: number, 
  y: number, 
  width: number, 
  height: number
): MockDOMRect {
  return {
    x, y, width, height,
    left: x, 
    top: y, 
    right: x + width, 
    bottom: y + height,
    toJSON: () => ({ x, y, width, height, left: x, top: y, right: x + width, bottom: y + height })
  };
}

/**
 * Create a mock Element with proper interface compliance
 */
export function createMockElement(
  bounds: MockDOMRect,
  scrollWidth?: number,
  scrollHeight?: number,
  tagName: string = 'DIV',
  className: string = 'test-element'
): MockElement {
  return {
    getBoundingClientRect: () => bounds,
    scrollWidth: scrollWidth || bounds.width,
    scrollHeight: scrollHeight || bounds.height,
    tagName,
    className,
    classList: { length: 1, 0: className },
    id: ''
  };
}

/**
 * Setup mock browser globals for testing
 */
export function setupMockBrowserEnvironment(): void {
  // Mock window
  (global as any).window = {
    getComputedStyle: () => ({
      position: 'static',
      display: 'block'
    }),
    devicePixelRatio: 1
  };

  // Mock document with querySelector support
  (global as any).document = {
    body: createMockElement(createMockDOMRect(0, 0, 1200, 800)),
    querySelector: (selector: string) => {
      const elementMap: Record<string, MockElement> = {
        'chat-widget': (() => {
          const widget = createMockElement(createMockDOMRect(300, 200, 600, 400));
          widget.tagName = 'CHAT-WIDGET';
          widget.classList = { length: 1, 0: 'chat-widget' };
          return widget;
        })(),
        'screenshot-widget': (() => {
          const widget = createMockElement(createMockDOMRect(50, 100, 400, 300));
          widget.tagName = 'DIV';
          widget.classList = { length: 1, 0: 'screenshot-widget' };
          return widget;
        })(),
        'body': (() => {
          const body = createMockElement(createMockDOMRect(0, 0, 1200, 800));
          body.tagName = 'BODY';
          return body;
        })()
      };

      return elementMap[selector] || null;
    }
  };
}

/**
 * Clean up mock browser environment
 */
export function cleanupMockBrowserEnvironment(): void {
  delete (global as any).window;
  delete (global as any).document;
}

/**
 * Test runner wrapper with automatic cleanup
 */
export async function withMockBrowser<T>(testFn: () => Promise<T>): Promise<T> {
  setupMockBrowserEnvironment();
  try {
    return await testFn();
  } finally {
    cleanupMockBrowserEnvironment();
  }
}

/**
 * Create test scenarios for common widget testing
 */
export interface TestScenario {
  name: string;
  selector: string;
  expectedBounds: { x: number; y: number; width: number; height: number };
}

export const COMMON_WIDGET_SCENARIOS: TestScenario[] = [
  {
    name: 'Chat Widget',
    selector: 'chat-widget',
    expectedBounds: { x: 300, y: 200, width: 600, height: 400 }
  },
  {
    name: 'Screenshot Widget', 
    selector: 'screenshot-widget',
    expectedBounds: { x: 50, y: 100, width: 400, height: 300 }
  },
  {
    name: 'Full Body',
    selector: 'body', 
    expectedBounds: { x: 0, y: 0, width: 1200, height: 800 }
  }
];

/**
 * Validation helper for coordinate testing
 */
export function validateCoordinates(
  actual: { x: number; y: number; width: number; height: number },
  expected: { x: number; y: number; width: number; height: number },
  tolerance: number = 0
): { valid: boolean; error?: string } {
  const checks = [
    { name: 'x', actual: actual.x, expected: expected.x },
    { name: 'y', actual: actual.y, expected: expected.y },
    { name: 'width', actual: actual.width, expected: expected.width },
    { name: 'height', actual: actual.height, expected: expected.height }
  ];

  for (const check of checks) {
    if (Math.abs(check.actual - check.expected) > tolerance) {
      return {
        valid: false,
        error: `${check.name} mismatch: expected ${check.expected}, got ${check.actual}`
      };
    }
  }

  return { valid: true };
}