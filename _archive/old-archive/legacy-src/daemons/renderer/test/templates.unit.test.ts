/**
 * Layer 1 Unit Tests: Template Files
 * Middle-Out Testing Approach - Core template validation
 * 
 * Tests the foundational template files before testing daemon processing
 */

import { describe, test, expect, beforeAll } from '@jest/globals';
import { promises as fs } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

describe('Layer 1: Template Files - Core Validation', () => {
  let templatesDir: string;
  
  beforeAll(() => {
    // Get templates directory relative to this test file
    const currentDir = dirname(fileURLToPath(import.meta.url));
    templatesDir = join(currentDir, '..', 'templates');
  });

  describe('Template File Existence', () => {
    test('chat-widget.html exists and is readable', async () => {
      const filePath = join(templatesDir, 'chat-widget.html');
      await expect(fs.access(filePath, fs.constants.R_OK)).resolves.not.toThrow();
      
      const stats = await fs.stat(filePath);
      expect(stats.isFile()).toBe(true);
      expect(stats.size).toBeGreaterThan(0);
    });

    test('sidebar-widget.html exists and is readable', async () => {
      const filePath = join(templatesDir, 'sidebar-widget.html');
      await expect(fs.access(filePath, fs.constants.R_OK)).resolves.not.toThrow();
      
      const stats = await fs.stat(filePath);
      expect(stats.isFile()).toBe(true);
      expect(stats.size).toBeGreaterThan(0);
    });

    test('chat-widget.ts exists and is readable', async () => {
      const filePath = join(templatesDir, 'chat-widget.ts');
      await expect(fs.access(filePath, fs.constants.R_OK)).resolves.not.toThrow();
      
      const stats = await fs.stat(filePath);
      expect(stats.isFile()).toBe(true);
      expect(stats.size).toBeGreaterThan(0);
    });

    test('sidebar-widget.ts exists and is readable', async () => {
      const filePath = join(templatesDir, 'sidebar-widget.ts');
      await expect(fs.access(filePath, fs.constants.R_OK)).resolves.not.toThrow();
      
      const stats = await fs.stat(filePath);
      expect(stats.isFile()).toBe(true);
      expect(stats.size).toBeGreaterThan(0);
    });

    test('widget-loader.ts exists and is readable', async () => {
      const filePath = join(templatesDir, 'widget-loader.ts');
      await expect(fs.access(filePath, fs.constants.R_OK)).resolves.not.toThrow();
      
      const stats = await fs.stat(filePath);
      expect(stats.isFile()).toBe(true);
      expect(stats.size).toBeGreaterThan(0);
    });
  });

  describe('HTML Template Structure', () => {
    test('chat-widget.html contains required elements', async () => {
      const html = await fs.readFile(join(templatesDir, 'chat-widget.html'), 'utf-8');
      
      // Verify essential structure
      expect(html).toContain('<style>');
      expect(html).toContain(':host {');
      expect(html).toContain('<div class="chat-header">');
      expect(html).toContain('<div class="chat-messages">');
      expect(html).toContain('<div class="chat-input">');
      
      // Verify required IDs for JavaScript interaction
      expect(html).toContain('id="status"');
      expect(html).toContain('id="messages"');
      expect(html).toContain('id="chatInput"');
      
      // Verify version placeholder
      expect(html).toContain('{{CONTINUUM_VERSION}}');
      
      // Verify CSS classes for styling
      expect(html).toContain('class="connection-status"');
      expect(html).toContain('class="input-field"');
    });

    test('sidebar-widget.html contains required elements', async () => {
      const html = await fs.readFile(join(templatesDir, 'sidebar-widget.html'), 'utf-8');
      
      // Verify essential structure
      expect(html).toContain('<style>');
      expect(html).toContain(':host {');
      expect(html).toContain('<div class="sidebar-header">');
      expect(html).toContain('<div class="sidebar-content">');
      
      // Verify required IDs for JavaScript interaction
      expect(html).toContain('id="version"');
      expect(html).toContain('id="ws-status"');
      expect(html).toContain('id="cmd-status"');
      
      // Verify version placeholder
      expect(html).toContain('{{CONTINUUM_VERSION}}');
      
      // Verify status sections
      expect(html).toContain('WebSocket');
      expect(html).toContain('Commands');
      expect(html).toContain('Widgets');
      expect(html).toContain('Architecture');
    });
  });

  describe('TypeScript Template Structure', () => {
    test('chat-widget.ts has correct class structure', async () => {
      const ts = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      
      // Verify class definition
      expect(ts).toContain('class ChatWidget extends HTMLElement');
      expect(ts).toContain('private shadowRoot: ShadowRoot');
      expect(ts).toContain('private api?: ContinuumAPI');
      
      // Verify template placeholder
      expect(ts).toContain('{{CHAT_WIDGET_HTML}}');
      
      // Verify essential methods
      expect(ts).toContain('constructor()');
      expect(ts).toContain('connectedCallback()');
      expect(ts).toContain('private render()');
      expect(ts).toContain('private initializeWidget()');
      expect(ts).toContain('private setupConnection(');
      expect(ts).toContain('private addMessage(');
      
      // Verify TypeScript typing
      expect(ts).toContain(': void');
      expect(ts).toContain('HTMLElement');
      expect(ts).toContain('as HTMLElement');
    });

    test('sidebar-widget.ts has correct class structure', async () => {
      const ts = await fs.readFile(join(templatesDir, 'sidebar-widget.ts'), 'utf-8');
      
      // Verify class definition
      expect(ts).toContain('class ContinuumSidebar extends HTMLElement');
      expect(ts).toContain('private shadowRoot: ShadowRoot');
      expect(ts).toContain('private api?: ContinuumAPI');
      
      // Verify template placeholder
      expect(ts).toContain('{{SIDEBAR_WIDGET_HTML}}');
      
      // Verify essential methods
      expect(ts).toContain('constructor()');
      expect(ts).toContain('connectedCallback()');
      expect(ts).toContain('private render()');
      expect(ts).toContain('private initializeWidget()');
      expect(ts).toContain('private setupStatusUpdates(');
      expect(ts).toContain('private async testCommandDiscovery(');
    });

    test('widget-loader.ts has component registration', async () => {
      const ts = await fs.readFile(join(templatesDir, 'widget-loader.ts'), 'utf-8');
      
      // Verify interface definition
      expect(ts).toContain('interface ContinuumAPI');
      expect(ts).toContain('declare global');
      
      // Verify component registration
      expect(ts).toContain('customElements.define');
      expect(ts).toContain("'chat-widget'");
      expect(ts).toContain("'continuum-sidebar'");
      
      // Verify DOM ready handling
      expect(ts).toContain('DOMContentLoaded');
      expect(ts).toContain('querySelector');
      
      // Verify export
      expect(ts).toContain('export default');
    });
  });

  describe('Template Consistency', () => {
    test('HTML and TypeScript templates have matching placeholders', async () => {
      const chatHTML = await fs.readFile(join(templatesDir, 'chat-widget.html'), 'utf-8');
      const chatTS = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      
      // Verify version placeholder consistency
      expect(chatHTML).toContain('{{CONTINUUM_VERSION}}');
      expect(chatTS).toContain('{{CHAT_WIDGET_HTML}}');
      
      const sidebarHTML = await fs.readFile(join(templatesDir, 'sidebar-widget.html'), 'utf-8');
      const sidebarTS = await fs.readFile(join(templatesDir, 'sidebar-widget.ts'), 'utf-8');
      
      expect(sidebarHTML).toContain('{{CONTINUUM_VERSION}}');
      expect(sidebarTS).toContain('{{SIDEBAR_WIDGET_HTML}}');
    });

    test('HTML IDs match TypeScript getElementById calls', async () => {
      const chatHTML = await fs.readFile(join(templatesDir, 'chat-widget.html'), 'utf-8');
      const chatTS = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      
      // Extract IDs from HTML
      const htmlIds = [...chatHTML.matchAll(/id="([^"]+)"/g)].map(match => match[1]);
      
      // Verify TypeScript references each ID
      htmlIds.forEach(id => {
        expect(chatTS).toContain(`getElementById('${id}')`);
      });
      
      // Specific ID checks for chat widget
      expect(htmlIds).toContain('status');
      expect(htmlIds).toContain('messages');
      expect(htmlIds).toContain('chatInput');
    });
  });

  describe('Template Security', () => {
    test('HTML templates do not contain script tags', async () => {
      const chatHTML = await fs.readFile(join(templatesDir, 'chat-widget.html'), 'utf-8');
      const sidebarHTML = await fs.readFile(join(templatesDir, 'sidebar-widget.html'), 'utf-8');
      
      expect(chatHTML).not.toContain('<script');
      expect(chatHTML).not.toContain('javascript:');
      expect(sidebarHTML).not.toContain('<script');
      expect(sidebarHTML).not.toContain('javascript:');
    });

    test('TypeScript templates use proper type annotations', async () => {
      const chatTS = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      const sidebarTS = await fs.readFile(join(templatesDir, 'sidebar-widget.ts'), 'utf-8');
      
      // Verify no 'any' types
      expect(chatTS).not.toContain(': any');
      expect(sidebarTS).not.toContain(': any');
      
      // Verify proper type casting
      expect(chatTS).toContain('as HTMLElement');
      expect(chatTS).toContain('as HTMLInputElement');
      expect(sidebarTS).toContain('as HTMLElement');
    });
  });
});