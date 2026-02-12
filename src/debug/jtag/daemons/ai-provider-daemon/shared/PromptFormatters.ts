/**
 * Prompt Formatters - Universal prompt formatting for different model families
 *
 * ## Architecture
 *
 * This module provides a centralized way to format prompts for different model families.
 * Each model config specifies its `promptFormat` type, and adapters use this module to
 * format messages correctly before sending them to the model.
 *
 * ## Supported Formats
 *
 * - **base**: Base models (GPT-2, Llama base): "User: ...\n\nAssistant:"
 * - **chatml**: ChatML format: "<|im_start|>user\n...<|im_end|>"
 * - **llama2**: Llama-2 chat: "[INST] ... [/INST]"
 * - **alpaca**: Alpaca format: "### Instruction:\n...\n\n### Response:"
 * - **openai**: OpenAI native messages array (no formatting needed)
 * - **anthropic**: Anthropic native messages array (no formatting needed)
 *
 * ## Usage
 *
 * 1. **Model Config** (PersonaModelConfigs.ts):
 *    ```typescript
 *    'sentinel': {
 *      provider: 'sentinel',
 *      model: 'gpt2',
 *      promptFormat: 'base'  // Specify format here
 *    }
 *    ```
 *
 * 2. **Adapter** (SentinelAdapter.ts, CandleAdapter.ts, etc.):
 *    ```typescript
 *    import { formatPrompt } from '../shared/PromptFormatters';
 *
 *    // Get format from model config
 *    const format = modelConfig.promptFormat ?? 'base';
 *
 *    // Format messages
 *    const formatted = formatPrompt(request.messages, format);
 *    ```
 *
 * ## Design Principles
 *
 * - **Model property, not provider property**: promptFormat belongs to the model,
 *   not the provider. Same provider can serve different model families.
 *
 * - **Centralized formatting**: All formatting logic lives here, not scattered
 *   across adapters. Makes it easy to add new formats.
 *
 * - **Type-safe**: Uses PromptFormat enum from UserCreateTypes.ts to ensure
 *   only valid formats are used.
 */

import type { PromptFormat } from '../../../commands/user/create/shared/UserCreateTypes';

/**
 * Message interface compatible with AIProviderTypesV2.TextGenerationMessage
 * We accept either string or ContentPart[] but convert ContentPart[] to string internally
 */
export interface ChatMessage {
  readonly role: 'system' | 'user' | 'assistant';
  readonly content: string | readonly { readonly type: string; readonly text?: string }[];
}

/**
 * Convert content (string or ContentPart[]) to plain string
 */
function contentToString(content: string | readonly { readonly type: string; readonly text?: string }[]): string {
  if (typeof content === 'string') {
    return content;
  }
  // Extract text from ContentPart array
  return content
    .filter(part => part.type === 'text' && part.text)
    .map(part => part.text!)
    .join(' ');
}

/**
 * Format messages according to model's expected prompt format
 *
 * @param messages - Array of chat messages
 * @param format - Target prompt format
 * @returns Formatted prompt string (or messages array for native formats)
 */
export function formatPrompt(messages: readonly ChatMessage[], format: PromptFormat): string | readonly ChatMessage[] {
  switch (format) {
    case 'openai':
    case 'anthropic':
      // These providers expect native messages array - no formatting needed
      return messages;

    case 'base':
      return formatBaseModel(messages);

    case 'chatml':
      return formatChatML(messages);

    case 'llama2':
      return formatLlama2(messages);

    case 'alpaca':
      return formatAlpaca(messages);

    default:
      // Fallback to base format for unknown types
      console.warn(`Unknown prompt format: ${format}, falling back to base format`);
      return formatBaseModel(messages);
  }
}

/**
 * Format for base models (GPT-2, base Llama, etc.)
 * Format: "System: ...\n\nUser: ...\n\nAssistant: ...\n\nUser: ...\n\nAssistant:"
 */
function formatBaseModel(messages: readonly ChatMessage[]): string {
  const parts: string[] = [];

  for (const msg of messages) {
    const content = contentToString(msg.content);
    if (msg.role === 'system') {
      parts.push(`System: ${content}`);
    } else if (msg.role === 'user') {
      parts.push(`User: ${content}`);
    } else if (msg.role === 'assistant') {
      parts.push(`Assistant: ${content}`);
    }
  }

  // Add final "Assistant:" to prompt the model to respond
  parts.push('Assistant:');

  return parts.join('\n\n');
}

/**
 * Format for ChatML (used by many open source models)
 * Format: "<|im_start|>system\n...<|im_end|>\n<|im_start|>user\n...<|im_end|>"
 */
function formatChatML(messages: readonly ChatMessage[]): string {
  const parts: string[] = [];

  for (const msg of messages) {
    parts.push(`<|im_start|>${msg.role}\n${msg.content}<|im_end|>`);
  }

  // Add assistant start token to prompt response
  parts.push('<|im_start|>assistant');

  return parts.join('\n');
}

/**
 * Format for Llama-2 chat models
 * Format: "[INST] <<SYS>>\n...\n<</SYS>>\n\n... [/INST] ... [INST] ... [/INST]"
 */
function formatLlama2(messages: readonly ChatMessage[]): string {
  const parts: string[] = [];
  let systemPrompt = '';

  // Extract system prompt if present
  const systemMsg = messages.find(m => m.role === 'system');
  if (systemMsg) {
    systemPrompt = `<<SYS>>\n${systemMsg.content}\n<</SYS>>\n\n`;
  }

  // Format conversation
  let isFirst = true;
  for (const msg of messages) {
    if (msg.role === 'system') continue; // Already handled

    if (msg.role === 'user') {
      const prefix = isFirst ? systemPrompt : '';
      parts.push(`[INST] ${prefix}${msg.content} [/INST]`);
      isFirst = false;
    } else if (msg.role === 'assistant') {
      parts.push(` ${msg.content} `);
    }
  }

  return parts.join('');
}

/**
 * Format for Alpaca-style models
 * Format: "### Instruction:\n...\n\n### Response:\n...\n\n### Instruction:\n...\n\n### Response:"
 */
function formatAlpaca(messages: readonly ChatMessage[]): string {
  const parts: string[] = [];
  let systemPrompt = '';

  // Extract system prompt if present
  const systemMsg = messages.find(m => m.role === 'system');
  if (systemMsg) {
    systemPrompt = `${systemMsg.content}\n\n`;
  }

  // Add system prompt at the beginning if it exists
  if (systemPrompt) {
    parts.push(systemPrompt.trim());
  }

  // Format conversation
  for (const msg of messages) {
    if (msg.role === 'system') continue; // Already handled

    if (msg.role === 'user') {
      parts.push(`### Instruction:\n${msg.content}`);
    } else if (msg.role === 'assistant') {
      parts.push(`### Response:\n${msg.content}`);
    }
  }

  // Add final response prompt
  parts.push('### Response:');

  return parts.join('\n\n');
}

/**
 * Check if a prompt format requires string formatting (vs native messages array)
 */
export function requiresStringFormatting(format: PromptFormat): boolean {
  return format !== 'openai' && format !== 'anthropic';
}

/**
 * Estimate token count for a message (rough approximation: 1 token ≈ 4 characters)
 */
function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

/**
 * Truncate messages to fit within context window
 *
 * Strategy:
 * - Always keep system message (if present)
 * - Always keep last user message (the current question)
 * - Truncate from the beginning of conversation history
 * - Reserve space for prompt formatting overhead (~50 tokens)
 *
 * @param messages - Array of chat messages
 * @param contextWindow - Maximum input tokens allowed
 * @param format - Prompt format (affects overhead calculation)
 * @returns Truncated messages array that fits within context window
 */
export function truncateMessages(
  messages: readonly ChatMessage[],
  contextWindow: number,
  format: PromptFormat
): ChatMessage[] {
  if (messages.length === 0) {
    return [];
  }

  // Reserve tokens for formatting overhead (varies by format)
  const formattingOverhead = format === 'llama2' ? 100 : 50;
  const availableTokens = contextWindow - formattingOverhead;

  // Separate system message and conversation
  const systemMsg = messages.find(m => m.role === 'system');
  const conversationMsgs = messages.filter(m => m.role !== 'system');

  // Always keep system message
  let usedTokens = systemMsg ? estimateTokens(contentToString(systemMsg.content)) : 0;

  // Always keep last user message (current question)
  const lastUserMsg = conversationMsgs.length > 0 ? conversationMsgs[conversationMsgs.length - 1] : null;
  if (lastUserMsg) {
    usedTokens += estimateTokens(contentToString(lastUserMsg.content));
  }

  // Check if we're already over budget with just system + last message
  if (usedTokens > availableTokens) {
    console.warn(`⚠️  System + last message (${usedTokens} tokens) exceeds context window (${availableTokens} tokens)`);
    // Return minimal set: system (truncated if needed) + last message (truncated if needed)
    const result: ChatMessage[] = [];
    if (systemMsg) {
      const maxSystemTokens = Math.floor(availableTokens * 0.3); // 30% for system
      const systemChars = maxSystemTokens * 4;
      const systemContent = contentToString(systemMsg.content);
      result.push({
        role: 'system',
        content: systemContent.substring(0, systemChars) + (systemContent.length > systemChars ? '...' : '')
      });
    }
    if (lastUserMsg) {
      const resultContent = result.length > 0 ? contentToString(result[0].content) : '';
      const maxUserTokens = availableTokens - (result.length > 0 ? estimateTokens(resultContent) : 0);
      const userChars = maxUserTokens * 4;
      const userContent = contentToString(lastUserMsg.content);
      result.push({
        role: lastUserMsg.role,
        content: userContent.substring(0, userChars) + (userContent.length > userChars ? '...' : '')
      });
    }
    return result;
  }

  // Add conversation history from most recent to oldest (excluding last message already counted)
  const historyMsgs: ChatMessage[] = [];
  for (let i = conversationMsgs.length - 2; i >= 0; i--) {
    const msg = conversationMsgs[i];
    const msgContent = contentToString(msg.content);
    const msgTokens = estimateTokens(msgContent);

    if (usedTokens + msgTokens <= availableTokens) {
      historyMsgs.unshift(msg); // Add to beginning
      usedTokens += msgTokens;
    } else {
      // No more room, stop adding history
      break;
    }
  }

  // Assemble final messages array
  const result: ChatMessage[] = [];
  if (systemMsg) {
    result.push(systemMsg);
  }
  result.push(...historyMsgs);
  if (lastUserMsg) {
    result.push(lastUserMsg);
  }

  const droppedCount = conversationMsgs.length - historyMsgs.length - (lastUserMsg ? 1 : 0);
  if (droppedCount > 0) {
    console.log(`📏 Truncated ${droppedCount} messages to fit ${contextWindow} token context window (using ${usedTokens} tokens)`);
  }

  return result;
}
