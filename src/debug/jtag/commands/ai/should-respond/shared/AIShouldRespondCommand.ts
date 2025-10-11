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

    return `**Your Task**: Decide if "${personaName}" should respond to the message marked with >>> arrows <<< above.

**CRITICAL: Distinguish Questions from Answers**:
- A QUESTION asks for information (ends with ?, uses "how", "what", "why", etc.)
- An ANSWER provides information (explains, describes, gives details)
- Someone asking "What is X?" is a QUESTION - they need an answer!
- Someone saying "X is Y because..." is an ANSWER - they provided information

**Think like a human in a group chat**:
- If someone ASKED a question and nobody ANSWERED yet → RESPOND (0.8 confidence)
- If someone already gave a good answer → Stay quiet (even if you'd phrase it differently)
- If you'd just be repeating the same idea → Stay quiet
- Only speak if you have genuinely NEW information

**Critical Decision Rules**:
1. **Is this a QUESTION that needs answering?**
   - Questions end with "?" or ask "how", "what", "why", "when", "where"
   - If it's a question and nobody answered yet → confidence = 0.8 (RESPOND!)
   - If it's NOT a question → Continue to rule 2

2. **Have I ACTUALLY ANSWERED this specific question before?**
   - Meta-commentary like "let's discuss X" is NOT an answer
   - Only count it if I gave specific technical details/explanation
   - If I ACTUALLY answered → confidence = 0.0 (STAY SILENT)
   - If I only mentioned it in passing → Continue to rule 3

3. **Has someone else ACTUALLY answered this?**
   - Meta-commentary doesn't count - only real answers with details
   - If YES and answer is complete → confidence = 0.0 (STAY SILENT)
   - If YES but answer is WRONG → confidence = 0.9 (CORRECT IT)
   - If NO → Continue to rule 4

4. **Would I just be rephrasing what was said?**
   - Same explanation, different words? → confidence = 0.0
   - Same concept, different analogy? → confidence = 0.0
   - Adding minor details to complete answer? → confidence = 0.0

5. **Mentioned by name?** → confidence = 0.9 (respond even if redundant)

**Examples**:
- User asks: "What is X?" → Nobody answered yet → **RESPOND (0.8)** - this is an unanswered question!
- User asks: "What is X?" → Helper AI explains X clearly → You would also explain X → **STAY SILENT (0.0)**
- User asks: "What is X?" → Helper AI says "X is Y" (WRONG) → You know X is Z → **CORRECT IT (0.9)**
- User asks: "What is X and why use it?" → Helper AI explains what X is → You explain WHY to use it → **ADD VALUE (0.7)**

**Response Format** (JSON only):
{
  "shouldRespond": true/false,
  "confidence": 0.0-1.0,
  "reason": "brief explanation - is this a question? has anyone answered?",
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
