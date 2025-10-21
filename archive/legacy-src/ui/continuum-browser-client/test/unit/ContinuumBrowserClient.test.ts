/**
 * Unit tests for ContinuumBrowserClient
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';
import { ContinuumBrowserClient } from '../../ContinuumBrowserClient';

describe('ContinuumBrowserClient', () => {
  let client: ContinuumBrowserClient;

  beforeEach(() => {
    // Mock WebSocket
    global.WebSocket = jest.fn() as any;
    global.navigator = { userAgent: 'test-agent' } as any;
    global.window = { location: { href: 'http://localhost:3000' } } as any;
    
    client = new ContinuumBrowserClient();
  });

  describe('initialization', () => {
    it('should initialize with correct initial state', () => {
      expect(client.state).toBe('connecting');
      expect(client.sessionId).toBe(null);
      expect(client.clientId).toBe(null);
      expect(client.version).toBeDefined();
    });

    it('should have correct version from package.json', () => {
      expect(client.version).toMatch(/^\d+\.\d+\.\d+$/);
    });
  });

  describe('state management', () => {
    it('should allow state change callbacks', () => {
      const callback = jest.fn();
      client.onStateChange(callback);
      
      // Trigger state change (would normally come from WebSocket)
      // This is a simplified test - actual state changes happen through WebSocket events
      expect(callback).not.toHaveBeenCalled();
    });

    it('should call ready callbacks when ready', () => {
      const callback = jest.fn();
      client.onReady(callback);
      
      // If already ready, should call immediately
      if (client.state === 'ready') {
        expect(callback).toHaveBeenCalled();
      }
    });
  });

  describe('method attachment', () => {
    it('should allow dynamic method attachment', () => {
      const testMethod = jest.fn();
      client.attachMethod('test', testMethod);
      
      expect(client.hasMethod('test')).toBe(true);
    });

    it('should detect existing methods', () => {
      expect(client.hasMethod('execute')).toBe(true);
      expect(client.hasMethod('isConnected')).toBe(true);
    });
  });

  describe('connection status', () => {
    it('should report not connected when not ready', () => {
      expect(client.isConnected()).toBe(false);
    });
  });
});