/**
 * Coding Agent Provider Architecture
 *
 * Plugin pattern: providers self-register with the registry.
 * Adding a new provider = implement CodingAgentProvider + register().
 * No Rust changes, no step type changes, no central lists.
 */

export type {
  CodingAgentConfig,
  CodingAgentInteraction,
  CodingAgentProgressEvent,
  CodingAgentProvider,
  CodingAgentResult,
  CodingAgentToolCall,
} from './CodingAgentProvider';

export { CodingAgentRegistry } from './CodingAgentRegistry';
export { ClaudeCodeProvider } from './ClaudeCodeProvider';
export { LocalAgentProvider } from './LocalAgentProvider';

// Self-register built-in providers
import { CodingAgentRegistry } from './CodingAgentRegistry';
import { ClaudeCodeProvider } from './ClaudeCodeProvider';
import { LocalAgentProvider } from './LocalAgentProvider';

CodingAgentRegistry.register(new ClaudeCodeProvider());
CodingAgentRegistry.register(new LocalAgentProvider());
