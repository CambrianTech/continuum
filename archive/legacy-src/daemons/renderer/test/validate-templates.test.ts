/**
 * Layer 1 Template Validation Tests - TypeScript
 * Middle-Out Testing: Dynamic template discovery and validation
 */

import { describe, test, expect, beforeAll } from '@jest/globals';
import { promises as fs } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

interface TemplateDiscovery {
  widgets: string[];
  html: string[];
  typescript: string[];
  loaders: string[];
}

interface WidgetRequirements {
  html: {
    universal: string[];
    specific: Record<string, string[]>;
  };
  typescript: {
    universal: string[];
    specific: Record<string, string[]>;
  };
}

const WIDGET_REQUIREMENTS: WidgetRequirements = {
  html: {
    universal: ['<style>', ':host {', '{{CONTINUUM_VERSION}}'],
    specific: {
      chat: [
        '<div class="chat-header">',
        '<div class="chat-messages">',
        '<div class="chat-input">',
        'id="status"',
        'id="messages"',
        'id="chatInput"'
      ],
      sidebar: [
        '<div class="sidebar-header">',
        '<div class="sidebar-content">',
        'id="version"',
        'id="ws-status"',
        'id="cmd-status"'
      ]
    }
  },
  typescript: {
    universal: [
      'extends HTMLElement',
      'private shadowRoot: ShadowRoot',
      'private api?: ContinuumAPI',
      'constructor()',
      'connectedCallback()',
      'private render()',
      'private initializeWidget()'
    ],
    specific: {
      chat: [
        'class ChatWidget extends HTMLElement',
        '{{CHAT_WIDGET_HTML}}',
        'private setupConnection(',
        'private addMessage(',
        'as HTMLInputElement'
      ],
      sidebar: [
        'class ContinuumSidebar extends HTMLElement',
        '{{SIDEBAR_WIDGET_HTML}}',
        'private setupStatusUpdates(',
        'private async testCommandDiscovery('
      ]
    }
  }
};

describe('Layer 1: Template Validation - Dynamic Discovery', () => {
  let templatesDir: string;
  let discoveredTemplates: TemplateDiscovery;

  beforeAll(async () => {
    const currentDir = dirname(fileURLToPath(import.meta.url));
    templatesDir = join(currentDir, '..', 'templates');
    discoveredTemplates = await discoverTemplates(templatesDir);
  });

  async function discoverTemplates(dir: string): Promise<TemplateDiscovery> {
    const templates: TemplateDiscovery = {
      widgets: [],
      html: [],
      typescript: [],
      loaders: []
    };

    try {
      const files = await fs.readdir(dir);
      
      for (const file of files) {
        const filePath = join(dir, file);
        const stats = await fs.stat(filePath);
        
        if (stats.isFile()) {
          if (file.endsWith('.html')) {
            templates.html.push(file);
            const widgetName = file.replace('.html', '');
            if (widgetName.includes('widget')) {
              templates.widgets.push(widgetName);
            }
          } else if (file.endsWith('.ts')) {
            templates.typescript.push(file);
            if (file.includes('loader')) {
              templates.loaders.push(file);
            }
          }
        }
      }
    } catch (error) {
      console.error(`Failed to discover templates: ${error}`);
    }

    return templates;
  }

  describe('Template Discovery', () => {
    test('should discover widget templates', () => {
      expect(discoveredTemplates.widgets.length).toBeGreaterThan(0);
      expect(discoveredTemplates.html.length).toBeGreaterThan(0);
      expect(discoveredTemplates.typescript.length).toBeGreaterThan(0);
      expect(discoveredTemplates.loaders.length).toBeGreaterThan(0);
    });

    test('should find matching HTML and TypeScript for each widget', () => {
      for (const widget of discoveredTemplates.widgets) {
        const htmlFile = `${widget}.html`;
        const tsFile = `${widget}.ts`;
        
        expect(discoveredTemplates.html).toContain(htmlFile);
        expect(discoveredTemplates.typescript).toContain(tsFile);
      }
    });
  });

  describe('Template File Validation', () => {
    test.each(discoveredTemplates.html)('%s should exist and be readable', async (htmlFile) => {
      const filePath = join(templatesDir, htmlFile);
      await expect(fs.access(filePath, fs.constants.R_OK)).resolves.not.toThrow();
      
      const stats = await fs.stat(filePath);
      expect(stats.isFile()).toBe(true);
      expect(stats.size).toBeGreaterThan(0);
    });

    test.each(discoveredTemplates.typescript)('%s should exist and be readable', async (tsFile) => {
      const filePath = join(templatesDir, tsFile);
      await expect(fs.access(filePath, fs.constants.R_OK)).resolves.not.toThrow();
      
      const stats = await fs.stat(filePath);
      expect(stats.isFile()).toBe(true);
      expect(stats.size).toBeGreaterThan(0);
    });
  });

  describe('HTML Template Structure', () => {
    test.each(discoveredTemplates.html)('%s should contain universal HTML requirements', async (htmlFile) => {
      const content = await fs.readFile(join(templatesDir, htmlFile), 'utf-8');
      
      for (const requirement of WIDGET_REQUIREMENTS.html.universal) {
        expect(content).toContain(requirement);
      }
    });

    test.each(discoveredTemplates.widgets)('%s should contain widget-specific HTML requirements', async (widget) => {
      const htmlFile = `${widget}.html`;
      const content = await fs.readFile(join(templatesDir, htmlFile), 'utf-8');
      
      // Determine widget type and check specific requirements
      for (const [type, requirements] of Object.entries(WIDGET_REQUIREMENTS.html.specific)) {
        if (widget.includes(type)) {
          for (const requirement of requirements) {
            expect(content).toContain(requirement);
          }
        }
      }
    });
  });

  describe('TypeScript Template Structure', () => {
    test.each(discoveredTemplates.typescript.filter(f => f.includes('widget') && !f.includes('loader')))('%s should contain universal TypeScript requirements', async (tsFile) => {
      const content = await fs.readFile(join(templatesDir, tsFile), 'utf-8');
      
      for (const requirement of WIDGET_REQUIREMENTS.typescript.universal) {
        expect(content).toContain(requirement);
      }
    });

    test.each(discoveredTemplates.widgets)('%s should contain widget-specific TypeScript requirements', async (widget) => {
      const tsFile = `${widget}.ts`;
      const content = await fs.readFile(join(templatesDir, tsFile), 'utf-8');
      
      // Determine widget type and check specific requirements  
      for (const [type, requirements] of Object.entries(WIDGET_REQUIREMENTS.typescript.specific)) {
        if (widget.includes(type)) {
          for (const requirement of requirements) {
            expect(content).toContain(requirement);
          }
        }
      }
    });

    test.each(discoveredTemplates.loaders)('%s should contain loader requirements', async (loaderFile) => {
      const content = await fs.readFile(join(templatesDir, loaderFile), 'utf-8');
      
      expect(content).toContain('interface ContinuumAPI');
      expect(content).toContain('declare global');
      expect(content).toContain('customElements.define');
      expect(content).toContain('DOMContentLoaded');
      expect(content).toContain('export default');
    });
  });

  describe('Template Consistency', () => {
    test('should have matching placeholders between HTML and TypeScript', async () => {
      for (const widget of discoveredTemplates.widgets) {
        const htmlContent = await fs.readFile(join(templatesDir, `${widget}.html`), 'utf-8');
        const tsContent = await fs.readFile(join(templatesDir, `${widget}.ts`), 'utf-8');
        
        expect(htmlContent).toContain('{{CONTINUUM_VERSION}}');
        
        if (widget.includes('chat')) {
          expect(tsContent).toContain('{{CHAT_WIDGET_HTML}}');
        } else if (widget.includes('sidebar')) {
          expect(tsContent).toContain('{{SIDEBAR_WIDGET_HTML}}');
        }
      }
    });

    test('should have matching IDs between HTML and TypeScript', async () => {
      for (const widget of discoveredTemplates.widgets) {
        const htmlContent = await fs.readFile(join(templatesDir, `${widget}.html`), 'utf-8');
        const tsContent = await fs.readFile(join(templatesDir, `${widget}.ts`), 'utf-8');
        
        // Extract IDs from HTML
        const idMatches = htmlContent.matchAll(/id="([^"]+)"/g);
        const htmlIds = Array.from(idMatches, match => match[1]);
        
        // Verify TypeScript references each ID
        for (const id of htmlIds) {
          expect(tsContent).toContain(`getElementById('${id}')`);
        }
      }
    });
  });

  describe('Template Security', () => {
    test.each(discoveredTemplates.html)('%s should not contain security vulnerabilities', async (htmlFile) => {
      const content = await fs.readFile(join(templatesDir, htmlFile), 'utf-8');
      
      expect(content).not.toMatch(/<script/i);
      expect(content).not.toMatch(/javascript:/i);
      expect(content).not.toMatch(/on\w+=/i); // onclick, onload, etc.
    });

    test.each(discoveredTemplates.typescript)('%s should use proper TypeScript typing', async (tsFile) => {
      const content = await fs.readFile(join(templatesDir, tsFile), 'utf-8');
      
      expect(content).not.toMatch(/:\s*any/); // No 'any' types
      expect(content).toMatch(/as HTML\w*Element/); // Proper type assertions
    });
  });
});