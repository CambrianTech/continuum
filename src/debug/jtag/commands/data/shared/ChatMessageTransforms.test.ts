/**
 * Chat Message Transformation Tests
 *
 * Verifies media extraction logic works correctly
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { extractMediaFromMessage, isChatMessageEntity } from './ChatMessageTransforms';
import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { MediaItem } from '../../../system/data/entities/ChatMessageEntity';

describe('ChatMessageTransforms', () => {
  describe('isChatMessageEntity', () => {
    it('should return true for chat_messages collection', () => {
      const entity = new ChatMessageEntity();
      const result = isChatMessageEntity('chat_messages', entity);
      assert.strictEqual(result, true);
    });

    it('should return false for other collections', () => {
      const entity = new ChatMessageEntity();
      assert.strictEqual(isChatMessageEntity('users', entity), false);
      assert.strictEqual(isChatMessageEntity('rooms', entity), false);
    });
  });

  describe('extractMediaFromMessage', () => {
    it('should extract media from message entity', () => {
      const message = new ChatMessageEntity();
      const mediaItems: MediaItem[] = [
        { type: 'image', url: 'https://example.com/image.png' },
        { type: 'video', url: 'https://example.com/video.mp4' }
      ];
      
      message.content = {
        text: 'Check out these files',
        media: mediaItems
      };

      const result = extractMediaFromMessage(message);

      assert.strictEqual(result.media.length, 2);
      assert.strictEqual(result.media[0].type, 'image');
      assert.strictEqual(result.media[1].type, 'video');
      
      // Verify media is cleared from entity
      assert.strictEqual((result.entity as ChatMessageEntity).content.media?.length, 0);
      
      // Verify text content is preserved
      assert.strictEqual((result.entity as ChatMessageEntity).content.text, 'Check out these files');
    });

    it('should return empty media array when no media present', () => {
      const message = new ChatMessageEntity();
      message.content = {
        text: 'Just text, no media',
        media: []
      };

      const result = extractMediaFromMessage(message);

      assert.strictEqual(result.media.length, 0);
      assert.strictEqual((result.entity as ChatMessageEntity).content.text, 'Just text, no media');
    });

    it('should handle entity without media field', () => {
      const message = new ChatMessageEntity();
      message.content = {
        text: 'No media field at all'
      };

      const result = extractMediaFromMessage(message);

      assert.strictEqual(result.media.length, 0);
      assert.strictEqual(result.entity, message);
    });

    it('should preserve entity prototype', () => {
      const message = new ChatMessageEntity();
      message.content = {
        text: 'Test',
        media: [{ type: 'image', url: 'test.png' }]
      };

      const result = extractMediaFromMessage(message);

      // Verify prototype chain is preserved
      assert.strictEqual(Object.getPrototypeOf(result.entity), Object.getPrototypeOf(message));
      assert.ok(result.entity instanceof ChatMessageEntity);
    });

    it('should not share media array references', () => {
      const message = new ChatMessageEntity();
      const originalMedia: MediaItem[] = [
        { type: 'image', url: 'test.png' }
      ];
      
      message.content = {
        text: 'Test',
        media: originalMedia
      };

      const result = extractMediaFromMessage(message);

      // Modify the extracted media
      result.media.push({ type: 'video', url: 'new.mp4' });

      // Original should not be affected (new array created)
      assert.strictEqual(result.media.length, 2);
      assert.strictEqual(originalMedia.length, 1);
    });
  });
});
