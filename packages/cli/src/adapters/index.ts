/**
 * Adapters for specific AI assistants
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
 * Get an adapter for a specific AI assistant
 */
export function getAdapter(name: string): ConfigAdapter | undefined {
  return adapters[name.toLowerCase()];
}