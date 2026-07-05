/**
 * Browser UI Test Helpers - Reusable patterns for browser UI testing
 * 
 * Uses exec() commands to control browser UI from server test clients.
 * This pattern will be commonly used across many test scenarios.
 */

import type { JTAGClient } from '../../system/core/client/shared/JTAGClient';

export interface BrowserUIInteraction {
  selector: string;
  action: 'click' | 'type' | 'getValue' | 'waitFor';
  value?: string;
  timeout?: number;
}

export interface BrowserUIResult {
  success: boolean;
  value?: any;
  error?: string;
}

/**
 * Execute a browser UI interaction via exec() command
 */
export async function interactWithBrowserUI(
  client: JTAGClient,
  interaction: BrowserUIInteraction
): Promise<BrowserUIResult> {
  const { selector, action, value, timeout = 5000 } = interaction;
  
  const script = `
    console.log('🌐 BROWSER UI: ${action} on ${selector}');
    
    try {
      const element = document.querySelector('${selector.replace(/'/g, "\\'")}');
      if (!element) {
        console.log('❌ BROWSER UI: Element not found: ${selector}');
        return { success: false, error: 'Element not found: ${selector}' };
      }
      
      switch ('${action}') {
        case 'click':
          element.click();
          console.log('✅ BROWSER UI: Clicked ${selector}');
          return { success: true };
          
        case 'type':
          if (element.tagName === 'INPUT' || element.tagName === 'TEXTAREA') {
            element.value = '${value ? value.replace(/'/g, "\\'") : ''}';
            element.dispatchEvent(new Event('input', { bubbles: true }));
            element.dispatchEvent(new Event('change', { bubbles: true }));
            console.log('✅ BROWSER UI: Typed into ${selector}');
            return { success: true };
          } else {
            element.textContent = '${value ? value.replace(/'/g, "\\'") : ''}';
            console.log('✅ BROWSER UI: Set text content of ${selector}');
            return { success: true };
          }
          
        case 'getValue':
          const value = element.value || element.textContent || element.innerText;
          console.log('✅ BROWSER UI: Got value from ${selector}:', value);
          return { success: true, value };
          
        case 'waitFor':
          // Element exists, so wait condition is met
          console.log('✅ BROWSER UI: Element ${selector} found (wait condition met)');
          return { success: true };
          
        default:
          console.log('❌ BROWSER UI: Unknown action: ${action}');
          return { success: false, error: 'Unknown action: ${action}' };
      }
    } catch (error) {
      console.log('❌ BROWSER UI: Error in ${action} on ${selector}:', error.message);
      return { success: false, error: error.message };
    }
  `;

  try {
    const result = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: script
      }
    });

    if (result.success && result.commandResult?.success) {
      const execResult = result.commandResult.result || result.commandResult.commandResult;
      return execResult || { success: true };
    } else {
      return { success: false, error: 'Exec command failed' };
    }
  } catch (error) {
    return { success: false, error: String(error) };
  }
}

/**
 * Send a chat message through browser UI
 */
export async function sendChatMessageThroughUI(
  client: JTAGClient,
  message: string,
  senderName?: string
): Promise<BrowserUIResult> {
  console.log(`💬 BROWSER UI: Sending chat message${senderName ? ` as ${senderName}` : ''}`);
  
  // Type message into chat input
  const typeResult = await interactWithBrowserUI(client, {
    selector: 'chat-widget input[type="text"], .chat-input, #chat-input, input[placeholder*="message" i]',
    action: 'type',
    value: senderName ? `${message} [${senderName}]` : message
  });

  if (!typeResult.success) {
    return typeResult;
  }

  // Click send button
  const clickResult = await interactWithBrowserUI(client, {
    selector: 'chat-widget button[type="submit"], .chat-send-btn, #chat-send, button:contains("Send")',
    action: 'click'
  });

  if (clickResult.success) {
    console.log('✅ BROWSER UI: Chat message sent successfully');
  }

  return clickResult;
}

/**
 * Take screenshot of specific UI component
 */
export async function screenshotUIComponent(
  client: JTAGClient,
  selector: string,
  filename?: string
): Promise<BrowserUIResult> {
  try {
    const result = await client.commands.screenshot({
      querySelector: selector,
      filename: filename || `ui-component-${Date.now()}.png`
    });

    if (result.success) {
      console.log(`📸 BROWSER UI: Screenshot taken of ${selector}`);
      return { success: true };
    } else {
      return { success: false, error: 'Screenshot failed' };
    }
  } catch (error) {
    return { success: false, error: String(error) };
  }
}

/**
 * Wait for element to appear in browser UI
 */
export async function waitForUIElement(
  client: JTAGClient,
  selector: string,
  timeout: number = 10000
): Promise<BrowserUIResult> {
  console.log(`⏳ BROWSER UI: Waiting for ${selector} (timeout: ${timeout}ms)`);
  
  const startTime = Date.now();
  
  while (Date.now() - startTime < timeout) {
    const result = await interactWithBrowserUI(client, {
      selector,
      action: 'waitFor'
    });
    
    if (result.success) {
      return result;
    }
    
    // Wait 500ms before checking again
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  
  return { success: false, error: `Timeout waiting for ${selector}` };
}

/**
 * Verify chat message appears in UI
 */
export async function verifyChatMessageInUI(
  client: JTAGClient,
  messageContent: string,
  senderName?: string
): Promise<BrowserUIResult> {
  console.log(`🔍 BROWSER UI: Verifying message appears: "${messageContent}"`);
  
  const script = `
    console.log('🔍 BROWSER UI: Searching for chat message...');
    
    try {
      // Look for chat messages in various possible containers
      const messageContainers = document.querySelectorAll([
        'chat-widget .message', 
        '.chat-message', 
        '.message', 
        '[class*="message"]',
        '.chat-container div',
        'chat-widget div'
      ].join(', '));
      
      const searchText = '${messageContent.replace(/'/g, "\\'")}';
      const searchSender = '${senderName ? senderName.replace(/'/g, "\\'") : ''}';
      
      let found = false;
      let matchedElement = null;
      
      for (const container of messageContainers) {
        const text = container.textContent || container.innerText || '';
        
        if (text.includes(searchText)) {
          if (!searchSender || text.includes(searchSender)) {
            found = true;
            matchedElement = container;
            break;
          }
        }
      }
      
      if (found) {
        console.log('✅ BROWSER UI: Message found in chat UI');
        return { success: true, value: matchedElement?.textContent };
      } else {
        console.log('❌ BROWSER UI: Message not found in chat UI');
        console.log('Available messages:', Array.from(messageContainers).map(el => el.textContent).slice(0, 5));
        return { success: false, error: 'Message not found in UI' };
      }
    } catch (error) {
      console.log('❌ BROWSER UI: Error verifying message:', error.message);
      return { success: false, error: error.message };
    }
  `;

  try {
    const result = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript', 
        source: script
      }
    });

    if (result.success && result.commandResult?.success) {
      const execResult = result.commandResult.result || result.commandResult.commandResult;
      return execResult || { success: false, error: 'No result from verification' };
    } else {
      return { success: false, error: 'Exec command failed' };
    }
  } catch (error) {
    return { success: false, error: String(error) };
  }
}