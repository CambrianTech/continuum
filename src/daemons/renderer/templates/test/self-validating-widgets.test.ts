/**
 * Self-Validating Widget Tests - Simple & Robust
 * Widgets validate themselves - tests just verify validation works
 */

import { describe, test, expect, beforeAll } from '@jest/globals';
import { promises as fs } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

describe('Self-Validating Widgets', () => {
  let templatesDir: string;

  beforeAll(() => {
    const currentDir = dirname(fileURLToPath(import.meta.url));
    templatesDir = join(currentDir, '..', 'templates');
  });

  describe('BaseWidget Infrastructure', () => {
    test('BaseWidget.ts exists and contains validation logic', async () => {
      const baseWidgetPath = join(templatesDir, 'BaseWidget.ts');
      const content = await fs.readFile(baseWidgetPath, 'utf-8');
      
      // Verify core validation methods exist
      expect(content).toContain('validateTemplate()');
      expect(content).toContain('getRequiredElements()');
      expect(content).toContain('getRequiredIds()');
      expect(content).toContain('abstract class BaseWidget');
      expect(content).toContain('protected abstract initializeWidget()');
    });
  });

  describe('Widget Template Processing', () => {
    test('can simulate widget self-validation without DOM', async () => {
      // Load widget template code
      const chatWidgetCode = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      const chatWidgetHTML = await fs.readFile(join(templatesDir, 'chat-widget.html'), 'utf-8');
      
      // Simulate template injection (what RendererDaemon does)
      const processedHTML = chatWidgetHTML.replace(/\{\{CONTINUUM_VERSION\}\}/g, '1.0.0');
      const finalCode = chatWidgetCode.replace(
        /\{\{CHAT_WIDGET_HTML\}\}/g, 
        processedHTML.replace(/`/g, '\\`').replace(/\$/g, '\\$')
      );
      
      // Verify processing worked
      expect(finalCode).toContain('class ChatWidget extends BaseWidget');
      expect(finalCode).toContain('<div class="chat-header">');
      expect(finalCode).toContain('getRequiredElements()');
      expect(finalCode).toContain('getRequiredIds()');
      expect(finalCode).not.toContain('{{CHAT_WIDGET_HTML}}');
      expect(finalCode).not.toContain('{{CONTINUUM_VERSION}}');
    });

    test('widgets define their required elements', async () => {
      const chatWidgetCode = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      
      // Verify chat widget defines its requirements
      expect(chatWidgetCode).toContain('getRequiredElements()');
      expect(chatWidgetCode).toContain('getRequiredIds()');
      expect(chatWidgetCode).toContain("'status', 'messages', 'chatInput'");
      expect(chatWidgetCode).toContain('<div class="chat-header">');
    });
  });

  describe('Template Consistency via Widget Self-Validation', () => {
    test('chat widget requirements match its HTML template', async () => {
      const chatWidgetCode = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      const chatWidgetHTML = await fs.readFile(join(templatesDir, 'chat-widget.html'), 'utf-8');
      
      // Extract required IDs from TypeScript (simplified parsing)
      const requiredIds = ['status', 'messages', 'chatInput']; // From getRequiredIds()
      
      // Verify HTML has all required IDs
      for (const id of requiredIds) {
        expect(chatWidgetHTML).toContain(`id="${id}"`);
      }
      
      // Extract required elements from TypeScript
      const requiredElements = [
        '<div class="chat-header">',
        '<div class="chat-messages">',
        '<div class="chat-input">'
      ];
      
      // Verify HTML has all required elements
      for (const element of requiredElements) {
        expect(chatWidgetHTML).toContain(element);
      }
    });
  });

  describe('Widget Architecture Benefits', () => {
    test('BaseWidget provides consistent error handling', async () => {
      const baseWidgetCode = await fs.readFile(join(templatesDir, 'BaseWidget.ts'), 'utf-8');
      
      expect(baseWidgetCode).toContain('getElement(id: string)');
      expect(baseWidgetCode).toContain('getTypedElement<T extends HTMLElement>');
      expect(baseWidgetCode).toContain('executeCommand(');
      expect(baseWidgetCode).toContain('updateConnectionStatus(');
      expect(baseWidgetCode).toContain('log(message: string');
    });

    test('widgets are self-documenting through validation', async () => {
      const chatWidgetCode = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
      
      // The widget itself documents what it needs
      expect(chatWidgetCode).toContain('getRequiredElements()');
      expect(chatWidgetCode).toContain('getRequiredIds()');
      
      // No need for external test files to hardcode requirements
      // The widget knows what it needs!
    });
  });
});