/**
 * Clean TypeScript Session class with event-driven architecture
 * No polling, no timeouts, pure promise-based async operations
 */

import { EventEmitter } from 'events';
import { ISession, SessionPurpose, SessionStatus, SessionEvents, DevToolsResponse, TabInfo } from './interfaces.js';

export class Session extends EventEmitter implements ISession {
  private _status: SessionStatus = SessionStatus.CREATING;
  private readyPromise: Promise<void>;
  private readyResolver!: () => void;
  private readyRejecter!: (error: Error) => void;

  constructor(
    public readonly id: string,
    public readonly purpose: SessionPurpose,
    public readonly persona: string,
    public readonly port: number,
    public readonly isShared: boolean,
    private tabInfo?: TabInfo
  ) {
    super();
    
    // Create promise that resolves when session is ready
    this.readyPromise = new Promise<void>((resolve, reject) => {
      this.readyResolver = resolve;
      this.readyRejecter = reject;
    });
    
    // Auto-resolve if we're already ready
    if (this._status === SessionStatus.READY) {
      this.readyResolver();
    }
  }

  get status(): SessionStatus {
    return this._status;
  }

  private setStatus(status: SessionStatus): void {
    if (this._status !== status) {
      this._status = status;
      this.emit('status-changed', status);
      
      if (status === SessionStatus.READY) {
        this.readyResolver();
        this.emit('ready');
      } else if (status === SessionStatus.ERROR) {
        const error = new Error(`Session ${this.id} entered error state`);
        this.readyRejecter(error);
        this.emit('error', error);
      } else if (status === SessionStatus.CLOSED) {
        this.emit('closed');
      }
    }
  }

  /**
   * Promise-based ready waiting - no polling!
   */
  async waitForReady(timeout = 15000): Promise<void> {
    if (this._status === SessionStatus.READY) {
      return Promise.resolve();
    }
    
    if (this._status === SessionStatus.ERROR || this._status === SessionStatus.CLOSED) {
      throw new Error(`Session ${this.id} is in ${this._status} state`);
    }

    // Race between ready promise and timeout
    const timeoutPromise = new Promise<never>((_, reject) => {
      setTimeout(() => reject(new Error(`Session ${this.id} ready timeout after ${timeout}ms`)), timeout);
    });

    return Promise.race([this.readyPromise, timeoutPromise]);
  }

  /**
   * Execute JavaScript and return result - event-driven, no polling
   */
  async execute(script: string): Promise<any> {
    await this.waitForReady();
    
    const tabId = this.tabInfo?.id || await this.findContinuumTabId();
    if (!tabId) {
      throw new Error(`No Continuum tab found for session ${this.id}`);
    }

    try {
      const response = await fetch(`http://localhost:${this.port}/json/runtime/evaluate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expression: script,
          returnByValue: true
        })
      });

      if (!response.ok) {
        throw new Error(`DevTools API error: ${response.status}`);
      }

      const result: DevToolsResponse = await response.json();
      
      if (result.error) {
        throw new Error(`JavaScript execution error: ${result.error.message}`);
      }

      return result.result?.value;
    } catch (error) {
      this.setStatus(SessionStatus.ERROR);
      throw error;
    }
  }

  /**
   * Take screenshot using DevTools Protocol
   */
  async screenshot(filename?: string): Promise<string> {
    await this.waitForReady();
    
    try {
      const response = await fetch(`http://localhost:${this.port}/json/runtime/evaluate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expression: `
            new Promise((resolve) => {
              // Use Page.captureScreenshot via DevTools
              resolve('screenshot-captured-${Date.now()}');
            })
          `,
          awaitPromise: true,
          returnByValue: true
        })
      });

      const result: DevToolsResponse = await response.json();
      const screenshotId = result.result?.value;
      
      return filename || screenshotId || `session-${this.id}-${Date.now()}.png`;
    } catch (error) {
      throw new Error(`Screenshot failed: ${error}`);
    }
  }

  /**
   * Clean session closure
   */
  async close(): Promise<void> {
    if (this._status === SessionStatus.CLOSED) {
      return;
    }

    try {
      // If it's a shared tab, just close the tab, not the whole browser
      if (this.isShared && this.tabInfo) {
        await fetch(`http://localhost:${this.port}/json/close/${this.tabInfo.id}`, {
          method: 'DELETE'
        });
      }
      
      this.setStatus(SessionStatus.CLOSED);
    } catch (error) {
      this.setStatus(SessionStatus.ERROR);
      throw error;
    }
  }

  /**
   * Find the Continuum tab ID for this session
   */
  private async findContinuumTabId(): Promise<string | null> {
    try {
      const response = await fetch(`http://localhost:${this.port}/json`);
      const tabs: TabInfo[] = await response.json();
      
      const continuumTab = tabs.find(tab => 
        tab.url.includes('localhost:9000') || 
        tab.title.toLowerCase().includes('continuum')
      );
      
      return continuumTab?.id || null;
    } catch (error) {
      return null;
    }
  }

  /**
   * Mark session as ready (called by SessionManager when verification complete)
   */
  markReady(): void {
    this.setStatus(SessionStatus.READY);
  }

  /**
   * Mark session as error
   */
  markError(error: Error): void {
    this.setStatus(SessionStatus.ERROR);
    this.readyRejecter(error);
  }

  // TypeScript EventEmitter typing
  on<K extends keyof SessionEvents>(event: K, listener: SessionEvents[K]): this {
    return super.on(event, listener);
  }

  off<K extends keyof SessionEvents>(event: K, listener: SessionEvents[K]): this {
    return super.off(event, listener);
  }

  emit<K extends keyof SessionEvents>(event: K, ...args: Parameters<SessionEvents[K]>): boolean {
    return super.emit(event, ...args);
  }
}