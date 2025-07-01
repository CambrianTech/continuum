/**
 * Widget Browser Integration Test
 * Tests widget loading and functionality in a real browser environment
 */

import puppeteer from 'puppeteer';
import { BaseWidget } from '../../shared/BaseWidget.js';
import { ChatWidget } from '../../Chat/ChatWidget.js';
import { SidebarWidget } from '../../Sidebar/SidebarWidget.js';

interface WidgetTestResult {
  widget: string;
  loaded: boolean;
  hasCSS: boolean;
  hasContent: boolean;
  interactive: boolean;
  errors: string[];
}

export class WidgetBrowserIntegrationTester {
  private browser: puppeteer.Browser | null = null;
  private page: puppeteer.Page | null = null;
  private baseUrl: string = 'http://localhost:9000';

  async initialize(): Promise<void> {
    console.log('🚀 Launching browser for widget integration testing...');
    
    this.browser = await puppeteer.launch({
      headless: false, // Show browser for debugging
      args: [
        '--no-sandbox',
        '--disable-setuid-sandbox',
        '--disable-web-security',
        '--disable-features=VizDisplayCompositor'
      ]
    });

    this.page = await this.browser.newPage();
    
    // Capture console logs
    this.page.on('console', (msg) => {
      console.log(`🌐 Browser Console [${msg.type()}]:`, msg.text());
    });

    // Capture errors
    this.page.on('error', (error) => {
      console.error('🚨 Page Error:', error);
    });

    // Capture unhandled promise rejections
    this.page.on('pageerror', (error) => {
      console.error('🚨 Page Promise Rejection:', error);
    });
  }

  async testWidgetSystem(): Promise<{
    results: WidgetTestResult[];
    overallSuccess: boolean;
  }> {
    if (!this.page) {
      throw new Error('Browser not initialized');
    }

    console.log('🧪 Loading Continuum interface...');
    
    // Navigate to the main page
    await this.page.goto(this.baseUrl, { waitUntil: 'networkidle0' });
    
    console.log('📍 Page loaded, testing widgets...');

    const results: WidgetTestResult[] = [];

    // Test each widget
    const widgetTests = [
      { name: 'continuum-sidebar', displayName: 'SidebarWidget' },
      { name: 'chat-widget', displayName: 'ChatWidget' }
    ];

    for (const widget of widgetTests) {
      const result = await this.testSingleWidget(widget.name, widget.displayName);
      results.push(result);
    }

    const overallSuccess = results.every(r => r.loaded && r.hasCSS && r.hasContent);

    console.log('\n📊 WIDGET INTEGRATION TEST SUMMARY:');
    results.forEach(result => {
      const status = result.loaded && result.hasCSS && result.hasContent ? '✅' : '❌';
      console.log(`${status} ${result.widget}:`);
      console.log(`   Loaded: ${result.loaded ? '✅' : '❌'}`);
      console.log(`   CSS Applied: ${result.hasCSS ? '✅' : '❌'}`);
      console.log(`   Has Content: ${result.hasContent ? '✅' : '❌'}`);
      console.log(`   Interactive: ${result.interactive ? '✅' : '❌'}`);
      if (result.errors.length > 0) {
        console.log(`   Errors: ${result.errors.join(', ')}`);
      }
    });

    return { results, overallSuccess };
  }

  private async testSingleWidget(tagName: string, displayName: string): Promise<WidgetTestResult> {
    if (!this.page) {
      throw new Error('Page not available');
    }

    const result: WidgetTestResult = {
      widget: displayName,
      loaded: false,
      hasCSS: false,
      hasContent: false,
      interactive: false,
      errors: []
    };

    try {
      console.log(`🔍 Testing ${displayName} (${tagName})...`);

      // Check if widget element exists
      const widgetExists = await this.page.evaluate((tag) => {
        return document.querySelector(tag) !== null;
      }, tagName);

      if (!widgetExists) {
        result.errors.push('Widget element not found in DOM');
        return result;
      }

      result.loaded = true;

      // Check if widget has shadow DOM content
      const hasContent = await this.page.evaluate((tag) => {
        const widget = document.querySelector(tag);
        if (!widget) return false;
        
        // Check if it has shadowRoot
        const shadowRoot = (widget as any).shadowRoot;
        if (!shadowRoot) return false;
        
        // Check if shadowRoot has content
        return shadowRoot.innerHTML.trim().length > 0;
      }, tagName);

      result.hasContent = hasContent;

      // Check if CSS is applied (look for styled elements)
      const hasCSS = await this.page.evaluate((tag) => {
        const widget = document.querySelector(tag);
        if (!widget) return false;
        
        const shadowRoot = (widget as any).shadowRoot;
        if (!shadowRoot) return false;
        
        // Check if there's a <style> element in shadowRoot
        const styleElement = shadowRoot.querySelector('style');
        return styleElement && styleElement.textContent && styleElement.textContent.length > 100;
      }, tagName);

      result.hasCSS = hasCSS;

      // Test basic interactivity (for sidebar)
      if (tagName === 'continuum-sidebar') {
        const isInteractive = await this.testSidebarInteractivity();
        result.interactive = isInteractive;
      }

      // Test basic interactivity (for chat)
      if (tagName === 'chat-widget') {
        const isInteractive = await this.testChatInteractivity();
        result.interactive = isInteractive;
      }

    } catch (error) {
      result.errors.push(`Test failed: ${error instanceof Error ? error.message : String(error)}`);
    }

    return result;
  }

  private async testSidebarInteractivity(): Promise<boolean> {
    if (!this.page) return false;

    try {
      // Test if room tabs are clickable
      const hasRoomTabs = await this.page.evaluate(() => {
        const sidebar = document.querySelector('continuum-sidebar');
        if (!sidebar) return false;
        
        const shadowRoot = (sidebar as any).shadowRoot;
        if (!shadowRoot) return false;
        
        const roomTabs = shadowRoot.querySelectorAll('.room-tab');
        return roomTabs.length > 0;
      });

      return hasRoomTabs;
    } catch (error) {
      console.warn('Sidebar interactivity test failed:', error);
      return false;
    }
  }

  private async testChatInteractivity(): Promise<boolean> {
    if (!this.page) return false;

    try {
      // Test if chat input exists and is functional
      const hasInput = await this.page.evaluate(() => {
        const chat = document.querySelector('chat-widget');
        if (!chat) return false;
        
        const shadowRoot = (chat as any).shadowRoot;
        if (!shadowRoot) return false;
        
        const input = shadowRoot.querySelector('#messageInput');
        return input !== null;
      });

      return hasInput;
    } catch (error) {
      console.warn('Chat interactivity test failed:', error);
      return false;
    }
  }

  async cleanup(): Promise<void> {
    if (this.browser) {
      await this.browser.close();
      console.log('🧹 Browser closed');
    }
  }

  // Screenshot capability for visual verification
  async takeScreenshot(filename: string = 'widget-test-screenshot.png'): Promise<void> {
    if (!this.page) return;
    
    await this.page.screenshot({ 
      path: filename, 
      fullPage: true 
    });
    console.log(`📸 Screenshot saved: ${filename}`);
  }
}

// Jest-compatible test function
export async function runWidgetBrowserIntegrationTest(): Promise<void> {
  const tester = new WidgetBrowserIntegrationTester();
  
  try {
    await tester.initialize();
    const results = await tester.testWidgetSystem();
    
    // Take screenshot for visual verification
    await tester.takeScreenshot('widget-integration-test.png');
    
    if (!results.overallSuccess) {
      const failedWidgets = results.results.filter(r => !r.loaded || !r.hasCSS || !r.hasContent);
      throw new Error(`Widget integration test failed. Failed widgets: ${failedWidgets.map(w => w.widget).join(', ')}`);
    }
    
    console.log('🎉 All widget integration tests passed!');
    
  } finally {
    await tester.cleanup();
  }
}

// Stand-alone execution
if (import.meta.url === `file://${process.argv[1]}`) {
  runWidgetBrowserIntegrationTest().catch(console.error);
}