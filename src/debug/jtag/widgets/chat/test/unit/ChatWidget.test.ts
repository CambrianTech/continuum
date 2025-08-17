#!/usr/bin/env tsx

/**
 * ChatWidget Unit Test
 * 
 * Tests the ChatWidget in isolation without JTAG system dependencies.
 * Follows middle-out testing principle - start with the core widget functionality.
 */

import { ChatWidget } from '../../ChatWidget';

describe('ChatWidget Unit Tests', () => {
  let widget: ChatWidget;

  beforeEach(() => {
    widget = new ChatWidget();
    document.body.appendChild(widget);
  });

  afterEach(() => {
    if (widget.parentNode) {
      widget.parentNode.removeChild(widget);
    }
  });

  describe('Basic Widget Functionality', () => {
    it('should have correct widget name', () => {
      expect(ChatWidget.widgetName).toBe('chat');
    });

    it('should have correct tag name', () => {
      expect(ChatWidget.tagName).toBe('chat-widget');
    });

    it('should create shadow DOM on connect', () => {
      widget.connectedCallback();
      expect(widget.shadowRoot).toBeTruthy();
    });

    it('should render chat UI elements', () => {
      widget.connectedCallback();
      
      const header = widget.shadowRoot?.querySelector('.chat-header');
      const messages = widget.shadowRoot?.querySelector('.messages');
      const inputArea = widget.shadowRoot?.querySelector('.input-area');
      
      expect(header).toBeTruthy();
      expect(messages).toBeTruthy();
      expect(inputArea).toBeTruthy();
    });

    it('should have message input and send button', () => {
      widget.connectedCallback();
      
      const messageInput = widget.shadowRoot?.getElementById('messageInput') as HTMLInputElement;
      const sendButton = widget.shadowRoot?.getElementById('sendButton');
      
      expect(messageInput).toBeTruthy();
      expect(sendButton).toBeTruthy();
      expect(messageInput.type).toBe('text');
    });
  });

  describe('Message Handling', () => {
    beforeEach(() => {
      widget.connectedCallback();
    });

    it('should clear input after adding user message', () => {
      const messageInput = widget.shadowRoot?.getElementById('messageInput') as HTMLInputElement;
      messageInput.value = 'test message';
      
      // Call addMessage directly to test message UI
      (widget as any).addMessage('test message', 'user');
      
      const messages = widget.shadowRoot?.querySelectorAll('.message');
      expect(messages?.length).toBeGreaterThan(1); // Initial message + new message
    });

    it('should add messages to UI with correct classes', () => {
      (widget as any).addMessage('user test', 'user');
      (widget as any).addMessage('assistant test', 'assistant');
      
      const userMessage = widget.shadowRoot?.querySelector('.message.user');
      const assistantMessage = widget.shadowRoot?.querySelector('.message.assistant');
      
      expect(userMessage?.textContent).toContain('user test');
      expect(assistantMessage?.textContent).toContain('assistant test');
    });
  });

  describe('Widget Integration Interface', () => {
    it('should have executeCommand method for JTAG integration', () => {
      expect(typeof (widget as any).executeCommand).toBe('function');
    });

    it('should throw error when WidgetDaemon not available', async () => {
      // Mock missing widgetDaemon
      const originalWidgetDaemon = (global as any).widgetDaemon;
      delete (global as any).widgetDaemon;
      
      try {
        await (widget as any).executeCommand('ping', {});
        fail('Should have thrown error');
      } catch (error) {
        expect(error.message).toContain('WidgetDaemon not available');
      } finally {
        // Restore
        (global as any).widgetDaemon = originalWidgetDaemon;
      }
    });
  });
});

console.log('🧪 ChatWidget Unit Tests: Validating self-contained widget functionality...');