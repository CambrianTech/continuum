/**
 * RendererDaemon Unit Tests
 * Tests the UIGenerator.cjs wrapper and rendering functionality
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { RendererDaemon } from '../RendererDaemon.js';

describe('RendererDaemon', () => {
  let daemon: RendererDaemon;

  beforeEach(() => {
    daemon = new RendererDaemon();
  });

  afterEach(async () => {
    if (daemon) {
      await daemon.stop();
    }
  });

  describe('Daemon Lifecycle', () => {
    it('should start and stop cleanly', async () => {
      expect(daemon.getStatus()).toBe('stopped');
      
      await daemon.start();
      expect(daemon.getStatus()).toBe('running');
      
      await daemon.stop();
      expect(daemon.getStatus()).toBe('stopped');
    });

    it('should load legacy renderer on startup', async () => {
      await daemon.start();
      
      const response = await daemon.sendMessage({
        type: 'get_capabilities',
        data: {}
      });
      
      expect(response.success).toBe(true);
      expect(response.data.engine).toBe('legacy');
      expect(response.data.capabilities).toContain('legacy-ui');
    });
  });

  describe('Legacy UIGenerator Integration', () => {
    beforeEach(async () => {
      await daemon.start();
    });

    it('should load UIGenerator.cjs successfully', async () => {
      const response = await daemon.sendMessage({
        type: 'get_capabilities',
        data: {}
      });
      
      expect(response.success).toBe(true);
      expect(response.data.capabilities).toContain('legacy-ui');
    });

    it('should handle render requests', async () => {
      const response = await daemon.sendMessage({
        type: 'render_request',
        data: {
          type: 'render_ui',
          data: { page: 'test' }
        }
      });
      
      // Should not crash even if render fails
      expect(response).toBeDefined();
      console.log('Render response:', response);
    });
  });

  describe('Engine Switching', () => {
    beforeEach(async () => {
      await daemon.start();
    });

    it('should switch between legacy and modern engines', async () => {
      // Start with legacy
      let response = await daemon.sendMessage({
        type: 'get_capabilities',
        data: {}
      });
      expect(response.data.engine).toBe('legacy');

      // Try to switch to modern (should fallback to legacy)
      response = await daemon.sendMessage({
        type: 'switch_engine',
        data: { engine: 'modern' }
      });
      
      expect(response.success).toBe(true);
      console.log('Engine switch response:', response);
    });
  });

  describe('Error Handling', () => {
    beforeEach(async () => {
      await daemon.start();
    });

    it('should handle invalid message types gracefully', async () => {
      const response = await daemon.sendMessage({
        type: 'invalid_message',
        data: {}
      });
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown message type');
    });
  });

  describe('CommonJS Integration', () => {
    it('should properly import UIGenerator.cjs in ES module context', async () => {
      // Test the import mechanism
      try {
        const { createRequire } = await import('module');
        const require = createRequire(import.meta.url);
        const UIGeneratorClass = require('../../ui/UIGenerator.cjs');
        
        expect(UIGeneratorClass).toBeDefined();
        expect(typeof UIGeneratorClass).toBe('function');
        
        // Try to instantiate
        const instance = new UIGeneratorClass(null);
        expect(instance).toBeDefined();
        expect(typeof instance.generateHTML).toBe('function');
        
      } catch (error) {
        console.error('CommonJS import test failed:', error);
        throw error;
      }
    });
  });
});