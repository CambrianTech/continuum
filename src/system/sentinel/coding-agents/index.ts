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

// Self-register built-in providers
import { CodingAgentRegistry } from './CodingAgentRegistry';
import { ClaudeCodeProvider } from './ClaudeCodeProvider';

CodingAgentRegistry.register(new ClaudeCodeProvider());
