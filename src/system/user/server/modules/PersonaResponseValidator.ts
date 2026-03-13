/**
 * PersonaResponseValidator - Response cleaning and validation gates
 *
 * Extracted from PersonaResponseGenerator to isolate validation logic.
 * Delegates to Rust IPC for actual validation (garbage, loop, truncated tool, semantic loop).
 */

import type { RustCognitionBridge } from './RustCognitionBridge';
import type { ConversationMessage } from '@shared/generated/persona';

export interface ValidationContext {
  responseText: string;
  hasToolCalls: boolean;
  conversationHistory: ConversationMessage[];
}

export interface CleanResult {
  text: string;
  thinking?: string;
  wasCleaned: boolean;
}

export interface ValidationResult {
  passed: boolean;
  gate?: string;
  confidence: number;
  reason: string;
  /** Raw Rust validation result for detailed gate inspection */
  raw: Record<string, unknown>;
}

export class PersonaResponseValidator {
  private _rustBridge: RustCognitionBridge | null = null;
  private personaName: string;
  private log: (message: string, ...args: unknown[]) => void;

  constructor(personaName: string, log: (message: string, ...args: unknown[]) => void) {
    this.personaName = personaName;
    this.log = log;
  }

  setRustBridge(bridge: RustCognitionBridge): void {
    this._rustBridge = bridge;
  }

  private get rustBridge(): RustCognitionBridge {
    if (!this._rustBridge) throw new Error('Rust bridge not initialized — cannot validate response');
    return this._rustBridge;
  }

  /**
   * Clean AI response via Rust IPC — strips name prefixes, extracts thinking tags.
   * Returns cleaned text and any extracted thinking content.
   */
  async cleanResponse(rawText: string): Promise<CleanResult> {
    const cleaned = await this.rustBridge.cleanResponse(rawText);

    if (cleaned.was_cleaned && cleaned.text.length === 0) {
      this.log(`⚠️ ${this.personaName}: [VALIDATE] Response empty after cleaning — suppressing`);
      return { text: '', thinking: cleaned.thinking, wasCleaned: true };
    }

    return {
      text: cleaned.was_cleaned ? cleaned.text : rawText,
      thinking: cleaned.thinking,
      wasCleaned: cleaned.was_cleaned,
    };
  }

  /**
   * Run combined validation gates (1 Rust IPC call).
   * Gates: garbage detection, response loop, truncated tool call, semantic loop.
   */
  async validate(ctx: ValidationContext): Promise<ValidationResult> {
    const validation = await this.rustBridge.validateResponse(
      ctx.responseText,
      ctx.hasToolCalls,
      ctx.conversationHistory,
    );

    if (!validation.passed) {
      const gate = validation.gate_failed ?? 'unknown';
      this.log(`🚫 ${this.personaName}: [VALIDATE] Gate FAILED: ${gate} (${validation.total_time_us}us)`);

      const confidence = gate === 'garbage' ? validation.garbage_result.score
        : gate === 'response_loop' ? 0.9
        : gate === 'truncated_tool_call' ? 0.95
        : gate === 'semantic_loop' ? validation.semantic_result.similarity
        : 0.8;

      const reason = gate === 'garbage' ? `Garbage output: ${validation.garbage_result.reason} - ${validation.garbage_result.details}`
        : gate === 'response_loop' ? `Response loop detected - ${validation.loop_duplicate_count} duplicates`
        : gate === 'truncated_tool_call' ? 'Truncated tool call detected - response cut off mid-tool-call'
        : gate === 'semantic_loop' ? validation.semantic_result.reason
        : `Validation failed: ${gate}`;

      return { passed: false, gate, confidence, reason, raw: validation };
    }

    return { passed: true, confidence: 1.0, reason: 'All gates passed', raw: validation };
  }

  /**
   * Determine if a garbage gate failure means the response should be treated as an error
   * (vs a redundant/silent response for loop-type gates).
   */
  isHardFailure(gate: string): boolean {
    return gate === 'garbage';
  }
}
