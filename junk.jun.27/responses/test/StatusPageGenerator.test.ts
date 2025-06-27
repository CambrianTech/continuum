/**
 * StatusPageGenerator Unit Tests
 * Tests HTML generation, daemon listing, and uptime formatting
 */

import { StatusPageGenerator } from '../StatusPageGenerator';

describe('StatusPageGenerator', () => {
  let generator: StatusPageGenerator;
  
  beforeEach(() => {
    generator = new StatusPageGenerator('test-server', '1.0.0', 9000);
  });

  describe('HTML Generation', () => {
    test('should generate valid HTML structure', async () => {
      const mockDaemons = new Map();
      const html = await generator.generateStatusPage(mockDaemons);
      
      expect(html).toContain('<!DOCTYPE html>');
      expect(html).toContain('<html>');
      expect(html).toContain('<head>');
      expect(html).toContain('<body>');
      expect(html).toContain('Continuum Service');
    });

    test('should include server information', async () => {
      const mockDaemons = new Map();
      const html = await generator.generateStatusPage(mockDaemons);
      
      expect(html).toContain('test-server');
      expect(html).toContain('1.0.0');
      expect(html).toContain('9000');
      expect(html).toContain('Running');
    });

    test('should list registered daemons', async () => {
      const mockDaemons = new Map([
        ['renderer', { getStatus: () => 'running' }],
        ['command-processor', { getStatus: () => 'stopped' }]
      ]);
      
      const html = await generator.generateStatusPage(mockDaemons);
      
      expect(html).toContain('renderer');
      expect(html).toContain('command-processor');
      expect(html).toContain('running');
      expect(html).toContain('stopped');
    });

    test('should handle daemons without getStatus method', async () => {
      const mockDaemons = new Map([
        ['broken-daemon', {}]
      ]);
      
      const html = await generator.generateStatusPage(mockDaemons);
      
      expect(html).toContain('broken-daemon');
      expect(html).toContain('unknown');
    });
  });

  describe('Uptime Formatting', () => {
    test('should format uptime correctly', async () => {
      // Test by checking the HTML output contains reasonable uptime
      const mockDaemons = new Map();
      const html = await generator.generateStatusPage(mockDaemons);
      
      // Should contain uptime label and some value
      expect(html).toContain('Uptime:');
      expect(html).toMatch(/\d+[mhd]/); // Should match patterns like "5m", "2h", "1d"
    });
  });

  describe('CSS Styles', () => {
    test('should include comprehensive CSS styles', async () => {
      const mockDaemons = new Map();
      const html = await generator.generateStatusPage(mockDaemons);
      
      expect(html).toContain('<style>');
      expect(html).toContain('font-family');
      expect(html).toContain('background');
      expect(html).toContain('.container');
      expect(html).toContain('.daemon-status');
    });
  });

  describe('Endpoint Documentation', () => {
    test('should list available endpoints', async () => {
      const mockDaemons = new Map();
      const html = await generator.generateStatusPage(mockDaemons);
      
      expect(html).toContain('/health');
      expect(html).toContain('/status');
      expect(html).toContain('/api/*');
      expect(html).toContain('ws://');
    });
  });
});