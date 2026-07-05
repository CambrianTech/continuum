/**
 * Iframe Shim Bridge
 *
 * Type-safe client for communicating with JTAG shims injected into proxied iframes.
 * Enables remote control of web pages: screenshot, click, type, scroll, query, evaluate.
 *
 * Usage:
 *   const bridge = new IframeShimBridge(iframe);
 *   await bridge.waitForReady();
 *   const screenshot = await bridge.screenshot({ scale: 1 });
 *   await bridge.click({ selector: 'button.submit' });
 *   await bridge.type({ selector: 'input[name=search]', text: 'hello' });
 */

import type {
  ShimCommand,
  ShimCommandMap,
  ShimRequest,
  ShimResponse,
  ShimResult,
  ShimReadyEvent,
} from './IframeShimTypes';

export class IframeShimBridge {
  private iframe: HTMLIFrameElement;
  private readyPromise: Promise<ShimReadyEvent>;
  private readyResolve!: (event: ShimReadyEvent) => void;
  private isReady = false;
  private shimVersion?: string;
  private shimUrl?: string;

  constructor(iframe: HTMLIFrameElement) {
    this.iframe = iframe;

    // Set up ready promise
    this.readyPromise = new Promise((resolve) => {
      this.readyResolve = resolve;
    });

    // Listen for shim ready event
    window.addEventListener('message', this.handleMessage);
  }

  /**
   * Clean up event listeners
   */
  dispose(): void {
    window.removeEventListener('message', this.handleMessage);
  }

  /**
   * Wait for shim to be ready (announces itself on load)
   */
  async waitForReady(timeout = 10000): Promise<ShimReadyEvent> {
    if (this.isReady) {
      return { type: 'jtag-shim-ready', version: this.shimVersion!, url: this.shimUrl! };
    }

    const timeoutPromise = new Promise<never>((_, reject) => {
      setTimeout(() => reject(new Error('Shim ready timeout')), timeout);
    });

    return Promise.race([this.readyPromise, timeoutPromise]);
  }

  /**
   * Check if shim is ready
   */
  get ready(): boolean {
    return this.isReady;
  }

  /**
   * Execute a command on the shim
   */
  async execute<C extends ShimCommand>(
    command: C,
    params: ShimCommandMap[C]['params'],
    timeout = 30000
  ): Promise<ShimResult<ShimCommandMap[C]['result']>> {
    const requestId = `${command}-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;

    return new Promise((resolve) => {
      const timeoutId = setTimeout(() => {
        window.removeEventListener('message', handler);
        resolve({
          success: false,
          error: { message: `Command '${command}' timed out after ${timeout}ms`, code: 'TIMEOUT' }
        });
      }, timeout);

      const handler = (event: MessageEvent) => {
        const data = event.data as ShimResponse;
        if (data?.type === 'jtag-shim-response' && data?.requestId === requestId) {
          clearTimeout(timeoutId);
          window.removeEventListener('message', handler);
          resolve(data.result as ShimResult<ShimCommandMap[C]['result']>);
        }
      };

      window.addEventListener('message', handler);

      // Send request to iframe
      const request: ShimRequest = {
        type: 'jtag-shim-request',
        command,
        params,
        requestId
      };

      this.iframe.contentWindow?.postMessage(request, '*');
    });
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // CONVENIENCE METHODS
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Ping the shim to verify it's responsive
   */
  async ping(): Promise<ShimResult<ShimCommandMap['ping']['result']>> {
    return this.execute('ping', {});
  }

  /**
   * Capture screenshot of the page
   */
  async screenshot(
    params: ShimCommandMap['screenshot']['params'] = {}
  ): Promise<ShimResult<ShimCommandMap['screenshot']['result']>> {
    return this.execute('screenshot', params);
  }

  /**
   * Click an element
   */
  async click(
    params: ShimCommandMap['click']['params']
  ): Promise<ShimResult<ShimCommandMap['click']['result']>> {
    return this.execute('click', params);
  }

  /**
   * Type text into an input element
   */
  async type(
    params: ShimCommandMap['type']['params']
  ): Promise<ShimResult<ShimCommandMap['type']['result']>> {
    return this.execute('type', params);
  }

  /**
   * Scroll to element or position
   */
  async scroll(
    params: ShimCommandMap['scroll']['params'] = {}
  ): Promise<ShimResult<ShimCommandMap['scroll']['result']>> {
    return this.execute('scroll', params);
  }

  /**
   * Query for a single element
   */
  async query(
    params: ShimCommandMap['query']['params']
  ): Promise<ShimResult<ShimCommandMap['query']['result']>> {
    return this.execute('query', params);
  }

  /**
   * Query for multiple elements
   */
  async queryAll(
    params: ShimCommandMap['queryAll']['params']
  ): Promise<ShimResult<ShimCommandMap['queryAll']['result']>> {
    return this.execute('queryAll', params);
  }

  /**
   * Get value of input/select element
   */
  async getValue(
    params: ShimCommandMap['getValue']['params']
  ): Promise<ShimResult<ShimCommandMap['getValue']['result']>> {
    return this.execute('getValue', params);
  }

  /**
   * Set value of input/select element
   */
  async setValue(
    params: ShimCommandMap['setValue']['params']
  ): Promise<ShimResult<ShimCommandMap['setValue']['result']>> {
    return this.execute('setValue', params);
  }

  /**
   * Focus an element
   */
  async focus(
    params: ShimCommandMap['focus']['params']
  ): Promise<ShimResult<void>> {
    return this.execute('focus', params);
  }

  /**
   * Hover over an element
   */
  async hover(
    params: ShimCommandMap['hover']['params']
  ): Promise<ShimResult<void>> {
    return this.execute('hover', params);
  }

  /**
   * Wait for element to appear
   */
  async waitFor(
    params: ShimCommandMap['waitFor']['params']
  ): Promise<ShimResult<ShimCommandMap['waitFor']['result']>> {
    return this.execute('waitFor', params, params.timeout || 30000);
  }

  /**
   * Get page info (url, title, dimensions)
   */
  async pageInfo(): Promise<ShimResult<ShimCommandMap['pageInfo']['result']>> {
    return this.execute('pageInfo', {});
  }

  /**
   * Evaluate arbitrary JavaScript in the page context
   * Use with caution!
   */
  async evaluate(
    params: ShimCommandMap['evaluate']['params']
  ): Promise<ShimResult<ShimCommandMap['evaluate']['result']>> {
    return this.execute('evaluate', params);
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // PRIVATE
  // ═══════════════════════════════════════════════════════════════════════════

  private handleMessage = (event: MessageEvent): void => {
    const data = event.data as ShimReadyEvent;
    if (data?.type === 'jtag-shim-ready') {
      this.isReady = true;
      this.shimVersion = data.version;
      this.shimUrl = data.url;
      this.readyResolve(data);
      console.log(`[IframeShimBridge] Shim ready: v${data.version} @ ${data.url}`);
    }
  };
}

/**
 * Factory to create bridge from iframe selector (pierces shadow DOM)
 */
export function createBridgeFromSelector(selector: string): IframeShimBridge | null {
  // Smart query that pierces shadow DOM
  const findElement = (sel: string): Element | null => {
    // Try direct query first
    let element = document.querySelector(sel);
    if (element) return element;

    // Pierce shadow DOMs
    const walker = document.createTreeWalker(
      document.body,
      NodeFilter.SHOW_ELEMENT,
      null
    );

    while (walker.nextNode()) {
      const node = walker.currentNode as Element;
      if (node.shadowRoot) {
        element = node.shadowRoot.querySelector(sel);
        if (element) return element;
      }
    }

    return null;
  };

  const iframe = findElement(selector) as HTMLIFrameElement;
  if (!iframe || !(iframe instanceof HTMLIFrameElement)) {
    return null;
  }

  return new IframeShimBridge(iframe);
}
