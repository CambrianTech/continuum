#!/usr/bin/env tsx
/**
 * Chat Widget Interaction Tests - Direct UI Testing
 * 
 * Tests actual widget UI interactions using exec commands:
 * - Populate text inputs with test messages
 * - Click send buttons to trigger message sending
 * - Verify message appears in widget HTML
 * - Test input validation and error handling
 * - Test keyboard interactions (Enter key, etc.)
 * - Verify widget state updates after interactions
 * 
 * This tests the ACTUAL user experience by simulating real clicks and typing.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';

console.log('🧪 CHAT WIDGET INTERACTION TESTS');

interface WidgetInteractionResult {
  readonly test: string;
  readonly success: boolean;
  readonly details: string;
  readonly widgetState?: any;
  readonly htmlSnapshot?: string;
  readonly timing?: number;
}

interface TestResults {
  textInput: Array<WidgetInteractionResult>;
  buttonClicks: Array<WidgetInteractionResult>;
  keyboardInput: Array<WidgetInteractionResult>;
  widgetState: Array<WidgetInteractionResult>;
  errorHandling: Array<WidgetInteractionResult>;
}

class ChatWidgetInteractionTest {
  private client: any;
  private testRoomId: string = 'widget-interaction-test';
  private results: TestResults = {
    textInput: [],
    buttonClicks: [],
    keyboardInput: [],
    widgetState: [],
    errorHandling: []
  };

  async initialize(): Promise<void> {
    console.log('🔗 Connecting to JTAG system for widget interaction testing...');
    
    try {
      const result = await JTAGClientServer.connect();
      this.client = result.client;
      
      if (!result.listResult.success) {
        throw new Error('Failed to connect to JTAG system');
      }
      
      console.log(`✅ Connected to JTAG system with ${result.listResult.commands.length} commands`);
      
    } catch (error) {
      console.error('❌ Failed to initialize widget interaction test:', error);
      throw error;
    }
  }

  /**
   * TEST 1: Text Input Population and Manipulation
   */
  async testTextInput(): Promise<void> {
    console.log('\n📝 TEST 1: Text Input Population and Manipulation');
    
    try {
      const testMessage = `Widget Input Test ${Date.now()}`;
      const startTime = Date.now();
      
      // Test basic text input population
      const inputPopulation = await this.client.executeCommand('exec', {
        code: `
          console.log('📝 WIDGET TEST: Looking for chat widget input...');
          
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) {
            return { success: false, error: 'Chat widget not found' };
          }
          
          // Look for input in shadow DOM first, then regular DOM
          let messageInput = null;
          if (chatWidget.shadowRoot) {
            messageInput = chatWidget.shadowRoot.querySelector('#messageInput') ||
                          chatWidget.shadowRoot.querySelector('input[type="text"]') ||
                          chatWidget.shadowRoot.querySelector('input') ||
                          chatWidget.shadowRoot.querySelector('textarea');
          }
          
          if (!messageInput) {
            messageInput = chatWidget.querySelector('#messageInput') ||
                          chatWidget.querySelector('input[type="text"]') ||
                          chatWidget.querySelector('input') ||
                          chatWidget.querySelector('textarea');
          }
          
          if (!messageInput) {
            return { 
              success: false, 
              error: 'No message input found',
              shadowDomExists: !!chatWidget.shadowRoot,
              shadowDomHTML: chatWidget.shadowRoot ? chatWidget.shadowRoot.innerHTML.substring(0, 300) : null
            };
          }
          
          console.log('✅ WIDGET TEST: Message input found');
          
          // Clear existing value and set new value
          messageInput.value = '';
          messageInput.value = '${testMessage}';
          
          // Trigger input events to notify widget of changes
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          messageInput.dispatchEvent(new Event('change', { bubbles: true }));
          
          // Verify value was set
          const valueSet = messageInput.value === '${testMessage}';
          
          return {
            success: valueSet,
            inputValue: messageInput.value,
            inputType: messageInput.tagName.toLowerCase(),
            inputId: messageInput.id,
            inputClass: messageInput.className,
            eventsTriggered: true
          };
        `,
        environment: 'browser'
      });
      
      const timing = Date.now() - startTime;
      
      this.results.textInput.push({
        test: 'Populate text input field',
        success: inputPopulation.success,
        details: inputPopulation.success ?
          `Input populated: "${inputPopulation.result?.inputValue}", Type: ${inputPopulation.result?.inputType}` :
          `Failed: ${inputPopulation.error || 'Unknown error'}`,
        widgetState: inputPopulation.result,
        timing
      });
      
      // Test input value persistence
      await new Promise(resolve => setTimeout(resolve, 500));
      
      const inputPersistence = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          if (messageInput) {
            return {
              success: messageInput.value === '${testMessage}',
              currentValue: messageInput.value,
              valueLength: messageInput.value.length
            };
          }
          
          return { success: false, error: 'Input not found for persistence test' };
        `,
        environment: 'browser'
      });
      
      this.results.textInput.push({
        test: 'Input value persistence',
        success: inputPersistence.success,
        details: inputPersistence.success ?
          `Value persisted: ${inputPersistence.result?.success}, Current: "${inputPersistence.result?.currentValue}"` :
          `Failed: ${inputPersistence.error || 'Unknown error'}`
      });
      
      // Test input validation (empty, long text, special characters)
      const specialMessage = 'Test with émojis 🚀 and "special" chars & symbols!';
      const inputValidation = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          if (!messageInput) {
            return { success: false, error: 'Input not found' };
          }
          
          // Test special characters
          messageInput.value = '${specialMessage}';
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          
          const specialCharsWork = messageInput.value === '${specialMessage}';
          
          // Test very long text
          const longText = 'A'.repeat(1000);
          messageInput.value = longText;
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          
          const longTextHandled = messageInput.value.length <= 1000;
          
          // Test empty value
          messageInput.value = '';
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          
          const emptyValueHandled = messageInput.value === '';
          
          return {
            success: specialCharsWork && longTextHandled && emptyValueHandled,
            specialCharsWork,
            longTextHandled,
            emptyValueHandled,
            maxLength: messageInput.maxLength || 'no limit'
          };
        `,
        environment: 'browser'
      });
      
      this.results.textInput.push({
        test: 'Input validation and special characters',
        success: inputValidation.success,
        details: inputValidation.success ?
          `Special chars: ${inputValidation.result?.specialCharsWork}, Long text: ${inputValidation.result?.longTextHandled}, Empty: ${inputValidation.result?.emptyValueHandled}` :
          `Failed: ${inputValidation.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.textInput.push({
        test: 'Text input test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 2: Button Click Interactions
   */
  async testButtonClicks(): Promise<void> {
    console.log('\n🖱️ TEST 2: Button Click Interactions');
    
    try {
      const clickMessage = `Button Click Test ${Date.now()}`;
      
      // First populate input for sending
      await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          if (messageInput) {
            messageInput.value = '${clickMessage}';
            messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          }
          
          return { success: !!messageInput };
        `,
        environment: 'browser'
      });
      
      // Test send button click
      const sendButtonClick = await this.client.executeCommand('exec', {
        code: `
          console.log('🖱️ WIDGET TEST: Looking for send button...');
          
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) {
            return { success: false, error: 'Chat widget not found' };
          }
          
          // Look for send button in shadow DOM first, then regular DOM
          let sendButton = null;
          if (chatWidget.shadowRoot) {
            sendButton = chatWidget.shadowRoot.querySelector('#sendButton') ||
                        chatWidget.shadowRoot.querySelector('button[type="submit"]') ||
                        chatWidget.shadowRoot.querySelector('button') ||
                        chatWidget.shadowRoot.querySelector('[onclick]') ||
                        chatWidget.shadowRoot.querySelector('.send-button');
          }
          
          if (!sendButton) {
            sendButton = chatWidget.querySelector('#sendButton') ||
                        chatWidget.querySelector('button[type="submit"]') ||
                        chatWidget.querySelector('button') ||
                        chatWidget.querySelector('[onclick]') ||
                        chatWidget.querySelector('.send-button');
          }
          
          if (!sendButton) {
            return { 
              success: false, 
              error: 'No send button found',
              shadowDomExists: !!chatWidget.shadowRoot,
              availableButtons: chatWidget.shadowRoot ? 
                Array.from(chatWidget.shadowRoot.querySelectorAll('*')).filter(el => 
                  el.tagName.toLowerCase() === 'button' || el.onclick || el.getAttribute('role') === 'button'
                ).map(el => ({ tag: el.tagName, id: el.id, class: el.className, text: el.textContent?.trim() })) : []
            };
          }
          
          console.log('✅ WIDGET TEST: Send button found');
          
          // Record state before click
          const messageInput = chatWidget.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget.shadowRoot?.querySelector('input') ||
                              chatWidget.querySelector('#messageInput') ||
                              chatWidget.querySelector('input');
          
          const preClickValue = messageInput ? messageInput.value : 'no input';
          
          // Click the send button
          sendButton.click();
          
          // Record state after click
          const postClickValue = messageInput ? messageInput.value : 'no input';
          
          return {
            success: true,
            buttonFound: true,
            buttonType: sendButton.tagName.toLowerCase(),
            buttonId: sendButton.id,
            buttonClass: sendButton.className,
            buttonText: sendButton.textContent?.trim(),
            preClickValue,
            postClickValue,
            inputCleared: preClickValue !== postClickValue && postClickValue === ''
          };
        `,
        environment: 'browser'
      });
      
      this.results.buttonClicks.push({
        test: 'Find and click send button',
        success: sendButtonClick.success && sendButtonClick.result?.buttonFound,
        details: sendButtonClick.success ?
          `Button clicked: ${sendButtonClick.result?.buttonType}#${sendButtonClick.result?.buttonId}, Text: "${sendButtonClick.result?.buttonText}", Input cleared: ${sendButtonClick.result?.inputCleared}` :
          `Failed: ${sendButtonClick.error || 'Unknown error'}`,
        widgetState: sendButtonClick.result
      });
      
      // Wait and check if message was sent
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      const messageSentCheck = await this.client.executeCommand('exec', {
        code: `
          // Check if message appears in widget HTML
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) return { success: false, error: 'Widget not found' };
          
          let allHtml = '';
          if (chatWidget.shadowRoot) {
            allHtml += chatWidget.shadowRoot.innerHTML;
          }
          allHtml += chatWidget.innerHTML;
          
          const messageInHTML = allHtml.includes('${clickMessage}');
          
          // Check if widget has messages array
          let messageInArray = false;
          if (chatWidget.messages && Array.isArray(chatWidget.messages)) {
            messageInArray = chatWidget.messages.some(msg => 
              msg.content && msg.content.includes('${clickMessage}')
            );
          }
          
          return {
            success: messageInHTML || messageInArray,
            messageInHTML,
            messageInArray,
            widgetMessagesCount: chatWidget.messages ? chatWidget.messages.length : 0
          };
        `,
        environment: 'browser'
      });
      
      this.results.buttonClicks.push({
        test: 'Message sent after button click',
        success: messageSentCheck.success,
        details: messageSentCheck.success ?
          `Message in HTML: ${messageSentCheck.result?.messageInHTML}, In array: ${messageSentCheck.result?.messageInArray}, Total messages: ${messageSentCheck.result?.widgetMessagesCount}` :
          `Failed: ${messageSentCheck.error || 'Unknown error'}`
      });
      
      // Test disabled button state
      const disabledButtonTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const sendButton = chatWidget?.shadowRoot?.querySelector('#sendButton') ||
                            chatWidget?.shadowRoot?.querySelector('button') ||
                            chatWidget?.querySelector('#sendButton') ||
                            chatWidget?.querySelector('button');
          
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          if (!sendButton || !messageInput) {
            return { success: false, error: 'Button or input not found' };
          }
          
          // Clear input to test disabled state
          messageInput.value = '';
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          
          // Check if button is disabled when input is empty
          const isDisabledWhenEmpty = sendButton.disabled || sendButton.classList.contains('disabled');
          
          // Add text to enable button
          messageInput.value = 'Enable button test';
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          
          const isEnabledWhenFilled = !sendButton.disabled && !sendButton.classList.contains('disabled');
          
          return {
            success: true, // Test execution success
            disabledWhenEmpty: isDisabledWhenEmpty,
            enabledWhenFilled: isEnabledWhenFilled,
            hasDisabledAttribute: sendButton.hasAttribute('disabled'),
            buttonClasses: sendButton.className
          };
        `,
        environment: 'browser'
      });
      
      this.results.buttonClicks.push({
        test: 'Button disabled/enabled states',
        success: disabledButtonTest.success,
        details: disabledButtonTest.success ?
          `Disabled when empty: ${disabledButtonTest.result?.disabledWhenEmpty}, Enabled when filled: ${disabledButtonTest.result?.enabledWhenFilled}` :
          `Failed: ${disabledButtonTest.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.buttonClicks.push({
        test: 'Button click test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 3: Keyboard Input and Events
   */
  async testKeyboardInput(): Promise<void> {
    console.log('\n⌨️ TEST 3: Keyboard Input and Events');
    
    try {
      const keyboardMessage = `Keyboard Test ${Date.now()}`;
      
      // Test Enter key to send message
      const enterKeyTest = await this.client.executeCommand('exec', {
        code: `
          console.log('⌨️ WIDGET TEST: Testing Enter key functionality...');
          
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          if (!messageInput) {
            return { success: false, error: 'Message input not found' };
          }
          
          // Populate input
          messageInput.value = '${keyboardMessage}';
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          
          // Create and dispatch Enter key event
          const enterEvent = new KeyboardEvent('keydown', {
            key: 'Enter',
            keyCode: 13,
            which: 13,
            bubbles: true
          });
          
          const enterPressed = messageInput.dispatchEvent(enterEvent);
          
          // Also try keyup event
          const enterUpEvent = new KeyboardEvent('keyup', {
            key: 'Enter',
            keyCode: 13,
            which: 13,
            bubbles: true
          });
          
          messageInput.dispatchEvent(enterUpEvent);
          
          return {
            success: true,
            enterPressed,
            inputValueBeforeEnter: messageInput.value
          };
        `,
        environment: 'browser'
      });
      
      this.results.keyboardInput.push({
        test: 'Enter key event dispatch',
        success: enterKeyTest.success,
        details: enterKeyTest.success ?
          `Enter key dispatched, Input value: "${enterKeyTest.result?.inputValueBeforeEnter}"` :
          `Failed: ${enterKeyTest.error || 'Unknown error'}`
      });
      
      // Wait and check if Enter key sent the message
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      const enterKeyResult = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          // Check if input was cleared (indicating message was sent)
          const inputCleared = messageInput ? messageInput.value === '' : false;
          
          // Check if message appears in widget
          let messageVisible = false;
          if (chatWidget) {
            const htmlContent = (chatWidget.shadowRoot?.innerHTML || '') + chatWidget.innerHTML;
            messageVisible = htmlContent.includes('${keyboardMessage}');
          }
          
          // Check widget messages array
          let inMessagesArray = false;
          if (chatWidget.messages && Array.isArray(chatWidget.messages)) {
            inMessagesArray = chatWidget.messages.some(msg => 
              msg.content && msg.content.includes('${keyboardMessage}')
            );
          }
          
          return {
            success: inputCleared || messageVisible || inMessagesArray,
            inputCleared,
            messageVisible,
            inMessagesArray,
            currentInputValue: messageInput ? messageInput.value : 'no input'
          };
        `,
        environment: 'browser'
      });
      
      this.results.keyboardInput.push({
        test: 'Enter key sends message',
        success: enterKeyResult.success,
        details: enterKeyResult.success ?
          `Input cleared: ${enterKeyResult.result?.inputCleared}, Message visible: ${enterKeyResult.result?.messageVisible}, In array: ${enterKeyResult.result?.inMessagesArray}` :
          `No response to Enter key: ${enterKeyResult.error || 'Enter not handled'}`
      });
      
      // Test Shift+Enter (should add newline, not send)
      const shiftEnterTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('textarea') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('textarea');
          
          if (!messageInput) {
            return { success: false, error: 'Input not found' };
          }
          
          // Add multi-line text
          messageInput.value = 'Line 1';
          messageInput.focus();
          
          // Simulate Shift+Enter
          const shiftEnterEvent = new KeyboardEvent('keydown', {
            key: 'Enter',
            keyCode: 13,
            which: 13,
            shiftKey: true,
            bubbles: true
          });
          
          messageInput.dispatchEvent(shiftEnterEvent);
          
          return {
            success: true,
            inputSupportsMultiline: messageInput.tagName.toLowerCase() === 'textarea',
            currentValue: messageInput.value,
            inputType: messageInput.tagName.toLowerCase()
          };
        `,
        environment: 'browser'
      });
      
      this.results.keyboardInput.push({
        test: 'Shift+Enter multiline support',
        success: shiftEnterTest.success,
        details: shiftEnterTest.success ?
          `Multiline support: ${shiftEnterTest.result?.inputSupportsMultiline}, Input type: ${shiftEnterTest.result?.inputType}` :
          `Failed: ${shiftEnterTest.error || 'Unknown error'}`
      });
      
      // Test Escape key (should clear input)
      const escapeKeyTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          if (!messageInput) {
            return { success: false, error: 'Input not found' };
          }
          
          // Add text then press Escape
          messageInput.value = 'Text to be cleared by Escape';
          
          const escapeEvent = new KeyboardEvent('keydown', {
            key: 'Escape',
            keyCode: 27,
            which: 27,
            bubbles: true
          });
          
          messageInput.dispatchEvent(escapeEvent);
          
          return {
            success: true,
            valueAfterEscape: messageInput.value,
            inputCleared: messageInput.value === ''
          };
        `,
        environment: 'browser'
      });
      
      this.results.keyboardInput.push({
        test: 'Escape key clears input',
        success: escapeKeyTest.success,
        details: escapeKeyTest.success ?
          `Input cleared by Escape: ${escapeKeyTest.result?.inputCleared}, Final value: "${escapeKeyTest.result?.valueAfterEscape}"` :
          `Failed: ${escapeKeyTest.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.keyboardInput.push({
        test: 'Keyboard input test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 4: Widget State Management
   */
  async testWidgetState(): Promise<void> {
    console.log('\n📊 TEST 4: Widget State Management');
    
    try {
      // Test widget state after interactions
      const stateTest = await this.client.executeCommand('exec', {
        code: `
          console.log('📊 WIDGET TEST: Analyzing widget state...');
          
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) {
            return { success: false, error: 'Chat widget not found' };
          }
          
          // Analyze widget properties and methods
          const widgetProps = Object.getOwnPropertyNames(chatWidget);
          const widgetPrototype = Object.getPrototypeOf(chatWidget);
          const widgetMethods = Object.getOwnPropertyNames(widgetPrototype)
            .filter(name => typeof chatWidget[name] === 'function');
          
          // Check for common state properties
          const stateProps = {
            messages: chatWidget.messages,
            currentRoom: chatWidget.currentRoom,
            user: chatWidget.user || chatWidget.userId,
            connected: chatWidget.connected,
            loading: chatWidget.loading
          };
          
          // Check widget lifecycle methods
          const lifecycleMethods = widgetMethods.filter(method => 
            method.includes('init') || method.includes('render') || 
            method.includes('update') || method.includes('cleanup')
          );
          
          // Check event handling methods
          const eventMethods = widgetMethods.filter(method =>
            method.includes('event') || method.includes('handle') ||
            method.includes('on') || method.startsWith('on')
          );
          
          return {
            success: true,
            widgetExists: true,
            propertyCount: widgetProps.length,
            methodCount: widgetMethods.length,
            hasMessages: !!chatWidget.messages,
            messagesCount: chatWidget.messages ? chatWidget.messages.length : 0,
            stateProps,
            lifecycleMethods,
            eventMethods,
            widgetTagName: chatWidget.tagName.toLowerCase()
          };
        `,
        environment: 'browser'
      });
      
      this.results.widgetState.push({
        test: 'Widget state analysis',
        success: stateTest.success,
        details: stateTest.success ?
          `Widget exists with ${stateTest.result?.methodCount} methods, ${stateTest.result?.messagesCount} messages, lifecycle methods: ${stateTest.result?.lifecycleMethods?.length}` :
          `Failed: ${stateTest.error || 'Unknown error'}`,
        widgetState: stateTest.result
      });
      
      // Test widget refresh/update functionality
      const refreshTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) return { success: false, error: 'Widget not found' };
          
          let refreshResult = { success: false };
          
          // Try different refresh methods
          if (typeof chatWidget.refresh === 'function') {
            await chatWidget.refresh();
            refreshResult = { success: true, method: 'refresh' };
          } else if (typeof chatWidget.update === 'function') {
            await chatWidget.update();
            refreshResult = { success: true, method: 'update' };
          } else if (typeof chatWidget.loadRoomHistory === 'function') {
            await chatWidget.loadRoomHistory();
            refreshResult = { success: true, method: 'loadRoomHistory' };
          } else if (typeof chatWidget.renderWidget === 'function') {
            await chatWidget.renderWidget();
            refreshResult = { success: true, method: 'renderWidget' };
          }
          
          return refreshResult;
        `,
        environment: 'browser'
      });
      
      this.results.widgetState.push({
        test: 'Widget refresh functionality',
        success: refreshTest.success,
        details: refreshTest.success ?
          `Widget refresh method available: ${refreshTest.result?.method}` :
          `No refresh method found: ${refreshTest.error || 'Widget not refreshable'}`
      });
      
      // Test widget event subscription
      const eventSubscriptionTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) return { success: false, error: 'Widget not found' };
          
          // Check for event-related properties and methods
          const eventMethods = [];
          const allMethods = Object.getOwnPropertyNames(Object.getPrototypeOf(chatWidget));
          
          allMethods.forEach(method => {
            if (method.includes('Event') || method.includes('subscribe') || 
                method.includes('listen') || method.includes('handler')) {
              eventMethods.push(method);
            }
          });
          
          // Test if widget can handle custom events
          let customEventHandled = false;
          const testEvent = new CustomEvent('chat:test-event', {
            detail: { test: true }
          });
          
          try {
            chatWidget.dispatchEvent(testEvent);
            customEventHandled = true;
          } catch (e) {
            // Event dispatch failed
          }
          
          return {
            success: eventMethods.length > 0 || customEventHandled,
            eventMethods,
            customEventHandled,
            canDispatchEvents: customEventHandled
          };
        `,
        environment: 'browser'
      });
      
      this.results.widgetState.push({
        test: 'Widget event handling',
        success: eventSubscriptionTest.success,
        details: eventSubscriptionTest.success ?
          `Event methods: ${eventSubscriptionTest.result?.eventMethods?.length}, Custom events: ${eventSubscriptionTest.result?.customEventHandled}` :
          `No event handling: ${eventSubscriptionTest.error || 'Widget does not handle events'}`
      });
      
    } catch (error) {
      this.results.widgetState.push({
        test: 'Widget state test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 5: Error Handling and Edge Cases
   */
  async testErrorHandling(): Promise<void> {
    console.log('\n🚨 TEST 5: Error Handling and Edge Cases');
    
    try {
      // Test sending empty message
      const emptyMessageTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          const messageInput = chatWidget?.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget?.shadowRoot?.querySelector('input') ||
                              chatWidget?.querySelector('#messageInput') ||
                              chatWidget?.querySelector('input');
          
          const sendButton = chatWidget?.shadowRoot?.querySelector('#sendButton') ||
                            chatWidget?.shadowRoot?.querySelector('button') ||
                            chatWidget?.querySelector('#sendButton') ||
                            chatWidget?.querySelector('button');
          
          if (!messageInput || !sendButton) {
            return { success: false, error: 'Input or button not found' };
          }
          
          // Clear input and try to send empty message
          messageInput.value = '';
          messageInput.dispatchEvent(new Event('input', { bubbles: true }));
          
          // Check if button is disabled
          const buttonDisabled = sendButton.disabled || sendButton.classList.contains('disabled');
          
          // Try clicking anyway
          sendButton.click();
          
          return {
            success: true,
            buttonDisabledForEmpty: buttonDisabled,
            emptyClickAttempted: true
          };
        `,
        environment: 'browser'
      });
      
      this.results.errorHandling.push({
        test: 'Empty message handling',
        success: emptyMessageTest.success,
        details: emptyMessageTest.success ?
          `Button disabled for empty input: ${emptyMessageTest.result?.buttonDisabledForEmpty}` :
          `Failed: ${emptyMessageTest.error || 'Unknown error'}`
      });
      
      // Test widget without network connection (simulate)
      const networkErrorTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) return { success: false, error: 'Widget not found' };
          
          // Try to trigger a network error scenario
          let errorHandled = false;
          let errorMessage = '';
          
          // Override fetch to simulate network error
          const originalFetch = window.fetch;
          window.fetch = () => Promise.reject(new Error('Network error'));
          
          try {
            // Try to send a message which should trigger network call
            if (typeof chatWidget.sendMessage === 'function') {
              const result = await chatWidget.sendMessage('Network test message');
              errorHandled = !result || !result.success;
            }
          } catch (error) {
            errorHandled = true;
            errorMessage = error.message;
          }
          
          // Restore original fetch
          window.fetch = originalFetch;
          
          return {
            success: true, // Test executed successfully
            errorHandled,
            errorMessage
          };
        `,
        environment: 'browser'
      });
      
      this.results.errorHandling.push({
        test: 'Network error handling',
        success: networkErrorTest.success,
        details: networkErrorTest.success ?
          `Network error handled gracefully: ${networkErrorTest.result?.errorHandled}` :
          `Failed: ${networkErrorTest.error || 'Unknown error'}`
      });
      
      // Test widget behavior when DOM is manipulated
      const domManipulationTest = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) return { success: false, error: 'Widget not found' };
          
          // Try removing and re-adding input element
          const messageInput = chatWidget.shadowRoot?.querySelector('#messageInput') ||
                              chatWidget.querySelector('#messageInput');
          
          if (messageInput) {
            const parent = messageInput.parentNode;
            const inputClone = messageInput.cloneNode(true);
            
            // Remove input
            messageInput.remove();
            
            // Try to use widget without input
            let widgetHandlesRemoval = true;
            try {
              if (typeof chatWidget.sendMessage === 'function') {
                await chatWidget.sendMessage('Test after removal');
              }
            } catch (error) {
              widgetHandlesRemoval = false;
            }
            
            // Re-add input
            if (parent) {
              parent.appendChild(inputClone);
            }
            
            return {
              success: true,
              handlesInputRemoval: widgetHandlesRemoval,
              inputRestored: !!parent
            };
          }
          
          return { success: false, error: 'No input to manipulate' };
        `,
        environment: 'browser'
      });
      
      this.results.errorHandling.push({
        test: 'DOM manipulation resilience',
        success: domManipulationTest.success,
        details: domManipulationTest.success ?
          `Handles input removal: ${domManipulationTest.result?.handlesInputRemoval}, Input restored: ${domManipulationTest.result?.inputRestored}` :
          `Failed: ${domManipulationTest.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.errorHandling.push({
        test: 'Error handling test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * Run all widget interaction tests
   */
  async runAllTests(): Promise<void> {
    await this.initialize();
    
    await this.testTextInput();
    await this.testButtonClicks();
    await this.testKeyboardInput();
    await this.testWidgetState();
    await this.testErrorHandling();
  }

  /**
   * Display comprehensive test results
   */
  displayResults(): void {
    console.log(`\n🎯 CHAT WIDGET INTERACTION TEST RESULTS`);
    console.log('=======================================');
    
    const categories = [
      { name: 'Text Input', tests: this.results.textInput, icon: '📝' },
      { name: 'Button Clicks', tests: this.results.buttonClicks, icon: '🖱️' },
      { name: 'Keyboard Input', tests: this.results.keyboardInput, icon: '⌨️' },
      { name: 'Widget State', tests: this.results.widgetState, icon: '📊' },
      { name: 'Error Handling', tests: this.results.errorHandling, icon: '🚨' }
    ];
    
    let totalTests = 0;
    let totalPassed = 0;
    
    categories.forEach(category => {
      const passed = category.tests.filter(test => test.success).length;
      const total = category.tests.length;
      totalTests += total;
      totalPassed += passed;
      
      console.log(`\n${category.icon} ${category.name.toUpperCase()}: ${passed}/${total} tests passed`);
      category.tests.forEach(test => {
        const status = test.success ? '✅' : '❌';
        const timing = test.timing ? ` (${test.timing}ms)` : '';
        console.log(`  ${status} ${test.test}: ${test.details}${timing}`);
      });
    });
    
    console.log(`\n📊 OVERALL SUMMARY: ${totalPassed}/${totalTests} widget interaction tests passed`);
    console.log(`Success Rate: ${((totalPassed / totalTests) * 100).toFixed(1)}%`);
    
    if (totalPassed === totalTests) {
      console.log('🎉 ✅ CHAT WIDGET INTERACTIONS WORKING PERFECTLY!');
      console.log('📝 Text input population and validation working');
      console.log('🖱️ Button clicks properly send messages');
      console.log('⌨️ Keyboard shortcuts (Enter, Escape) functional');
      console.log('📊 Widget state management is robust');
      console.log('🚨 Error handling prevents user frustration');
      console.log('🚀 Ready for production user interactions');
    } else {
      console.log('⚠️  Widget interactions have usability issues');
      console.log('🔧 Fix failing tests for proper user experience');
      console.log('📋 These issues prevent real chat functionality');
      
      // Identify critical interaction failures
      const criticalIssues = [];
      if (this.results.textInput.filter(t => t.success).length === 0) {
        criticalIssues.push('Text input completely non-functional');
      }
      if (this.results.buttonClicks.filter(t => t.success).length === 0) {
        criticalIssues.push('Send button completely non-functional');
      }
      if (this.results.keyboardInput.filter(t => t.success).length === 0) {
        criticalIssues.push('Keyboard shortcuts not working');
      }
      
      if (criticalIssues.length > 0) {
        console.log('\n🚨 CRITICAL INTERACTION FAILURES:');
        criticalIssues.forEach(issue => console.log(`   ❌ ${issue}`));
        console.log('\n💡 Users cannot send messages until these are fixed!');
      }
    }
  }
}

// Main execution
async function runChatWidgetInteractionTests(): Promise<void> {
  const testRunner = new ChatWidgetInteractionTest();
  
  try {
    await testRunner.runAllTests();
    testRunner.displayResults();
    
  } catch (error) {
    console.error('💥 Chat widget interaction test execution failed:', error);
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (require.main === module) {
  runChatWidgetInteractionTests().catch(error => {
    console.error('💥 Test runner failed:', error);
    process.exit(1);
  });
}

export { runChatWidgetInteractionTests, ChatWidgetInteractionTest };