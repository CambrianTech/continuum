/**
 * AI Generate Command - Shared Implementation
 * ============================================
 *
 * Shared command logic for text generation via AIProviderDaemon
 * Works in both browser and server environments
 *
 * Supports two modes:
 * 1. Direct messages: Provide messages array directly
 * 2. RAG context: Provide roomId + personaId, auto-builds RAG context
 *
 * Preview mode: Set preview=true to see what would be sent to LLM without calling it
 * This ensures preview shows EXACTLY what actual generation uses (no drift possible)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIGenerateParams, AIGenerateResult } from './AIGenerateTypes';
import type { TextGenerationRequest } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

export abstract class AIGenerateCommand extends CommandBase<AIGenerateParams, AIGenerateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai-generate', context, subpath, commander);
  }

  /**
   * Execute AI generation - implemented in server, browser delegates
   */
  abstract execute(params: AIGenerateParams): Promise<AIGenerateResult>;

  /**
   * Format request preview for human-readable output
   * Shows EXACTLY what would be sent to LLM
   * Protected so server implementation can use it
   */
  protected formatRequestPreview(request: TextGenerationRequest, ragContext?: any): string {
    const lines: string[] = [];

    lines.push('');
    lines.push('═══════════════════════════════════════════════════════');
    lines.push('  🤖 AI GENERATE PREVIEW: EXACT LLM REQUEST');
    lines.push('═══════════════════════════════════════════════════════');
    lines.push('');
    lines.push('This is EXACTLY what would be sent to AIProviderDaemon:');
    lines.push('');

    // Show the actual API request structure
    lines.push('📨 REQUEST STRUCTURE:');
    lines.push('   {');
    lines.push('     messages: [...],                    // Array shown below');
    lines.push(`     model: "${request.model}",`);
    lines.push(`     temperature: ${request.temperature},`);
    lines.push(`     maxTokens: ${request.maxTokens},`);
    lines.push(`     provider: "${request.provider}"`);
    lines.push('   }');
    lines.push('');

    // Show the messages array
    lines.push('📋 MESSAGES ARRAY (what LLM sees):');
    lines.push(`   Total: ${request.messages.length} messages`);
    lines.push('');

    request.messages.forEach((msg, index) => {
      const roleSymbol = msg.role === 'system' ? '⚙️' : msg.role === 'assistant' ? '🤖' : '👤';
      const timestamp = (msg as any).timestamp ? ` @ ${new Date((msg as any).timestamp).toLocaleTimeString()}` : '';

      lines.push(`   ┌─ Message ${index + 1} ${roleSymbol}────────────────────────────────`);
      lines.push(`   │ role: "${msg.role}"`);
      if ((msg as any).name) lines.push(`   │ name: "${(msg as any).name}"`);
      if ((msg as any).timestamp) lines.push(`   │ timestamp: ${(msg as any).timestamp}${timestamp}`);
      lines.push(`   │ content:`);

      // Show full content for system prompt, truncate others
      const maxLength = msg.role === 'system' ? Infinity : 500;
      const contentText = typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content);
      const content = contentText.length > maxLength
        ? contentText.substring(0, maxLength) + `\n... (${contentText.length - maxLength} more chars)`
        : contentText;

      content.split('\n').forEach((line: string) => {
        lines.push(`   │   ${line}`);
      });
      lines.push(`   └────────────────────────────────────────────────────`);
      lines.push('');
    });

    // Show RAG metadata if available
    if (ragContext) {
      lines.push('📊 RAG CONTEXT METADATA:');
      lines.push(`   Domain: ${ragContext.domain}`);
      lines.push(`   Context ID: ${ragContext.contextId}`);
      lines.push(`   Persona: ${ragContext.identity.name}`);
      lines.push(`   Messages in Context: ${ragContext.metadata.messageCount}`);
      lines.push(`   Artifacts: ${ragContext.metadata.artifactCount}`);
      lines.push(`   Memories: ${ragContext.metadata.memoryCount}`);
      lines.push('');
    }

    lines.push('═══════════════════════════════════════════════════════');
    lines.push('');

    return lines.join('\n');
  }
}
