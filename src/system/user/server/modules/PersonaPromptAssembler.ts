/**
 * PersonaPromptAssembler - LLM message array construction
 *
 * Extracted from PersonaResponseGenerator Phase 3.2.
 * Builds the complete message array from RAG context including:
 * - System prompt injection
 * - Vision artifact mapping (base64 for vision models, text descriptions for text-only)
 * - Conversation history with time gaps
 * - Identity reminder at end of context
 * - Voice mode instructions
 */

import type { ModelConfig } from '../../../data/entities/UserEntity';
import type { ContentPart, ChatMessage } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import type { MediaItem } from '../../../data/entities/ChatMessageEntity';
import { AICapabilityRegistry } from '../../../../daemons/ai-provider-daemon/shared/AICapabilityRegistry';
import { hasMediaMetadata } from '../../../rag/shared/RAGTypes';
import type { RAGContext } from '../../../rag/shared/RAGTypes';
import type { ProcessableMessage } from './QueueItemTypes';

export type LLMMessage = { role: 'system' | 'user' | 'assistant'; content: string | ChatMessage['content'] };

export class PersonaPromptAssembler {
  private personaName: string;
  private modelConfig: ModelConfig;
  private log: (message: string, ...args: any[]) => void;

  constructor(
    personaName: string,
    modelConfig: ModelConfig,
    log: (message: string, ...args: any[]) => void,
  ) {
    this.personaName = personaName;
    this.modelConfig = modelConfig;
    this.log = log;
  }

  /**
   * Build the complete LLM message array from RAG context.
   * Returns messages ready for AIProviderDaemon.generateText().
   */
  assembleMessages(
    fullRAGContext: RAGContext,
    originalMessage: ProcessableMessage,
  ): LLMMessage[] {
    const messages: LLMMessage[] = [];

    // System prompt from RAG builder
    const systemPrompt = fullRAGContext.identity.systemPrompt;
    this.log(`📋 ${this.personaName}: [ASSEMBLE] ${systemPrompt.length} chars (~${Math.ceil(systemPrompt.length / 4)} tokens), provider=${this.modelConfig.provider}`);

    messages.push({ role: 'system', content: systemPrompt });

    // Inject system-level image artifacts for vision models
    this.injectSystemArtifacts(messages, fullRAGContext);

    // Build artifact lookup maps for multimodal support
    const { artifactsByTimestampName } = this.buildArtifactMaps(fullRAGContext);

    // Add conversation history with time gaps
    this.addConversationHistory(messages, fullRAGContext, artifactsByTimestampName);

    // Identity reminder at END of context (recency bias)
    this.addIdentityReminder(messages);

    // Voice mode instructions
    this.addVoiceModeInstructions(messages, fullRAGContext, originalMessage);

    this.log(`✅ ${this.personaName}: [ASSEMBLE] LLM message array built (${messages.length} messages)`);
    return messages;
  }

  /**
   * Convert MediaItems to ContentPart blocks for inclusion in model messages.
   */
  mediaToContentParts(media: MediaItem[]): ContentPart[] {
    return media.map(m => {
      if (m.type === 'image') return { type: 'image' as const, image: m };
      if (m.type === 'audio') return { type: 'audio' as const, audio: m };
      if (m.type === 'video') return { type: 'video' as const, video: m };
      return { type: 'image' as const, image: m };
    });
  }

  private get hasVisionCapability(): boolean {
    return AICapabilityRegistry.getInstance().hasCapability(
      this.modelConfig.provider, this.modelConfig.model, 'image-input'
    );
  }

  private injectSystemArtifacts(messages: LLMMessage[], ragContext: RAGContext): void {
    if (!this.hasVisionCapability) return;

    const systemArtifacts = ragContext.artifacts.filter(
      a => a.type === 'screenshot' && a.base64 && !hasMediaMetadata(a)
    );

    if (systemArtifacts.length > 0) {
      const parts: ContentPart[] = [{ type: 'text', text: 'Current visual context:' }];
      for (const artifact of systemArtifacts) {
        const mimeType = (artifact.metadata?.mimeType as string) ?? 'image/jpeg';
        parts.push({ type: 'image', image: { base64: artifact.base64!, mimeType } });
      }
      messages.push({ role: 'user', content: parts });
      this.log(`🖼️  ${this.personaName}: Injected ${systemArtifacts.length} system-level screenshot(s) for vision model`);
    }
  }

  private buildArtifactMaps(ragContext: RAGContext) {
    type RAGArtifact = typeof ragContext.artifacts[number];
    const artifactsByMessageId = new Map<string, RAGArtifact[]>();
    const artifactsByTimestampName = new Map<string, RAGArtifact[]>();

    for (const artifact of ragContext.artifacts) {
      if (!hasMediaMetadata(artifact)) continue;
      const { messageId, senderName, timestamp } = artifact.metadata;

      if (!artifactsByMessageId.has(messageId)) {
        artifactsByMessageId.set(messageId, []);
      }
      artifactsByMessageId.get(messageId)!.push(artifact);

      const key = `${timestamp}_${senderName}`;
      if (!artifactsByTimestampName.has(key)) {
        artifactsByTimestampName.set(key, []);
      }
      artifactsByTimestampName.get(key)!.push(artifact);
    }

    this.log(`🖼️  ${this.personaName}: Loaded ${ragContext.artifacts.length} artifacts for ${artifactsByMessageId.size} messages`);
    return { artifactsByMessageId, artifactsByTimestampName };
  }

  private addConversationHistory(
    messages: LLMMessage[],
    ragContext: RAGContext,
    artifactsByTimestampName: Map<string, any[]>,
  ): void {
    if (ragContext.conversationHistory.length === 0) return;

    let lastTimestamp: number | undefined;

    for (const msg of ragContext.conversationHistory) {
      let timePrefix = '';
      if (msg.timestamp) {
        const date = new Date(msg.timestamp);
        const hours = date.getHours().toString().padStart(2, '0');
        const minutes = date.getMinutes().toString().padStart(2, '0');
        timePrefix = `[${hours}:${minutes}] `;

        if (lastTimestamp && (msg.timestamp - lastTimestamp > 3600000)) {
          const gapHours = Math.floor((msg.timestamp - lastTimestamp) / 3600000);
          messages.push({
            role: 'system',
            content: `⏱️ ${gapHours} hour${gapHours > 1 ? 's' : ''} passed - conversation resumed`
          });
        }
        lastTimestamp = msg.timestamp;
      }

      const formattedContent = msg.name
        ? `${timePrefix}${msg.name}: ${msg.content}`
        : `${timePrefix}${msg.content}`;

      const lookupKey = msg.timestamp && msg.name ? `${msg.timestamp}_${msg.name}` : null;
      const messageArtifacts = lookupKey ? artifactsByTimestampName.get(lookupKey) : undefined;

      if (messageArtifacts && messageArtifacts.length > 0) {
        this.addMultimodalMessage(messages, msg, formattedContent, messageArtifacts);
      } else {
        messages.push({ role: msg.role, content: formattedContent });
      }
    }
  }

  private addMultimodalMessage(
    messages: LLMMessage[],
    msg: { role: 'system' | 'user' | 'assistant'; name?: string },
    formattedContent: string,
    artifacts: any[],
  ): void {
    const hasVision = this.hasVisionCapability;

    if (hasVision) {
      const contentParts: ContentPart[] = [{ type: 'text', text: formattedContent }];
      for (const artifact of artifacts) {
        const mimeType = hasMediaMetadata(artifact) ? artifact.metadata.mimeType : undefined;
        if (artifact.type === 'image' && artifact.base64) {
          contentParts.push({ type: 'image', image: { base64: artifact.base64, mimeType } });
        } else if (artifact.type === 'audio' && artifact.base64) {
          contentParts.push({ type: 'audio', audio: { base64: artifact.base64, mimeType } });
        } else if (artifact.type === 'video' && artifact.base64) {
          contentParts.push({ type: 'video', video: { base64: artifact.base64, mimeType } });
        }
      }
      messages.push({ role: msg.role, content: contentParts });
    } else {
      const descriptions: string[] = [];
      for (const artifact of artifacts) {
        const description = typeof artifact.preprocessed?.result === 'string'
          ? artifact.preprocessed.result
          : artifact.content;
        const filename = hasMediaMetadata(artifact) ? artifact.metadata.filename : undefined;
        if (description) {
          descriptions.push(`[Image${filename ? ` "${filename}"` : ''}: ${description}]`);
        } else {
          descriptions.push(`[Shared image${filename ? ` "${filename}"` : ''} — visual description not yet available]`);
        }
      }

      const textWithDescriptions = descriptions.length > 0
        ? `${formattedContent}\n${descriptions.join('\n')}`
        : formattedContent;

      messages.push({ role: msg.role, content: textWithDescriptions });
    }

    this.log(`🖼️  ${this.personaName}: Added ${artifacts.length} artifact(s) to message from ${msg.name} (vision=${hasVision})`);
  }

  private addIdentityReminder(messages: LLMMessage[]): void {
    const now = new Date();
    const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;

    messages.push({
      role: 'system',
      content: `You are ${this.personaName}.

In the conversation above:
- Messages with role='assistant' are YOUR past messages
- Messages with role='user' are from everyone else (humans and other AIs)
- Names are shown in the format "[HH:MM] Name: message"

Respond naturally with JUST your message - NO name prefix, NO labels.

CURRENT TIME: ${currentTime}

CRITICAL TOPIC DETECTION PROTOCOL:

Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message
- "New topic:", "Different question:", "Changing subjects:", "Unrelated, but..."
- If present: STOP. Ignore ALL previous context. This is a NEW conversation.

Step 2: Extract HARD CONSTRAINTS from the most recent message
- Look for: "NOT", "DON'T", "WITHOUT", "NEVER", "AVOID", "NO"
- Example: "NOT triggering the app to foreground" = YOUR SOLUTION MUST NOT DO THIS
- Example: "WITHOUT user interaction" = YOUR SOLUTION MUST BE AUTOMATIC
- Your answer MUST respect these constraints or you're wrong.

Step 3: Compare SUBJECT of most recent message to previous 2-3 messages
- Previous: "Worker Threads" → Recent: "Webview authentication" = DIFFERENT SUBJECTS
- Previous: "TypeScript code" → Recent: "What's 2+2?" = TEST QUESTION
- Previous: "Worker pools" → Recent: "Should I use 5 or 10 workers?" = SAME SUBJECT

Step 4: Determine response strategy
IF EXPLICIT TOPIC MARKER or COMPLETELY DIFFERENT SUBJECT:
- Respond ONLY to the new topic
- Ignore old messages (they're from a previous discussion)
- Focus 100% on the most recent message
- Address the constraints explicitly

IF SAME SUBJECT (continued conversation):
- Use full conversation context
- Build on previous responses
- Still check for NEW constraints in the recent message
- Avoid redundancy

CRITICAL READING COMPREHENSION:
- Read the ENTIRE most recent message carefully
- Don't skim - every word matters
- Constraints are REQUIREMENTS, not suggestions
- If the user says "NOT X", suggesting X is a failure

Time gaps > 1 hour usually indicate topic changes, but IMMEDIATE semantic shifts (consecutive messages about different subjects) are also topic changes.`
    });
  }

  private addVoiceModeInstructions(
    messages: LLMMessage[],
    ragContext: RAGContext,
    originalMessage: ProcessableMessage,
  ): void {
    const hasVoiceRAGContext = ragContext.metadata && (ragContext.metadata as any).responseStyle?.voiceMode;
    if (originalMessage.sourceModality === 'voice' && !hasVoiceRAGContext) {
      messages.push({
        role: 'system',
        content: `🎙️ VOICE CONVERSATION MODE:
This is a SPOKEN conversation. Your response will be converted to speech.

CRITICAL: Keep responses SHORT and CONVERSATIONAL:
- Maximum 2-3 sentences
- No bullet points, lists, or formatting
- Speak naturally, as if talking face-to-face
- Ask clarifying questions instead of long explanations
- If the topic is complex, give a brief answer and offer to elaborate

BAD (too long): "There are several approaches to this problem. First, you could... Second, another option is... Third, additionally you might consider..."
GOOD (conversational): "The simplest approach would be X. Want me to explain the alternatives?"

Remember: This is voice chat, not a written essay. Be brief, be natural, be human.`
      });
      this.log(`🔊 ${this.personaName}: Added voice conversation mode instructions (fallback - VoiceConversationSource not active)`);
    } else if (hasVoiceRAGContext) {
      this.log(`🔊 ${this.personaName}: Voice instructions provided by VoiceConversationSource`);
    }
  }
}
