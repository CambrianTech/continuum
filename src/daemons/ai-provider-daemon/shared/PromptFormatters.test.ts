/**
 * Unit tests for PromptFormatters
 */

import { describe, it, expect } from 'vitest';
import { formatPrompt, requiresStringFormatting, type ChatMessage } from './PromptFormatters';

describe('PromptFormatters', () => {
  const testMessages: ChatMessage[] = [
    { role: 'system', content: 'You are a helpful assistant.' },
    { role: 'user', content: 'Hello!' },
    { role: 'assistant', content: 'Hi there!' },
    { role: 'user', content: 'How are you?' }
  ];

  describe('formatPrompt - base format', () => {
    it('should format messages for base models', () => {
      const result = formatPrompt(testMessages, 'base');
      expect(result).toBe(
        'System: You are a helpful assistant.\n\n' +
        'User: Hello!\n\n' +
        'Assistant: Hi there!\n\n' +
        'User: How are you?\n\n' +
        'Assistant:'
      );
    });

    it('should handle messages without system prompt', () => {
      const messages: ChatMessage[] = [
        { role: 'user', content: 'Hello!' },
        { role: 'assistant', content: 'Hi!' }
      ];
      const result = formatPrompt(messages, 'base');
      expect(result).toBe(
        'User: Hello!\n\n' +
        'Assistant: Hi!\n\n' +
        'Assistant:'
      );
    });
  });

  describe('formatPrompt - chatml format', () => {
    it('should format messages in ChatML format', () => {
      const result = formatPrompt(testMessages, 'chatml');
      expect(result).toBe(
        '<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n' +
        '<|im_start|>user\nHello!<|im_end|>\n' +
        '<|im_start|>assistant\nHi there!<|im_end|>\n' +
        '<|im_start|>user\nHow are you?<|im_end|>\n' +
        '<|im_start|>assistant'
      );
    });
  });

  describe('formatPrompt - llama2 format', () => {
    it('should format messages in Llama-2 format', () => {
      const result = formatPrompt(testMessages, 'llama2');
      expect(result).toBe(
        '[INST] <<SYS>>\nYou are a helpful assistant.\n<</SYS>>\n\nHello! [/INST] Hi there! ' +
        '[INST] How are you? [/INST]'
      );
    });

    it('should handle messages without system prompt', () => {
      const messages: ChatMessage[] = [
        { role: 'user', content: 'Hello!' },
        { role: 'assistant', content: 'Hi!' }
      ];
      const result = formatPrompt(messages, 'llama2');
      expect(result).toBe('[INST] Hello! [/INST] Hi! ');
    });
  });

  describe('formatPrompt - alpaca format', () => {
    it('should format messages in Alpaca format', () => {
      const result = formatPrompt(testMessages, 'alpaca');
      expect(result).toBe(
        'You are a helpful assistant.\n\n' +
        '### Instruction:\nHello!\n\n' +
        '### Response:\nHi there!\n\n' +
        '### Instruction:\nHow are you?\n\n' +
        '### Response:'
      );
    });
  });

  describe('formatPrompt - openai/anthropic formats', () => {
    it('should return messages array unchanged for openai', () => {
      const result = formatPrompt(testMessages, 'openai');
      expect(result).toEqual(testMessages);
    });

    it('should return messages array unchanged for anthropic', () => {
      const result = formatPrompt(testMessages, 'anthropic');
      expect(result).toEqual(testMessages);
    });
  });

  describe('requiresStringFormatting', () => {
    it('should return true for formats that need string conversion', () => {
      expect(requiresStringFormatting('base')).toBe(true);
      expect(requiresStringFormatting('chatml')).toBe(true);
      expect(requiresStringFormatting('llama2')).toBe(true);
      expect(requiresStringFormatting('alpaca')).toBe(true);
    });

    it('should return false for native message array formats', () => {
      expect(requiresStringFormatting('openai')).toBe(false);
      expect(requiresStringFormatting('anthropic')).toBe(false);
    });
  });

  describe('edge cases', () => {
    it('should handle empty messages array', () => {
      const result = formatPrompt([], 'base');
      expect(result).toBe('Assistant:');
    });

    it('should handle single user message', () => {
      const messages: ChatMessage[] = [{ role: 'user', content: 'Test' }];
      const result = formatPrompt(messages, 'base');
      expect(result).toBe('User: Test\n\nAssistant:');
    });

    it('should fallback to base format for unknown type', () => {
      const result = formatPrompt(testMessages, 'unknown' as any);
      expect(typeof result).toBe('string');
      expect(result).toContain('User:');
      expect(result).toContain('Assistant:');
    });
  });
});
