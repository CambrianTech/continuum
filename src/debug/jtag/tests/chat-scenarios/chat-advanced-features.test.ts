#!/usr/bin/env tsx
/**
 * Chat Advanced Features Tests - Discord-Scale Functionality
 * 
 * Tests advanced chat features that are expected in modern chat applications:
 * - Message threading and reply chains
 * - Emoji reactions and custom emojis
 * - @mentions and notifications
 * - File attachments and media sharing
 * - Message editing and history
 * - Rich text formatting (bold, italic, code blocks)
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { ChatMessage } from '../../widgets/chat/shared/ChatModuleTypes';
import type { ChatSendMessageParams, ChatSendMessageResult } from '../../commands/chat/send-message/shared/ChatSendMessageTypes';

console.log('🧪 DISCORD-SCALE CHAT ADVANCED FEATURES TESTS');

interface AdvancedMessage extends ChatMessage {
  readonly threadId?: string;
  readonly parentMessageId?: string;
  readonly reactions?: ReadonlyArray<{
    readonly emoji: string;
    readonly count: number;
    readonly userIds: ReadonlyArray<string>;
  }>;
  readonly mentions?: ReadonlyArray<{
    readonly userId: string;
    readonly userName: string;
    readonly type: 'user' | 'role' | 'channel';
  }>;
  readonly attachments?: ReadonlyArray<{
    readonly fileId: string;
    readonly fileName: string;
    readonly fileType: string;
    readonly fileSize: number;
    readonly url: string;
  }>;
  readonly editHistory?: ReadonlyArray<{
    readonly content: string;
    readonly editedAt: string;
    readonly editReason?: string;
  }>;
  readonly formatting?: {
    readonly bold?: ReadonlyArray<[number, number]>;
    readonly italic?: ReadonlyArray<[number, number]>;
    readonly code?: ReadonlyArray<[number, number]>;
    readonly codeBlock?: ReadonlyArray<[number, number, string]>; // [start, end, language]
  };
}

interface TestResults {
  threading: Array<{ test: string; success: boolean; details: string }>;
  reactions: Array<{ test: string; success: boolean; details: string }>;
  mentions: Array<{ test: string; success: boolean; details: string }>;
  attachments: Array<{ test: string; success: boolean; details: string }>;
  editing: Array<{ test: string; success: boolean; details: string }>;
  formatting: Array<{ test: string; success: boolean; details: string }>;
}

class AdvancedChatFeaturesTest {
  private client: any;
  private testRoomId: string = 'advanced-features-test';
  private testUserId: string = 'advanced-test-user';
  private results: TestResults = {
    threading: [],
    reactions: [],
    mentions: [],
    attachments: [],
    editing: [],
    formatting: []
  };

  async initialize(): Promise<void> {
    console.log('🔗 Connecting to JTAG system for advanced chat features testing...');
    
    try {
      const result = await JTAGClientServer.connect();
      this.client = result.client;
      
      if (!result.listResult.success) {
        throw new Error('Failed to connect to JTAG system');
      }
      
      console.log(`✅ Connected to JTAG system with ${result.listResult.commands.length} commands`);
      
    } catch (error) {
      console.error('❌ Failed to initialize advanced chat features test:', error);
      throw error;
    }
  }

  /**
   * TEST 1: Message Threading and Reply Chains
   */
  async testMessageThreading(): Promise<void> {
    console.log('\n🧵 TEST 1: Message Threading and Reply Chains');
    
    try {
      // Send original message
      const originalMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'This is the original message that will start a thread',
        messageType: 'original'
      });
      
      this.results.threading.push({
        test: 'Send original message',
        success: originalMessage.success,
        details: originalMessage.success ? 
          `Original message sent: ${originalMessage.messageId}` : 
          `Failed: ${originalMessage.error || 'Unknown error'}`
      });
      
      if (originalMessage.success) {
        // Send reply message in thread
        const replyMessage = await this.client.executeCommand('chat/send-message', {
          roomId: this.testRoomId,
          userId: 'reply-user',
          content: 'This is a reply in the thread',
          messageType: 'reply',
          threadId: originalMessage.messageId,
          parentMessageId: originalMessage.messageId
        });
        
        this.results.threading.push({
          test: 'Send reply message',
          success: replyMessage.success,
          details: replyMessage.success ?
            `Reply sent: ${replyMessage.messageId} in thread: ${originalMessage.messageId}` :
            `Failed: ${replyMessage.error || 'Unknown error'}`
        });
        
        // Send nested reply
        if (replyMessage.success) {
          const nestedReply = await this.client.executeCommand('chat/send-message', {
            roomId: this.testRoomId,
            userId: 'nested-user',
            content: 'This is a nested reply to the reply',
            messageType: 'reply',
            threadId: originalMessage.messageId,
            parentMessageId: replyMessage.messageId
          });
          
          this.results.threading.push({
            test: 'Send nested reply',
            success: nestedReply.success,
            details: nestedReply.success ?
              `Nested reply sent: ${nestedReply.messageId}` :
              `Failed: ${nestedReply.error || 'Unknown error'}`
          });
        }
        
        // Retrieve thread messages
        const threadMessages = await this.client.executeCommand('chat/get-thread', {
          threadId: originalMessage.messageId,
          roomId: this.testRoomId
        });
        
        this.results.threading.push({
          test: 'Retrieve thread messages',
          success: threadMessages.success && threadMessages.messages?.length >= 2,
          details: threadMessages.success ?
            `Retrieved ${threadMessages.messages?.length || 0} thread messages` :
            `Failed: ${threadMessages.error || 'Unknown error'}`
        });
      }
      
    } catch (error) {
      this.results.threading.push({
        test: 'Message threading test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 2: Emoji Reactions and Custom Emojis
   */
  async testEmojiReactions(): Promise<void> {
    console.log('\n👍 TEST 2: Emoji Reactions and Custom Emojis');
    
    try {
      // Send message to react to
      const messageToReact = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'React to this message with emojis!'
      });
      
      this.results.reactions.push({
        test: 'Send message for reactions',
        success: messageToReact.success,
        details: messageToReact.success ?
          `Message sent: ${messageToReact.messageId}` :
          `Failed: ${messageToReact.error || 'Unknown error'}`
      });
      
      if (messageToReact.success) {
        // Add thumbs up reaction
        const thumbsUpReaction = await this.client.executeCommand('chat/add-reaction', {
          messageId: messageToReact.messageId,
          roomId: this.testRoomId,
          userId: this.testUserId,
          emoji: '👍'
        });
        
        this.results.reactions.push({
          test: 'Add thumbs up reaction',
          success: thumbsUpReaction.success,
          details: thumbsUpReaction.success ?
            `Reaction added: 👍` :
            `Failed: ${thumbsUpReaction.error || 'Unknown error'}`
        });
        
        // Add heart reaction from different user
        const heartReaction = await this.client.executeCommand('chat/add-reaction', {
          messageId: messageToReact.messageId,
          roomId: this.testRoomId,
          userId: 'reaction-user',
          emoji: '❤️'
        });
        
        this.results.reactions.push({
          test: 'Add heart reaction',
          success: heartReaction.success,
          details: heartReaction.success ?
            `Reaction added: ❤️` :
            `Failed: ${heartReaction.error || 'Unknown error'}`
        });
        
        // Add custom emoji reaction
        const customReaction = await this.client.executeCommand('chat/add-reaction', {
          messageId: messageToReact.messageId,
          roomId: this.testRoomId,
          userId: 'custom-user',
          emoji: ':custom_emoji:',
          emojiType: 'custom',
          emojiUrl: 'https://example.com/custom-emoji.png'
        });
        
        this.results.reactions.push({
          test: 'Add custom emoji reaction',
          success: customReaction.success,
          details: customReaction.success ?
            `Custom reaction added: :custom_emoji:` :
            `Failed: ${customReaction.error || 'Unknown error'}`
        });
        
        // Get message with reactions
        const messageWithReactions = await this.client.executeCommand('chat/get-message', {
          messageId: messageToReact.messageId,
          roomId: this.testRoomId,
          includeReactions: true
        });
        
        this.results.reactions.push({
          test: 'Retrieve message with reactions',
          success: messageWithReactions.success && messageWithReactions.message?.reactions?.length > 0,
          details: messageWithReactions.success ?
            `Message has ${messageWithReactions.message?.reactions?.length || 0} reactions` :
            `Failed: ${messageWithReactions.error || 'Unknown error'}`
        });
        
        // Remove reaction
        const removeReaction = await this.client.executeCommand('chat/remove-reaction', {
          messageId: messageToReact.messageId,
          roomId: this.testRoomId,
          userId: this.testUserId,
          emoji: '👍'
        });
        
        this.results.reactions.push({
          test: 'Remove reaction',
          success: removeReaction.success,
          details: removeReaction.success ?
            `Reaction removed: 👍` :
            `Failed: ${removeReaction.error || 'Unknown error'}`
        });
      }
      
    } catch (error) {
      this.results.reactions.push({
        test: 'Emoji reactions test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 3: @Mentions and Notifications
   */
  async testMentionsAndNotifications(): Promise<void> {
    console.log('\n📢 TEST 3: @Mentions and Notifications');
    
    try {
      // Send message with user mention
      const userMention = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'Hey @john_doe, can you check this out?',
        mentions: [{
          userId: 'john_doe',
          userName: 'John Doe',
          type: 'user'
        }]
      });
      
      this.results.mentions.push({
        test: 'Send message with user mention',
        success: userMention.success,
        details: userMention.success ?
          `Message with user mention sent: ${userMention.messageId}` :
          `Failed: ${userMention.error || 'Unknown error'}`
      });
      
      // Send message with role mention
      const roleMention = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'Attention @developers, we have a new requirement',
        mentions: [{
          userId: 'developers',
          userName: 'Developers',
          type: 'role'
        }]
      });
      
      this.results.mentions.push({
        test: 'Send message with role mention',
        success: roleMention.success,
        details: roleMention.success ?
          `Message with role mention sent: ${roleMention.messageId}` :
          `Failed: ${roleMention.error || 'Unknown error'}`
      });
      
      // Send message with channel mention
      const channelMention = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'Check the discussion in #general channel',
        mentions: [{
          userId: 'general',
          userName: 'general',
          type: 'channel'
        }]
      });
      
      this.results.mentions.push({
        test: 'Send message with channel mention',
        success: channelMention.success,
        details: channelMention.success ?
          `Message with channel mention sent: ${channelMention.messageId}` :
          `Failed: ${channelMention.error || 'Unknown error'}`
      });
      
      // Get mentions for user
      const userMentions = await this.client.executeCommand('chat/get-mentions', {
        userId: 'john_doe',
        roomId: this.testRoomId,
        unreadOnly: true
      });
      
      this.results.mentions.push({
        test: 'Retrieve user mentions',
        success: userMentions.success,
        details: userMentions.success ?
          `Found ${userMentions.mentions?.length || 0} mentions for user` :
          `Failed: ${userMentions.error || 'Unknown error'}`
      });
      
      // Mark mention as read
      if (userMention.success) {
        const markRead = await this.client.executeCommand('chat/mark-mention-read', {
          messageId: userMention.messageId,
          userId: 'john_doe',
          roomId: this.testRoomId
        });
        
        this.results.mentions.push({
          test: 'Mark mention as read',
          success: markRead.success,
          details: markRead.success ?
            `Mention marked as read` :
            `Failed: ${markRead.error || 'Unknown error'}`
        });
      }
      
    } catch (error) {
      this.results.mentions.push({
        test: 'Mentions and notifications test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 4: File Attachments and Media Sharing
   */
  async testFileAttachments(): Promise<void> {
    console.log('\n📎 TEST 4: File Attachments and Media Sharing');
    
    try {
      // Upload file attachment
      const fileUpload = await this.client.executeCommand('chat/upload-attachment', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        fileName: 'test-document.pdf',
        fileType: 'application/pdf',
        fileSize: 1024000,
        fileData: Buffer.from('fake-pdf-data').toString('base64')
      });
      
      this.results.attachments.push({
        test: 'Upload file attachment',
        success: fileUpload.success,
        details: fileUpload.success ?
          `File uploaded: ${fileUpload.fileId}` :
          `Failed: ${fileUpload.error || 'Unknown error'}`
      });
      
      if (fileUpload.success) {
        // Send message with file attachment
        const messageWithFile = await this.client.executeCommand('chat/send-message', {
          roomId: this.testRoomId,
          userId: this.testUserId,
          content: 'Here is the document you requested',
          attachments: [{
            fileId: fileUpload.fileId,
            fileName: 'test-document.pdf',
            fileType: 'application/pdf',
            fileSize: 1024000,
            url: fileUpload.url
          }]
        });
        
        this.results.attachments.push({
          test: 'Send message with attachment',
          success: messageWithFile.success,
          details: messageWithFile.success ?
            `Message with attachment sent: ${messageWithFile.messageId}` :
            `Failed: ${messageWithFile.error || 'Unknown error'}`
        });
      }
      
      // Upload image attachment
      const imageUpload = await this.client.executeCommand('chat/upload-attachment', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        fileName: 'screenshot.png',
        fileType: 'image/png',
        fileSize: 512000,
        fileData: Buffer.from('fake-image-data').toString('base64')
      });
      
      this.results.attachments.push({
        test: 'Upload image attachment',
        success: imageUpload.success,
        details: imageUpload.success ?
          `Image uploaded: ${imageUpload.fileId}` :
          `Failed: ${imageUpload.error || 'Unknown error'}`
      });
      
      // Get attachment metadata
      if (fileUpload.success) {
        const attachmentInfo = await this.client.executeCommand('chat/get-attachment', {
          fileId: fileUpload.fileId,
          roomId: this.testRoomId
        });
        
        this.results.attachments.push({
          test: 'Get attachment metadata',
          success: attachmentInfo.success,
          details: attachmentInfo.success ?
            `Retrieved metadata for ${attachmentInfo.fileName}` :
            `Failed: ${attachmentInfo.error || 'Unknown error'}`
        });
      }
      
      // Delete attachment
      if (fileUpload.success) {
        const deleteAttachment = await this.client.executeCommand('chat/delete-attachment', {
          fileId: fileUpload.fileId,
          roomId: this.testRoomId,
          userId: this.testUserId
        });
        
        this.results.attachments.push({
          test: 'Delete attachment',
          success: deleteAttachment.success,
          details: deleteAttachment.success ?
            `Attachment deleted` :
            `Failed: ${deleteAttachment.error || 'Unknown error'}`
        });
      }
      
    } catch (error) {
      this.results.attachments.push({
        test: 'File attachments test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 5: Message Editing and History
   */
  async testMessageEditing(): Promise<void> {
    console.log('\n✏️ TEST 5: Message Editing and History');
    
    try {
      // Send original message
      const originalMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'This is the original message content'
      });
      
      this.results.editing.push({
        test: 'Send original message',
        success: originalMessage.success,
        details: originalMessage.success ?
          `Original message sent: ${originalMessage.messageId}` :
          `Failed: ${originalMessage.error || 'Unknown error'}`
      });
      
      if (originalMessage.success) {
        // Edit message content
        const editMessage = await this.client.executeCommand('chat/edit-message', {
          messageId: originalMessage.messageId,
          roomId: this.testRoomId,
          userId: this.testUserId,
          newContent: 'This is the edited message content',
          editReason: 'Fixed typo'
        });
        
        this.results.editing.push({
          test: 'Edit message content',
          success: editMessage.success,
          details: editMessage.success ?
            `Message edited successfully` :
            `Failed: ${editMessage.error || 'Unknown error'}`
        });
        
        // Edit message again
        const editAgain = await this.client.executeCommand('chat/edit-message', {
          messageId: originalMessage.messageId,
          roomId: this.testRoomId,
          userId: this.testUserId,
          newContent: 'This is the final edited message content',
          editReason: 'Clarification'
        });
        
        this.results.editing.push({
          test: 'Edit message multiple times',
          success: editAgain.success,
          details: editAgain.success ?
            `Message edited again successfully` :
            `Failed: ${editAgain.error || 'Unknown error'}`
        });
        
        // Get message edit history
        const editHistory = await this.client.executeCommand('chat/get-edit-history', {
          messageId: originalMessage.messageId,
          roomId: this.testRoomId
        });
        
        this.results.editing.push({
          test: 'Get message edit history',
          success: editHistory.success && editHistory.edits?.length >= 2,
          details: editHistory.success ?
            `Found ${editHistory.edits?.length || 0} edit records` :
            `Failed: ${editHistory.error || 'Unknown error'}`
        });
        
        // Try to edit someone else's message (should fail)
        const unauthorizedEdit = await this.client.executeCommand('chat/edit-message', {
          messageId: originalMessage.messageId,
          roomId: this.testRoomId,
          userId: 'other-user',
          newContent: 'Trying to edit someone else\'s message'
        });
        
        this.results.editing.push({
          test: 'Unauthorized edit (should fail)',
          success: !unauthorizedEdit.success, // Success means it properly rejected
          details: !unauthorizedEdit.success ?
            `Unauthorized edit properly rejected` :
            `Security issue: unauthorized edit allowed`
        });
      }
      
    } catch (error) {
      this.results.editing.push({
        test: 'Message editing test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 6: Rich Text Formatting
   */
  async testRichTextFormatting(): Promise<void> {
    console.log('\n💄 TEST 6: Rich Text Formatting');
    
    try {
      // Send message with bold text
      const boldMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'This message has **bold text** in it',
        formatting: {
          bold: [[17, 26]] // "bold text"
        }
      });
      
      this.results.formatting.push({
        test: 'Send message with bold formatting',
        success: boldMessage.success,
        details: boldMessage.success ?
          `Bold message sent: ${boldMessage.messageId}` :
          `Failed: ${boldMessage.error || 'Unknown error'}`
      });
      
      // Send message with italic text
      const italicMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'This message has *italic text* in it',
        formatting: {
          italic: [[17, 28]] // "italic text"
        }
      });
      
      this.results.formatting.push({
        test: 'Send message with italic formatting',
        success: italicMessage.success,
        details: italicMessage.success ?
          `Italic message sent: ${italicMessage.messageId}` :
          `Failed: ${italicMessage.error || 'Unknown error'}`
      });
      
      // Send message with inline code
      const codeMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'Use the `console.log()` function',
        formatting: {
          code: [[8, 21]] // "console.log()"
        }
      });
      
      this.results.formatting.push({
        test: 'Send message with inline code',
        success: codeMessage.success,
        details: codeMessage.success ?
          `Code message sent: ${codeMessage.messageId}` :
          `Failed: ${codeMessage.error || 'Unknown error'}`
      });
      
      // Send message with code block
      const codeBlockMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'Here is some TypeScript code:\n```typescript\nfunction hello(): string {\n  return "Hello, World!";\n}\n```',
        formatting: {
          codeBlock: [[30, 89, 'typescript']] // Code block with language
        }
      });
      
      this.results.formatting.push({
        test: 'Send message with code block',
        success: codeBlockMessage.success,
        details: codeBlockMessage.success ?
          `Code block message sent: ${codeBlockMessage.messageId}` :
          `Failed: ${codeBlockMessage.error || 'Unknown error'}`
      });
      
      // Send message with mixed formatting
      const mixedMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: 'This has **bold**, *italic*, and `code` all together',
        formatting: {
          bold: [[9, 14]], // "bold"
          italic: [[18, 24]], // "italic"
          code: [[31, 35]] // "code"
        }
      });
      
      this.results.formatting.push({
        test: 'Send message with mixed formatting',
        success: mixedMessage.success,
        details: mixedMessage.success ?
          `Mixed formatting message sent: ${mixedMessage.messageId}` :
          `Failed: ${mixedMessage.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.formatting.push({
        test: 'Rich text formatting test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * Run all advanced features tests
   */
  async runAllTests(): Promise<void> {
    await this.initialize();
    
    await this.testMessageThreading();
    await this.testEmojiReactions();
    await this.testMentionsAndNotifications();
    await this.testFileAttachments();
    await this.testMessageEditing();
    await this.testRichTextFormatting();
  }

  /**
   * Display comprehensive test results
   */
  displayResults(): void {
    console.log(`\n🎯 DISCORD-SCALE CHAT ADVANCED FEATURES TEST RESULTS`);
    console.log('==================================================');
    
    const categories = [
      { name: 'Threading', tests: this.results.threading, icon: '🧵' },
      { name: 'Reactions', tests: this.results.reactions, icon: '👍' },
      { name: 'Mentions', tests: this.results.mentions, icon: '📢' },
      { name: 'Attachments', tests: this.results.attachments, icon: '📎' },
      { name: 'Editing', tests: this.results.editing, icon: '✏️' },
      { name: 'Formatting', tests: this.results.formatting, icon: '💄' }
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
        console.log(`  ${status} ${test.test}: ${test.details}`);
      });
    });
    
    console.log(`\n📊 OVERALL SUMMARY: ${totalPassed}/${totalTests} advanced features tests passed`);
    console.log(`Success Rate: ${((totalPassed / totalTests) * 100).toFixed(1)}%`);
    
    if (totalPassed === totalTests) {
      console.log('🎉 ✅ ALL DISCORD-SCALE ADVANCED FEATURES WORKING!');
      console.log('🚀 Chat system has comprehensive Discord-level functionality');
      console.log('✨ Ready for production-scale chat applications');
    } else {
      console.log('⚠️  Some advanced features need implementation');
      console.log('🔧 Focus on failing tests to achieve Discord-scale parity');
      console.log('📋 These features are essential for modern chat applications');
    }
  }
}

// Main execution
async function runAdvancedChatFeaturesTests(): Promise<void> {
  const testRunner = new AdvancedChatFeaturesTest();
  
  try {
    await testRunner.runAllTests();
    testRunner.displayResults();
    
  } catch (error) {
    console.error('💥 Advanced chat features test execution failed:', error);
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (require.main === module) {
  runAdvancedChatFeaturesTests().catch(error => {
    console.error('💥 Test runner failed:', error);
    process.exit(1);
  });
}

export { runAdvancedChatFeaturesTests, AdvancedChatFeaturesTest };