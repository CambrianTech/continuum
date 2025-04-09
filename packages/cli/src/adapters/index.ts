/**
 * Adapters for different AI assistants
 */

import { ConfigAdapter } from '@continuum/core';
import { ClaudeAdapter } from './claude';
import { GPTAdapter } from './gpt';

// Registry of available adapters
const adapters: Record<string, ConfigAdapter> = {
  claude: new ClaudeAdapter(),
  gpt: new GPTAdapter()
};

/**
 * Get adapter for a specific assistant
 */
export function getAdapter(assistant: string): ConfigAdapter | undefined {
  return adapters[assistant.toLowerCase()];
}

/**
 * List available adapters
 */
export function listAvailableAdapters(): string[] {
  return Object.keys(adapters);
}

export { ClaudeAdapter, GPTAdapter };