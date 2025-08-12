/**
 * Browser Exec Command Test
 * Tests that exec command actually executes in browser and gets browser-specific data
 * Follows DEV-PROCESS.md methodology
 */

import { describe, it, expect, beforeAll } from '@jest/globals';

describe('ExecCommand Browser Execution', () => {
  let jtagClient: any;
  
  beforeAll(async () => {
    // Import JTAG client - this will only work if we're testing from the right environment
    const { jtag } = await import('@continuum/jtag');
    
    console.log('🔗 Connecting to JTAG browser system...');
    jtagClient = await jtag.connect();
    
    expect(jtagClient).toBeDefined();
    expect(jtagClient.commands).toBeDefined();
    expect(jtagClient.commands.exec).toBeDefined();
  });

  it('should execute code in browser and get user agent', async () => {
    console.log('🌐 Testing browser exec - user agent...');
    
    const result = await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔥 BROWSER EXEC: Getting user agent');
          const userAgent = navigator.userAgent;
          console.log('🔥 BROWSER EXEC: User agent is:', userAgent);
          userAgent;
        `
      }
    });
    
    console.log('📊 User agent result:', JSON.stringify(result, null, 2));
    
    expect(result).toBeDefined();
    expect(result.success).toBe(true);
    expect(result.result).toBeDefined();
    expect(typeof result.result).toBe('string');
    expect(result.result).toContain('Mozilla'); // All browsers have Mozilla in user agent
  });

  it('should execute code in browser and get window dimensions', async () => {
    console.log('🌐 Testing browser exec - window dimensions...');
    
    const result = await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔥 BROWSER EXEC: Getting window dimensions');
          const dimensions = {
            innerWidth: window.innerWidth,
            innerHeight: window.innerHeight,
            outerWidth: window.outerWidth,
            outerHeight: window.outerHeight,
            screenWidth: screen.width,
            screenHeight: screen.height
          };
          console.log('🔥 BROWSER EXEC: Window dimensions:', dimensions);
          dimensions;
        `
      }
    });
    
    console.log('📊 Window dimensions result:', JSON.stringify(result, null, 2));
    
    expect(result).toBeDefined();
    expect(result.success).toBe(true);
    expect(result.result).toBeDefined();
    expect(typeof result.result).toBe('object');
    expect(result.result.innerWidth).toBeGreaterThan(0);
    expect(result.result.innerHeight).toBeGreaterThan(0);
  });

  it('should execute code in browser and manipulate DOM', async () => {
    console.log('🌐 Testing browser exec - DOM manipulation...');
    
    const result = await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔥 BROWSER EXEC: DOM manipulation test');
          const title = document.title;
          const bodyTag = document.body ? 'body exists' : 'no body';
          const testDiv = document.createElement('div');
          testDiv.id = 'exec-test-proof';
          testDiv.textContent = 'EXEC COMMAND WORKED';
          document.body.appendChild(testDiv);
          
          const proof = {
            title: title,
            bodyExists: bodyTag,
            testDivCreated: !!document.getElementById('exec-test-proof'),
            url: window.location.href
          };
          console.log('🔥 BROWSER EXEC: DOM proof:', proof);
          proof;
        `
      }
    });
    
    console.log('📊 DOM manipulation result:', JSON.stringify(result, null, 2));
    
    expect(result).toBeDefined();
    expect(result.success).toBe(true);
    expect(result.result).toBeDefined();
    expect(result.result.testDivCreated).toBe(true);
    expect(result.result.url).toContain('localhost');
  });
});