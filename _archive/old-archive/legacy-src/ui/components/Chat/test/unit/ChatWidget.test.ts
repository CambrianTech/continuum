/**
 * ChatWidget Unit Tests
 * Tests for the Chat widget component following MIDDLE-OUT methodology
 */

import { ChatWidget } from '../../ChatWidget.ts';

describe('ChatWidget', () => {
  let widget: ChatWidget;

  beforeEach(() => {
    widget = new ChatWidget();
  });

  afterEach(() => {
    if (widget.shadowRoot) {
      widget.remove();
    }
  });

  describe('initialization', () => {
    it('should create widget instance', () => {
      expect(widget).toBeInstanceOf(ChatWidget);
      expect(widget.widgetName).toBe('ChatWidget');
      expect(widget.widgetIcon).toBe('💬');
      expect(widget.widgetTitle).toBe('Chat');
    });

    it('should have initial empty message state', () => {
      expect(widget['messages']).toEqual([]);
      expect(widget['isTyping']).toBe(false);
      expect(widget['currentRoomId']).toBe('general');
    });
  });

  describe('message handling', () => {
    it('should generate unique message IDs', () => {
      const id1 = widget['generateMessageId']();
      const id2 = widget['generateMessageId']();
      
      expect(id1).toContain('msg_');
      expect(id2).toContain('msg_');
      expect(id1).not.toBe(id2);
    });

    it('should add messages to the message list', () => {
      const message = {
        id: 'test-1',
        type: 'user' as const,
        content: 'Test message',
        timestamp: new Date()
      };

      widget['addMessage'](message);
      expect(widget['messages']).toHaveLength(1);
      expect(widget['messages'][0]).toEqual(message);
    });
  });

  describe('room management', () => {
    it('should switch rooms correctly', async () => {
      const initialRoom = widget['currentRoomId'];
      await widget.switchRoom('test-room');
      
      expect(widget['currentRoomId']).toBe('test-room');
      expect(widget['currentRoomId']).not.toBe(initialRoom);
      expect(widget['messages']).toEqual([]); // Messages should be cleared
    });

    it('should get display name for known rooms', () => {
      expect(widget['getRoomDisplayName']()).toBe('General'); // Default room
    });
  });

  describe('content rendering', () => {
    it('should render welcome message when no messages', () => {
      const content = widget.renderContent();
      
      expect(content).toContain('chat-container');
      expect(content).toContain('Welcome to Continuum');
      expect(content).toContain('input-area');
    });

    it('should format message content correctly', () => {
      const formatted = widget['formatMessageContent']('**bold** and *italic* and `code`');
      
      expect(formatted).toContain('<strong>bold</strong>');
      expect(formatted).toContain('<em>italic</em>');
      expect(formatted).toContain('<code>code</code>');
    });
  });

  describe('status handling', () => {
    it('should return correct status icons', () => {
      expect(widget['getStatusIcon']('sending')).toBe('⏳');
      expect(widget['getStatusIcon']('sent')).toBe('✓');
      expect(widget['getStatusIcon']('error')).toBe('❌');
      expect(widget['getStatusIcon']()).toBe('');
    });
  });
});