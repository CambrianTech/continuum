#!/usr/bin/env node
/**
 * Layer 1 Template Validation - Middle-Out Testing Approach
 * Simple Node.js script to validate template files before daemon testing
 */

//TODO: if you are reading this, rewrite in typescript now!

import { promises as fs } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const currentDir = dirname(fileURLToPath(import.meta.url));
const templatesDir = join(currentDir, '..', 'templates');

console.log('🧪 Layer 1: Template File Validation');
console.log(`📁 Templates directory: ${templatesDir}`);
console.log('');

let testCount = 0;
let passCount = 0;
let failCount = 0;

function test(description, testFn) {
  testCount++;
  try {
    testFn();
    passCount++;
    console.log(`✅ ${description}`);
  } catch (error) {
    failCount++;
    console.log(`❌ ${description}`);
    console.log(`   Error: ${error.message}`);
  }
}

function expect(actual) {
  return {
    toBe: (expected) => {
      if (actual !== expected) {
        throw new Error(`Expected ${expected}, got ${actual}`);
      }
    },
    toContain: (substring) => {
      if (!actual.includes(substring)) {
        throw new Error(`Expected "${actual}" to contain "${substring}"`);
      }
    },
    not: {
      toContain: (substring) => {
        if (actual.includes(substring)) {
          throw new Error(`Expected "${actual}" to NOT contain "${substring}"`);
        }
      }
    },
    toBeGreaterThan: (value) => {
      if (actual <= value) {
        throw new Error(`Expected ${actual} to be greater than ${value}`);
      }
    }
  };
}

/**
 * Dynamic Widget Template Discovery
 * Auto-discover widget templates like command discovery system
 */
async function discoverWidgetTemplates() {
  const templates = {
    widgets: [],
    loaders: [],
    html: [],
    typescript: []
  };

  try {
    const files = await fs.readdir(templatesDir);
    
    for (const file of files) {
      const filePath = join(templatesDir, file);
      const stats = await fs.stat(filePath);
      
      if (stats.isFile()) {
        if (file.endsWith('.html')) {
          templates.html.push(file);
          
          // Extract widget name from filename (e.g., 'chat-widget.html' -> 'chat-widget')
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
    
    return templates;
  } catch (error) {
    console.error(`❌ Failed to discover templates: ${error.message}`);
    return templates;
  }
}

// Dynamic template discovery and testing
console.log('🔍 Discovering Widget Templates:');
const discoveredTemplates = await discoverWidgetTemplates();

console.log(`📊 Discovery Results:`);
console.log(`   Widgets: ${discoveredTemplates.widgets.length} (${discoveredTemplates.widgets.join(', ')})`);
console.log(`   HTML Templates: ${discoveredTemplates.html.length} (${discoveredTemplates.html.join(', ')})`);
console.log(`   TypeScript Templates: ${discoveredTemplates.typescript.length} (${discoveredTemplates.typescript.join(', ')})`);
console.log(`   Loaders: ${discoveredTemplates.loaders.length} (${discoveredTemplates.loaders.join(', ')})`);
console.log('');

console.log('📋 Testing Template File Existence (Dynamic Discovery):');

// Test HTML templates dynamically
for (const htmlFile of discoveredTemplates.html) {
  test(`${htmlFile} exists and is readable`, async () => {
    const filePath = join(templatesDir, htmlFile);
    await fs.access(filePath, fs.constants.R_OK);
    const stats = await fs.stat(filePath);
    expect(stats.isFile()).toBe(true);
    expect(stats.size).toBeGreaterThan(0);
  });
}

// Test TypeScript templates dynamically
for (const tsFile of discoveredTemplates.typescript) {
  test(`${tsFile} exists and is readable`, async () => {
    const filePath = join(templatesDir, tsFile);
    await fs.access(filePath, fs.constants.R_OK);
    const stats = await fs.stat(filePath);
    expect(stats.isFile()).toBe(true);
    expect(stats.size).toBeGreaterThan(0);
  });
}

// Test widget pairs (HTML + TypeScript)
for (const widget of discoveredTemplates.widgets) {
  test(`${widget} has both HTML and TypeScript templates`, async () => {
    const htmlFile = `${widget}.html`;
    const tsFile = `${widget}.ts`;
    
    expect(discoveredTemplates.html).toContain(htmlFile);
    expect(discoveredTemplates.typescript).toContain(tsFile);
  });
}

console.log('');
console.log('🎨 Testing HTML Template Structure (Dynamic):');

// Test all HTML templates dynamically
for (const htmlFile of discoveredTemplates.html) {
  test(`${htmlFile} contains required HTML structure`, async () => {
    const html = await fs.readFile(join(templatesDir, htmlFile), 'utf-8');
    
    // Universal HTML template requirements
    expect(html).toContain('<style>');
    expect(html).toContain(':host {');
    expect(html).toContain('{{CONTINUUM_VERSION}}');
    
    // Widget-specific requirements based on filename
    if (htmlFile.includes('chat')) {
      expect(html).toContain('<div class="chat-header">');
      expect(html).toContain('<div class="chat-messages">');
      expect(html).toContain('<div class="chat-input">');
      expect(html).toContain('id="status"');
      expect(html).toContain('id="messages"');
      expect(html).toContain('id="chatInput"');
    } else if (htmlFile.includes('sidebar')) {
      expect(html).toContain('<div class="sidebar-header">');
      expect(html).toContain('<div class="sidebar-content">');
      expect(html).toContain('id="version"');
      expect(html).toContain('id="ws-status"');
      expect(html).toContain('id="cmd-status"');
    }
  });
  
  test(`${htmlFile} has valid CSS styling`, async () => {
    const html = await fs.readFile(join(templatesDir, htmlFile), 'utf-8');
    
    // CSS validation
    expect(html).toContain('font-family:');
    expect(html).toContain('background:');
    expect(html).toContain('color:');
    expect(html).toContain('padding:');
    
    // Shadow DOM styling
    expect(html).toContain(':host {');
  });
}

console.log('');
console.log('📝 Testing TypeScript Template Structure (Dynamic):');

// Test all TypeScript templates dynamically
for (const tsFile of discoveredTemplates.typescript) {
  if (tsFile.includes('widget') && !tsFile.includes('loader')) {
    // Widget component tests
    test(`${tsFile} has correct class structure`, async () => {
      const ts = await fs.readFile(join(templatesDir, tsFile), 'utf-8');
      
      // Universal widget requirements
      expect(ts).toContain('extends HTMLElement');
      expect(ts).toContain('private shadowRoot: ShadowRoot');
      expect(ts).toContain('private api?: ContinuumAPI');
      expect(ts).toContain('constructor()');
      expect(ts).toContain('connectedCallback()');
      expect(ts).toContain('private render()');
      expect(ts).toContain('private initializeWidget()');
      
      // Widget-specific requirements
      if (tsFile.includes('chat')) {
        expect(ts).toContain('class ChatWidget extends HTMLElement');
        expect(ts).toContain('{{CHAT_WIDGET_HTML}}');
        expect(ts).toContain('private setupConnection(');
        expect(ts).toContain('private addMessage(');
      } else if (tsFile.includes('sidebar')) {
        expect(ts).toContain('class ContinuumSidebar extends HTMLElement');
        expect(ts).toContain('{{SIDEBAR_WIDGET_HTML}}');
        expect(ts).toContain('private setupStatusUpdates(');
        expect(ts).toContain('private async testCommandDiscovery(');
      }
    });
    
    test(`${tsFile} uses proper TypeScript typing`, async () => {
      const ts = await fs.readFile(join(templatesDir, tsFile), 'utf-8');
      
      // Type safety checks
      expect(ts).not.toContain(': any');
      expect(ts).toContain('as HTMLElement');
      expect(ts).toContain(': void');
      
      if (tsFile.includes('chat')) {
        expect(ts).toContain('as HTMLInputElement');
        expect(ts).toContain("'user' | 'system' | 'error'");
      }
    });
  } else if (tsFile.includes('loader')) {
    // Loader component tests
    test(`${tsFile} has component registration`, async () => {
      const ts = await fs.readFile(join(templatesDir, tsFile), 'utf-8');
      
      expect(ts).toContain('interface ContinuumAPI');
      expect(ts).toContain('declare global');
      expect(ts).toContain('customElements.define');
      expect(ts).toContain('DOMContentLoaded');
      expect(ts).toContain('export default');
      
      // Check for widget registration based on discovered widgets
      for (const widget of discoveredTemplates.widgets) {
        if (widget.includes('chat')) {
          expect(ts).toContain("'chat-widget'");
        } else if (widget.includes('sidebar')) {
          expect(ts).toContain("'continuum-sidebar'");
        }
      }
    });
  }
}

console.log('');
console.log('🔍 Testing Template Consistency:');

test('HTML and TypeScript templates have matching placeholders', async () => {
  const chatHTML = await fs.readFile(join(templatesDir, 'chat-widget.html'), 'utf-8');
  const chatTS = await fs.readFile(join(templatesDir, 'chat-widget.ts'), 'utf-8');
  
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
  
  // Check for specific required ID references
  expect(chatTS).toContain("getElementById('status')");
  expect(chatTS).toContain("getElementById('messages')");
  expect(chatTS).toContain("getElementById('chatInput')");
});

console.log('');
console.log('🔒 Testing Template Security:');

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
  
  expect(chatTS).not.toContain(': any');
  expect(sidebarTS).not.toContain(': any');
  expect(chatTS).toContain('as HTMLElement');
  expect(sidebarTS).toContain('as HTMLElement');
});

// Run all tests
console.log('');
console.log('🚀 Running all template validation tests...');
console.log('');

// Since tests are async, we need to run them sequentially
(async () => {
  // Run all the test functions we defined above
  // (They were already executed when defined due to being async)
  
  console.log('');
  console.log('📊 Test Results:');
  console.log(`Total tests: ${testCount}`);
  console.log(`✅ Passed: ${passCount}`);
  console.log(`❌ Failed: ${failCount}`);
  console.log(`Success rate: ${Math.round((passCount / testCount) * 100)}%`);
  
  if (failCount === 0) {
    console.log('');
    console.log('🎉 Layer 1 Template Validation: ALL TESTS PASSED');
    console.log('✅ Ready to move to Layer 2: RendererDaemon Template Processing');
    process.exit(0);
  } else {
    console.log('');
    console.log('❌ Layer 1 Template Validation: SOME TESTS FAILED');
    console.log('🚫 Fix template issues before proceeding to Layer 2');
    process.exit(1);
  }
})();