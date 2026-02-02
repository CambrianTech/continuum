/**
 * AI Should Respond Fast Server Command
 *
 * Fast, deterministic "should respond" detection using bag-of-words scoring
 * No LLM calls - pure algorithmic approach
 */

import { ShouldRespondFastCommand } from '../shared/ShouldRespondFastCommand';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  ShouldRespondFastParams,
  ShouldRespondFastResult,
  PersonaResponseConfig,
  ResponseScoringWeights
} from '../shared/ShouldRespondFastTypes';
import { DEFAULT_SCORING_WEIGHTS, DEFAULT_RESPONSE_THRESHOLD } from '../shared/ShouldRespondFastTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListParams, DataListResult } from '../../../data/list/shared/DataListTypes';
import type { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';

import { DataList } from '../../../data/list/shared/DataListTypes';
export class ShouldRespondFastServerCommand extends ShouldRespondFastCommand {
  // Cache of recent message timestamps per persona per room
  private lastMessageTimes: Map<string, number> = new Map();

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/should-respond-fast', context, subpath, commander);
  }

  async execute(params: ShouldRespondFastParams): Promise<ShouldRespondFastResult> {
    try {
      // Validate required params - AIs may call this with incomplete data
      if (!params.personaId) {
        return this.buildResult(params, false, 0, {
          reasoning: 'Missing required parameter: personaId'
        });
      }

      if (!params.messageText) {
        return this.buildResult(params, false, 0, {
          reasoning: 'Missing required parameter: messageText'
        });
      }

      // Default contextId to a placeholder if not provided (allows tool to work)
      const contextId = params.contextId ?? 'default-context';

      console.log(`🎯 ShouldRespondFast: Evaluating for persona ${params.personaId.slice(0, 8)} in context ${contextId.slice(0, 8)}`);

      // Build config (merge defaults with overrides)
      const config = await this.buildConfig(params);

      // Check cooldown
      const cooldownKey = `${params.personaId}:${contextId}`;
      const lastMessageTime = this.lastMessageTimes.get(cooldownKey) ?? 0;
      const now = Date.now();
      const timeSinceLastMessage = (now - lastMessageTime) / 1000;

      if (timeSinceLastMessage < config.cooldownSeconds) {
        console.log(`⏰ ShouldRespondFast: Cooldown active (${timeSinceLastMessage.toFixed(0)}s < ${config.cooldownSeconds}s)`);
        return this.buildResult(params, false, 0, {
          reasoning: `Cooldown active - last message ${timeSinceLastMessage.toFixed(0)}s ago`
        });
      }

      // Calculate score
      const scoreResult = await this.calculateScore(params, config);

      // Determine if should respond
      const shouldRespond = scoreResult.score >= config.responseThreshold ||
        (config.alwaysRespondToMentions && scoreResult.signals.wasMentioned);

      // Update cooldown if responding
      if (shouldRespond) {
        this.lastMessageTimes.set(cooldownKey, now);
      }

      console.log(`${shouldRespond ? '✅' : '❌'} ShouldRespondFast: Score ${scoreResult.score} ${shouldRespond ? '>=' : '<'} threshold ${config.responseThreshold}`);

      return this.buildResult(params, shouldRespond, scoreResult.score, {
        scoreBreakdown: scoreResult.scoreBreakdown,
        signals: scoreResult.signals,
        reasoning: shouldRespond
          ? `Score ${scoreResult.score} exceeds threshold ${config.responseThreshold}`
          : `Score ${scoreResult.score} below threshold ${config.responseThreshold}`
      });

    } catch (error) {
      console.error('❌ ShouldRespondFast: Command failed:', error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: error instanceof Error ? error.message : String(error),
        shouldRespond: false,
        score: 0,
        scoreBreakdown: {
          directMention: 0,
          domainKeywords: 0,
          conversationContext: 0,
          isQuestion: 0,
          unansweredQuestion: 0,
          roomActivity: 0,
          humanMessage: 0
        },
        signals: {
          wasMentioned: false,
          matchedKeywords: [],
          isQuestion: false,
          recentlyActive: false,
          isHumanMessage: false
        },
        reasoning: 'Error occurred during evaluation'
      };
    }
  }

  /**
   * Build persona config (defaults + overrides + DB lookup)
   */
  private async buildConfig(params: ShouldRespondFastParams): Promise<PersonaResponseConfig> {
    // TODO: Load from database when PersonaResponseConfig entity exists
    // For now, use defaults with overrides

    const weights: ResponseScoringWeights = {
      ...DEFAULT_SCORING_WEIGHTS,
      ...(params.config?.weights ?? {})
    };

    return {
      personaId: params.personaId,
      personaName: params.config?.personaName ?? 'Persona',
      domainKeywords: params.config?.domainKeywords ?? [],
      weights,
      responseThreshold: params.config?.responseThreshold ?? DEFAULT_RESPONSE_THRESHOLD,
      alwaysRespondToMentions: params.config?.alwaysRespondToMentions ?? true,
      cooldownSeconds: params.config?.cooldownSeconds ?? 60
    };
  }

  /**
   * Calculate bag-of-words score
   */
  private async calculateScore(
    params: ShouldRespondFastParams,
    config: PersonaResponseConfig
  ): Promise<{
    score: number;
    scoreBreakdown: ShouldRespondFastResult['scoreBreakdown'];
    signals: ShouldRespondFastResult['signals'];
  }> {
    const messageText = params.messageText.toLowerCase();
    const scoreBreakdown = {
      directMention: 0,
      domainKeywords: 0,
      conversationContext: 0,
      isQuestion: 0,
      unansweredQuestion: 0,
      roomActivity: 0,
      humanMessage: 0
    };

    const signals = {
      wasMentioned: false,
      matchedKeywords: [] as string[],
      isQuestion: false,
      recentlyActive: false,
      isHumanMessage: false
    };

    // 1. Direct mention detection
    const personaNameLower = config.personaName.toLowerCase();
    if (messageText.includes(`@${personaNameLower}`) || messageText.includes(personaNameLower)) {
      scoreBreakdown.directMention = config.weights.directMention;
      signals.wasMentioned = true;
      console.log(`🎯 Detected direct mention: ${config.personaName}`);
    }

    // 2. Domain keyword matching
    for (const keyword of config.domainKeywords) {
      if (messageText.includes(keyword.toLowerCase())) {
        scoreBreakdown.domainKeywords += config.weights.domainKeyword;
        signals.matchedKeywords.push(keyword);
      }
    }
    if (signals.matchedKeywords.length > 0) {
      console.log(`🔑 Matched keywords: ${signals.matchedKeywords.join(', ')}`);
    }

    // 3. Conversation context - was persona recently active?
    const recentlyActive = await this.wasRecentlyActive(params.personaId, params.contextId);
    if (recentlyActive) {
      scoreBreakdown.conversationContext = config.weights.conversationContext;
      signals.recentlyActive = true;
      console.log(`💬 Persona recently active in conversation`);
    }

    // 4. Question detection
    const isQuestion = messageText.includes('?') ||
      /\b(how|what|why|when|where|who|can|could|would|should|is|are|do|does)\b/.test(messageText);
    if (isQuestion) {
      scoreBreakdown.isQuestion = config.weights.isQuestion;
      signals.isQuestion = true;
      console.log(`❓ Message is a question`);

      // 4a. Check if question is unanswered (someone should respond)
      const hasRecentResponse = await this.hasRecentResponse(params.messageId, params.contextId);
      if (!hasRecentResponse) {
        scoreBreakdown.unansweredQuestion = config.weights.unansweredQuestion;
        console.log(`❗ Unanswered question - needs attention`);
      }
    }

    // 5. Room activity - recent messages in last 5 minutes (deprecated, usually 0)
    const roomActivity = await this.getRoomActivity(params.contextId);
    if (roomActivity > 3 && config.weights.roomActivity > 0) {
      scoreBreakdown.roomActivity = config.weights.roomActivity;
      console.log(`📈 High room activity: ${roomActivity} messages in 5min`);
    }

    // 6. Human message detection - humans deserve responses!
    // senderType passed from caller OR detected from senderId lookup
    const isHumanMessage = params.senderType === 'human' ||
      (!params.senderType && await this.isHumanSender(params.senderId));
    if (isHumanMessage) {
      scoreBreakdown.humanMessage = config.weights.humanMessage;
      signals.isHumanMessage = true;
      console.log(`👤 Message is from HUMAN - priority boost!`);
    }

    // Calculate total score
    const score =
      scoreBreakdown.directMention +
      scoreBreakdown.domainKeywords +
      scoreBreakdown.conversationContext +
      scoreBreakdown.isQuestion +
      scoreBreakdown.unansweredQuestion +
      scoreBreakdown.roomActivity +
      scoreBreakdown.humanMessage;

    return { score, scoreBreakdown, signals };
  }

  /**
   * Check if persona was recently active in conversation
   */
  private async wasRecentlyActive(personaId: string, contextId: string): Promise<boolean> {
    try {
      // Check if persona sent message in last 10 minutes
      const tenMinutesAgo = new Date(Date.now() - 10 * 60 * 1000);

      const result = await DataList.execute<ChatMessageEntity>({
        collection: 'chat_messages',
        filter: {
          roomId: contextId,
          senderId: personaId
        },
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 1
      });

      if (result.success && result.items && result.items.length > 0) {
        const lastMessage = result.items[0];
        const messageTime = new Date(lastMessage.timestamp);
        return messageTime > tenMinutesAgo;
      }

      return false;
    } catch (error) {
      console.warn('⚠️ Failed to check recent activity:', error);
      return false;
    }
  }

  /**
   * Check if question has received a response in last 2 minutes
   */
  private async hasRecentResponse(messageId: string, contextId: string): Promise<boolean> {
    try {
      const twoMinutesAgo = new Date(Date.now() - 2 * 60 * 1000);

      const result = await DataList.execute<ChatMessageEntity>({
        collection: 'chat_messages',
        filter: { roomId: contextId },
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 20 // Check last 20 messages
      });

      if (result.success && result.items) {
        // Find the original message
        const originalIndex = result.items.findIndex((msg: ChatMessageEntity) => msg.id === messageId);
        if (originalIndex === -1) return false;

        // Check if any subsequent messages exist after the original
        const subsequentMessages = result.items.slice(0, originalIndex);
        if (subsequentMessages.length > 0) {
          const lastReply = subsequentMessages[0];
          const replyTime = new Date(lastReply.timestamp);
          return replyTime > twoMinutesAgo;
        }
      }

      return false;
    } catch (error) {
      console.warn('⚠️ Failed to check for recent responses:', error);
      return false; // Assume unanswered if check fails
    }
  }

  /**
   * Get room activity count (messages in last 5 minutes)
   */
  private async getRoomActivity(contextId: string): Promise<number> {
    try {
      const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000);

      const result = await DataList.execute<ChatMessageEntity>({
        collection: 'chat_messages',
        filter: { roomId: contextId },
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 50 // Check last 50 messages
      });

      if (result.success && result.items) {
        const recentMessages = result.items.filter((msg: ChatMessageEntity) => {
          const messageTime = new Date(msg.timestamp);
          return messageTime > fiveMinutesAgo;
        });
        return recentMessages.length;
      }

      return 0;
    } catch (error) {
      console.warn('⚠️ Failed to check room activity:', error);
      return 0;
    }
  }

  /**
   * Check if sender is a human (not AI persona, agent, or system)
   */
  private async isHumanSender(senderId: string): Promise<boolean> {
    if (!senderId) return false;

    try {
      const result = await DataList.execute<any>({
        collection: 'users',
        filter: { id: senderId },
        limit: 1
      });

      if (result.success && result.items && result.items.length > 0) {
        const user = result.items[0];
        const userType = user.userType || user.type || '';
        // Human types: 'human', 'owner', 'admin', 'user' (or empty - default is human)
        const isHuman = ['human', 'owner', 'admin', 'user', ''].includes(userType.toLowerCase());
        return isHuman;
      }

      // If can't find user, assume not human (safer for AI response filtering)
      return false;
    } catch (error) {
      console.warn('⚠️ Failed to check sender type:', error);
      return false;
    }
  }

  /**
   * Build result object
   */
  private buildResult(
    params: ShouldRespondFastParams,
    shouldRespond: boolean,
    score: number,
    extra: {
      scoreBreakdown?: ShouldRespondFastResult['scoreBreakdown'];
      signals?: ShouldRespondFastResult['signals'];
      reasoning: string;
    }
  ): ShouldRespondFastResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      shouldRespond,
      score,
      scoreBreakdown: extra.scoreBreakdown ?? {
        directMention: 0,
        domainKeywords: 0,
        conversationContext: 0,
        isQuestion: 0,
        unansweredQuestion: 0,
        roomActivity: 0,
        humanMessage: 0
      },
      signals: extra.signals ?? {
        wasMentioned: false,
        matchedKeywords: [],
        isQuestion: false,
        recentlyActive: false,
        isHumanMessage: false
      },
      reasoning: extra.reasoning
    };
  }
}
