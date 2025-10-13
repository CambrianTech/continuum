/**
 * AI Should-Respond Command - Shared Logic
 *
 * Sentinel/Coordinator pattern: Use AI to intelligently gate persona responses
 *
 * Uses llama3.2:3b (validated, fast, cheap) to analyze full conversation context
 * and decide if a persona should respond to a message.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { AIShouldRespondParams, AIShouldRespondResult } from './AIShouldRespondTypes';

export abstract class AIShouldRespondCommand extends CommandBase<CommandParams, CommandResult> {
  static readonly commandName = 'ai/should-respond';

  /**
   * Build the gating instruction that gets appended AFTER the conversation history
   *
   * The LLM will see:
   * 1. System: "You are a conversation coordinator..."
   * 2. [Full conversation history as proper messages]
   * 3. User: [This gating instruction]
   */
  protected buildGatingInstruction(params: AIShouldRespondParams): string {
    const { personaName } = params;

    return `You are "${personaName}" in a group chat. Should you respond to the message marked >>> like this <<<?

Think like a human:
- If someone needs help/info and nobody helped yet → respond
- If someone already got a good answer → stay quiet
- If you'd just repeat what was said → stay quiet
- If the answer given is WRONG → definitely respond to correct it

Return JSON only:
{
  "shouldRespond": true/false,
  "confidence": 0.0-1.0,
  "reason": "brief why/why not"
}`;
  }

  /**
   * DEPRECATED: Old method that flattened conversation to string
   * Kept for reference but should not be used
   */
  protected buildGatingPrompt(params: AIShouldRespondParams): string {
    const { personaName, ragContext, triggerMessage } = params;

    // Validate ragContext
    if (!ragContext) {
      throw new Error('ragContext is required for buildGatingPrompt');
    }

    // Extract conversation history from RAG context
    // IMPORTANT: Take more context to see past AI chatter, but highlight the trigger message
    const recentMessages = ragContext.conversationHistory?.slice(-15) ?? [];

    // Build conversation text with the trigger message HIGHLIGHTED
    const conversationLines = recentMessages.map(msg => {
      const line = `${msg.name ?? msg.role}: ${msg.content}`;
      // Check if this is the trigger message (match by content and sender)
      const isTrigger = msg.content === triggerMessage.content &&
                       msg.name === triggerMessage.senderName;
      return isTrigger ? `>>> ${line} <<<` : line;
    });

    // If trigger message isn't in recent history, append it explicitly
    const triggerInHistory = recentMessages.some(msg =>
      msg.content === triggerMessage.content &&
      msg.name === triggerMessage.senderName
    );

    if (!triggerInHistory) {
      conversationLines.push(`>>> ${triggerMessage.senderName}: ${triggerMessage.content} <<<`);
    }

    const conversationText = conversationLines.join('\n');

    // Extract persona identity for context
    const members = `${ragContext.identity?.name ?? personaName} and others`;

    return `You are a conversation coordinator for a multi-party chat room.

**Your Job**: Decide if "${personaName}" should respond to the message marked with >>> arrows <<<.

**Room Members**: ${members}

**Recent Conversation** (message to evaluate is marked with >>> arrows <<<):
${conversationText}

**Decision Rules**:
1. If ${personaName} is directly mentioned by name → respond
2. If this is a question and ${personaName} has unique expertise → respond
3. If someone else JUST answered the same question → DON'T respond (avoid spam)
4. If ${personaName} has spoken in 3+ of last 5 messages → DON'T respond (dominating)
5. If message is off-topic for ${personaName}'s expertise → DON'T respond
6. When in doubt, err on the side of SILENCE (better to miss one than spam)

**Response Format** (JSON only):
{
  "shouldRespond": true/false,
  "confidence": 0.0-1.0,
  "reason": "brief explanation",
  "factors": {
    "mentioned": true/false,
    "questionAsked": true/false,
    "domainRelevant": true/false,
    "recentlySpoke": true/false,
    "othersAnswered": true/false
  }
}`;
  }

  /**
   * Parse AI response into structured result
   *
   * The AI should return JSON, but we'll handle both JSON and natural language
   */
  protected parseGatingResponse(aiText: string): Partial<AIShouldRespondResult> {
    try {
      // Try to extract JSON from response
      const jsonMatch = aiText.match(/\{[\s\S]*\}/);
      if (jsonMatch) {
        const parsed = JSON.parse(jsonMatch[0]);
        return {
          shouldRespond: parsed.shouldRespond ?? false,
          confidence: parsed.confidence ?? 0.5,
          reason: parsed.reason ?? 'No reason provided',
          factors: parsed.factors ?? {
            mentioned: false,
            questionAsked: false,
            domainRelevant: false,
            recentlySpoke: false,
            othersAnswered: false
          }
        };
      }

      // Fallback: Look for keywords in natural language response
      const lowerText = aiText.toLowerCase();
      const shouldRespond = lowerText.includes('should respond') ||
                           lowerText.includes('yes') ||
                           lowerText.includes('true');

      return {
        shouldRespond,
        confidence: 0.5,
        reason: aiText.slice(0, 200),
        factors: {
          mentioned: lowerText.includes('mentioned'),
          questionAsked: lowerText.includes('question'),
          domainRelevant: lowerText.includes('relevant') || lowerText.includes('expertise'),
          recentlySpoke: lowerText.includes('recent') || lowerText.includes('dominating'),
          othersAnswered: lowerText.includes('answered') || lowerText.includes('already')
        }
      };
    } catch (error) {
      console.error('Failed to parse gating AI response:', error);
      // Default to NOT responding on parse errors (fail safe)
      return {
        shouldRespond: false,
        confidence: 0.0,
        reason: 'Failed to parse AI response',
        factors: {
          mentioned: false,
          questionAsked: false,
          domainRelevant: false,
          recentlySpoke: false,
          othersAnswered: false
        }
      };
    }
  }
}
