/**
 * ExecCommand Browser Proof Test
 * Following DEV-PROCESS.md methodology
 * Tests exec command with browser-specific evidence that cannot be faked
 */

import { describe, it, expect, beforeAll } from '@jest/globals';
import { ExecBrowserCommand } from '../browser/ExecBrowserCommand';
import type { ExecCommandParams } from '../shared/ExecTypes';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';

describe('ExecBrowserCommand Proof Tests', () => {
  let command: ExecBrowserCommand;
  let mockContext: JTAGContext;
  let mockCommander: any;

  beforeAll(() => {
    // Setup mock context for browser environment
    mockContext = {
      uuid: 'test-browser-context',
      environment: 'browser' as const
    };
    
    // Mock commander
    mockCommander = {
      sendMessage: jest.fn(),
      getName: () => 'test-commander'
    };
    
    command = new ExecBrowserCommand(mockContext, 'exec', mockCommander);
  });

  it('should execute JavaScript and get browser user agent', async () => {
    console.log('🌐 Testing browser exec - user agent proof...');
    
    const params: ExecCommandParams = {
      context: mockContext,
      sessionId: 'test-session',
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          // Browser-specific proof that cannot be faked in server context
          console.log('🔥 EXEC PROOF: Getting navigator.userAgent');
          const userAgent = navigator.userAgent;
          console.log('🔥 EXEC PROOF: User agent:', userAgent.substring(0, 50));
          userAgent;
        `
      }
    };
    
    const result = await command.execute(params);
    
    console.log('📊 User agent result:', JSON.stringify(result, null, 2));
    
    expect(result).toBeDefined();
    expect(result.success).toBe(true);
    expect(result.result).toBeDefined();
    expect(typeof result.result).toBe('string');
    expect(result.result).toContain('Mozilla'); // All browsers have Mozilla in user agent
    expect(result.environment).toBe('browser');
  });

  it('should execute JavaScript and get window dimensions', async () => {
    console.log('🌐 Testing browser exec - window dimensions proof...');
    
    const params: ExecCommandParams = {
      context: mockContext,
      sessionId: 'test-session',
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔥 EXEC PROOF: Getting window dimensions');
          const dimensions = {
            innerWidth: window.innerWidth,
            innerHeight: window.innerHeight,
            screen: {
              width: screen.width,
              height: screen.height
            }
          };
          console.log('🔥 EXEC PROOF: Dimensions:', dimensions);
          dimensions;
        `
      }
    };
    
    const result = await command.execute(params);
    
    console.log('📊 Window dimensions result:', JSON.stringify(result, null, 2));
    
    expect(result).toBeDefined();
    expect(result.success).toBe(true);
    expect(result.result).toBeDefined();
    expect(typeof result.result).toBe('object');
    expect(result.result.innerWidth).toBeGreaterThan(0);
    expect(result.result.innerHeight).toBeGreaterThan(0);
    expect(result.result.screen).toBeDefined();
  });

  it('should execute JavaScript and manipulate DOM', async () => {
    console.log('🌐 Testing browser exec - DOM manipulation proof...');
    
    const params: ExecCommandParams = {
      context: mockContext,  
      sessionId: 'test-session',
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔥 EXEC PROOF: DOM manipulation test');
          
          // Create a test element
          const testDiv = document.createElement('div');
          testDiv.id = 'exec-proof-test';
          testDiv.textContent = 'EXEC COMMAND WORKS';
          testDiv.style.color = 'red';
          
          // This will only work in browser context
          document.body.appendChild(testDiv);
          
          const proof = {
            title: document.title,
            elementCreated: !!document.getElementById('exec-proof-test'),
            elementText: document.getElementById('exec-proof-test')?.textContent,
            url: window.location.href,
            timestamp: Date.now()
          };
          
          console.log('🔥 EXEC PROOF: DOM proof:', proof);
          proof;
        `
      }
    };
    
    const result = await command.execute(params);
    
    console.log('📊 DOM manipulation result:', JSON.stringify(result, null, 2));
    
    expect(result).toBeDefined();
    expect(result.success).toBe(true);
    expect(result.result).toBeDefined();
    expect(result.result.elementCreated).toBe(true);
    expect(result.result.elementText).toBe('EXEC COMMAND WORKS');
    expect(result.result.url).toBeDefined();
  });

  it('should handle exec command errors properly', async () => {
    console.log('🌐 Testing browser exec - error handling...');
    
    const params: ExecCommandParams = {
      context: mockContext,
      sessionId: 'test-session', 
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔥 EXEC ERROR TEST: Intentional error');
          throw new Error('Test error for exec command');
        `
      }
    };
    
    const result = await command.execute(params);
    
    console.log('📊 Error handling result:', JSON.stringify(result, null, 2));
    
    expect(result).toBeDefined();
    expect(result.success).toBe(false);
    expect(result.error).toBeDefined();
    expect(result.error.message).toContain('Test error for exec command');
    expect(result.environment).toBe('browser');
  });
});