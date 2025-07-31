/**
 * JTAG Client Factory - Intelligent Client Connection Strategy
 * 
 * Automatically provides the best JTAG client connection regardless of environment:
 * - Local system access when available (same process)
 * - Remote client connection when external (different process/GitHub page/test)
 * - Fully connected and ready to use
 * 
 * This is the single entry point for getting a JTAG client from anywhere.
 */

import type { JTAGBase } from './JTAGBase';
import type { UUID } from './CrossPlatformUUID';
import { generateUUID } from './CrossPlatformUUID';

export interface JTAGClientConfig {
  readonly sessionId?: UUID;
  readonly serverUrl?: string;
  readonly timeout?: number;
  readonly maxRetries?: number;
}

export class JTAGClientFactory {
  private static instance: JTAGBase | null = null;
  private static connecting: Promise<JTAGBase> | null = null;

  /**
   * Get the best JTAG client, fully connected and ready to use
   * 
   * Smart connection strategy:
   * 1. Return cached client if already connected
   * 2. Detect if we're inside a JTAG server process → use local system
   * 3. Detect if we're external (test/GitHub page/etc) → connect as remote client
   * 4. Handle all connection setup automatically
   * 5. Return fully connected client with commands interface ready
   */
  static async getClient(config: JTAGClientConfig = {}): Promise<JTAGBase> {
    // Return cached client if available
    if (JTAGClientFactory.instance) {
      console.log('🔌 JTAGClientFactory: Using cached client connection');
      return JTAGClientFactory.instance;
    }

    // Prevent multiple concurrent connections
    if (JTAGClientFactory.connecting) {
      console.log('🔌 JTAGClientFactory: Connection in progress, waiting...');
      return JTAGClientFactory.connecting;
    }

    // Start connection process
    JTAGClientFactory.connecting = JTAGClientFactory.createOptimalClient(config);
    
    try {
      const client = await JTAGClientFactory.connecting;
      JTAGClientFactory.instance = client;
      return client;
    } finally {
      JTAGClientFactory.connecting = null;
    }
  }

  /**
   * Create the optimal client based on environment detection
   */
  private static async createOptimalClient(config: JTAGClientConfig): Promise<JTAGBase> {
    const environment = JTAGClientFactory.detectEnvironment();
    
    console.log(`🔍 JTAGClientFactory: Environment detected as ${environment}`);

    if (environment === 'server') {
      return JTAGClientFactory.createServerClient(config);
    } else {
      return JTAGClientFactory.createBrowserClient(config);
    }
  }

  /**
   * Create optimal client for server environment
   */
  private static async createServerClient(config: JTAGClientConfig): Promise<JTAGBase> {
    // Use the existing smart getClient() method from server-index
    // This handles singleton detection and fallback logic properly
    const { jtag: serverJtag } = await import('../server-index');
    console.log('🔌 JTAGClientFactory: Delegating to server-index getClient()');
    return serverJtag.getClient();
  }

  /**
   * Create optimal client for browser environment  
   */
  private static async createBrowserClient(config: JTAGClientConfig): Promise<JTAGBase> {
    // Use the existing smart getClient() method from browser-index
    // This handles singleton detection and fallback logic properly
    const { jtag: browserJtag } = await import('../browser-index');
    console.log('🔌 JTAGClientFactory: Delegating to browser-index getClient()');
    return browserJtag.getClient();
  }


  /**
   * Detect current environment
   */
  private static detectEnvironment(): 'server' | 'browser' {
    // Standard environment detection
    if (typeof window !== 'undefined') {
      return 'browser';
    }
    
    if (typeof process !== 'undefined' && process.versions?.node) {
      return 'server';
    }

    // Default to server for unknown environments
    return 'server';
  }

  /**
   * Reset factory state (useful for testing)
   */
  static reset(): void {
    JTAGClientFactory.instance = null;
    JTAGClientFactory.connecting = null;
  }

  /**
   * Check if a client is already connected
   */
  static isConnected(): boolean {
    return JTAGClientFactory.instance !== null;
  }

  /**
   * Get cached client without connecting (returns null if not connected)
   */
  static getCachedClient(): JTAGBase | null {
    return JTAGClientFactory.instance;
  }
}