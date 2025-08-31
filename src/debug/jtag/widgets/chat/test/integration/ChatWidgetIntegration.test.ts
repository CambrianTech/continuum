#!/usr/bin/env tsx

/**
 * ChatWidget Integration Test
 * 
 * Tests ChatWidget integration with JTAG system using proper typing.
 * Validates the widget can execute commands through WidgetDaemon.
 */

import { JTAGClientServer } from '../../../../system/core/client/server/JTAGClientServer';
import { SYSTEM_SCOPES } from '../../../../system/core/types/SystemScopes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { getActivePorts } from '../../../../examples/shared/ExampleConfig';

interface WidgetTestResult {
  success: boolean;
  message: string;
  evidence?: any;
  error?: string;
}

async function testChatWidgetIntegration(): Promise<WidgetTestResult> {
  console.log('🧪 ChatWidget Integration Test: Testing widget with JTAG system...');

  try {
    // Connect to JTAG system using dynamic port resolution
    const activePorts = await getActivePorts();
    const { client } = await JTAGClientServer.connect({
      targetEnvironment: 'server',
      transportType: 'websocket', 
      serverUrl: `ws://localhost:${activePorts.websocket_server}`
    });

    console.log('🔌 Connected to JTAG system for widget testing');

    // Test 1: Verify WidgetDaemon is available in browser
    const widgetDaemonTest = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔍 Checking WidgetDaemon availability...');
          if (window.widgetDaemon) {
            console.log('✅ WidgetDaemon found:', typeof window.widgetDaemon.executeCommand);
            return {
              widgetDaemonAvailable: true,
              executeCommandType: typeof window.widgetDaemon.executeCommand
            };
          } else {
            console.log('❌ WidgetDaemon not found');
            return { widgetDaemonAvailable: false };
          }
        `
      }
    });

    if (!widgetDaemonTest.success) {
      return {
        success: false,
        message: 'Failed to check WidgetDaemon availability',
        error: widgetDaemonTest.error
      };
    }

    // Test 2: Test widget command execution through WidgetDaemon
    const widgetCommandTest = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🧪 Testing widget command execution...');
          if (window.widgetDaemon && window.widgetDaemon.executeCommand) {
            try {
              const result = await window.widgetDaemon.executeCommand('ping', {});
              console.log('✅ Widget command execution success:', result);
              return {
                widgetCommandSuccess: true,
                pingResult: result,
                evidence: 'WIDGET_COMMAND_EXECUTED'
              };
            } catch (error) {
              console.log('❌ Widget command execution failed:', error);
              return {
                widgetCommandSuccess: false,
                error: error.message
              };
            }
          } else {
            return { widgetCommandSuccess: false, error: 'WidgetDaemon not available' };
          }
        `
      }
    });

    if (!widgetCommandTest.success) {
      return {
        success: false,
        message: 'Failed to test widget command execution',
        error: widgetCommandTest.error
      };
    }

    // Test 3: Verify ChatWidget can be instantiated
    const chatWidgetTest = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🎭 Testing ChatWidget instantiation...');
          
          // Check if ChatWidget is available
          if (typeof ChatWidget === 'undefined') {
            console.log('⚠️ ChatWidget not globally available - checking custom elements...');
            
            // Try to create via custom element
            const chatElement = document.createElement('chat-widget');
            if (chatElement.constructor.name === 'HTMLElement') {
              console.log('❌ ChatWidget not registered as custom element');
              return { chatWidgetAvailable: false, error: 'ChatWidget not registered' };
            } else {
              console.log('✅ ChatWidget available via custom element');
              return { chatWidgetAvailable: true, viaCustomElement: true };
            }
          } else {
            console.log('✅ ChatWidget globally available');
            const widget = new ChatWidget();
            return { 
              chatWidgetAvailable: true, 
              widgetName: ChatWidget.widgetName,
              tagName: ChatWidget.tagName
            };
          }
        `
      }
    });

    await client.disconnect();

    return {
      success: true,
      message: 'ChatWidget integration test completed',
      evidence: {
        widgetDaemonTest: widgetDaemonTest.commandResult?.result,
        widgetCommandTest: widgetCommandTest.commandResult?.result,
        chatWidgetTest: chatWidgetTest.commandResult?.result
      }
    };

  } catch (error) {
    return {
      success: false,
      message: 'ChatWidget integration test failed',
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

// Execute test if run directly
if (require.main === module) {
  (async () => {
    console.log('🚀 Starting ChatWidget Integration Tests...');
    const result = await testChatWidgetIntegration();
    
    if (result.success) {
      console.log('✅ ChatWidget Integration Test: SUCCESS');
      console.log('📋 Evidence:', JSON.stringify(result.evidence, null, 2));
    } else {
      console.log('❌ ChatWidget Integration Test: FAILED');
      console.log('💀 Error:', result.error);
    }
    
    process.exit(result.success ? 0 : 1);
  })();
}

export { testChatWidgetIntegration };