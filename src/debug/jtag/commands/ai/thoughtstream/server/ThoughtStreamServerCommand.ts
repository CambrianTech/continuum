/**
 * AI ThoughtStream Server Command
 *
 * Time-travel debugging for AI decisions:
 * - Query coordinator for active streams
 * - Recreate EXACT RAG context each AI saw
 * - Show full conversation history, identity, system prompt
 * - "RDP into any point in time and see what they saw"
 */

import type { ThoughtStreamParams, ThoughtStreamResult, ThoughtStreamDecision, Thought } from '../shared/ThoughtStreamTypes';
import { ThoughtStreamCommand } from '../shared/ThoughtStreamCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { getThoughtStreamCoordinator } from '../../../../system/conversation/server/ThoughtStreamCoordinator';
import { RAGBuilderFactory } from '../../../../system/rag/shared/RAGBuilder';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';
import type { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';

export class ThoughtStreamServerCommand extends ThoughtStreamCommand {
  private personaCache: Map<string, string | null> = new Map(); // name -> id

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/thoughtstream', context, subpath, commander);
  }

  async execute(params: ThoughtStreamParams): Promise<ThoughtStreamResult> {
    try {
      // Two modes:
      // 1. LIVE MODE: Query coordinator for active streams (last 60 seconds)
      // 2. HISTORICAL MODE: Reconstruct from AI decision logs + data queries

      // Try live mode first
      const coordinator = getThoughtStreamCoordinator();
      const activeStreams = coordinator.getStreams();

      // If we have specific messageId or time range, use historical reconstruction
      const useHistoricalMode = params.messageId || params.since || activeStreams.size === 0;

      if (useHistoricalMode) {
        return await this.executeHistorical(params);
      }

      // LIVE MODE: Get active streams from coordinator
      let streams = Array.from(activeStreams.values());

      // Filter by messageId if specified
      if (params.messageId) {
        const stream = coordinator.getStream(params.messageId);
        streams = stream ? [stream] : [];
      }

      // Filter by time range
      if (params.since) {
        const sinceMs = this.parseTimeRange(params.since);
        const cutoff = Date.now() - sinceMs;
        streams = streams.filter(s => s.startTime >= cutoff);
      }

      // Limit results
      const limit = params.limit ?? 10;
      streams = streams.slice(-limit);

      // Build detailed decision records
      const decisions: ThoughtStreamDecision[] = [];

      for (const stream of streams) {
        // Get the message entity to show what message this was about
        let messageEntity: ChatMessageEntity | undefined;
        let messageSender = 'Unknown';
        let messageContent = '';

        try {
          // Query data daemon for the message
          const msg = await ORM.read<ChatMessageEntity>(
            COLLECTIONS.CHAT_MESSAGES,
            stream.messageId
          );

          if (msg) {
            messageSender = msg.senderName || 'Unknown';
            messageContent = msg.content?.text ?? '';
          }
        } catch (error) {
          console.warn(`⚠️ Could not load message ${stream.messageId}:`, error);
        }

        // Convert thoughts to detailed records with RAG context recreation
        const thoughts: Thought[] = [];

        for (const thought of stream.thoughts) {
          // Recreate the EXACT RAG context this AI saw
          let ragContext;

          if (params.showRagContext) {
            try {
              // Call the SAME RAG builder that PersonaUser uses
              const ragBuilder = RAGBuilderFactory.getBuilder('chat');
              const fullContext = await ragBuilder.buildContext(
                stream.contextId,
                thought.personaId,
                {
                  maxMessages: 20,
                  maxMemories: 0,
                  includeArtifacts: false,
                  includeMemories: false
                }
              );

              ragContext = {
                totalMessages: fullContext.conversationHistory?.length ?? 0,
                filteredMessages: fullContext.conversationHistory?.length ?? 0,
                conversationHistory: fullContext.conversationHistory?.map(msg => ({
                  name: msg.name ?? msg.role,
                  content: msg.content,
                  timestamp: msg.timestamp
                }))
              };
            } catch (error) {
              console.warn(`⚠️ Could not recreate RAG context for ${thought.personaId}:`, error);
            }
          }

          thoughts.push({
            personaId: thought.personaId,
            personaName: await this.getPersonaName(thought.personaId, params),
            type: thought.type as 'claiming' | 'deferring' | 'observing',
            confidence: thought.confidence,
            reasoning: thought.reasoning,
            timestamp: thought.timestamp,
            ragContext
          });
        }

        // Build decision record
        const decision: ThoughtStreamDecision = {
          messageId: stream.messageId,
          streamId: `stream-${stream.messageId.slice(0, 8)}`,
          messageContent,
          messageSender,
          messageTimestamp: new Date(stream.startTime),
          thoughts,
          evaluationDuration: stream.decision
            ? (typeof stream.decision.timestamp === 'number' ? stream.decision.timestamp : stream.decision.timestamp.getTime()) - stream.startTime
            : Date.now() - stream.startTime,
          decision: {
            granted: stream.decision?.granted ?? [],
            denied: stream.decision?.denied ?? [],
            reasoning: stream.decision ? this.formatDecisionReasoning(stream) : 'Pending...',
            decisionTime: stream.decision
              ? (typeof stream.decision.timestamp === 'number' ? stream.decision.timestamp : stream.decision.timestamp.getTime())
              : 0,
            waitDuration: stream.decision
              ? (typeof stream.decision.timestamp === 'number' ? stream.decision.timestamp : stream.decision.timestamp.getTime()) - stream.startTime
              : 0
          },
          outcomes: [] // TODO: Track outcomes from PersonaUser posts
        };

        decisions.push(decision);
      }

      // Calculate summary statistics
      const summary = {
        totalStreams: decisions.length,
        totalThoughts: decisions.reduce((sum, d) => sum + d.thoughts.length, 0),
        avgThoughtsPerStream: decisions.length > 0
          ? decisions.reduce((sum, d) => sum + d.thoughts.length, 0) / decisions.length
          : 0,
        avgEvaluationTime: decisions.length > 0
          ? decisions.reduce((sum, d) => sum + d.evaluationDuration, 0) / decisions.length
          : 0,
        avgDecisionTime: decisions.filter(d => d.decision.waitDuration > 0).length > 0
          ? decisions.reduce((sum, d) => sum + d.decision.waitDuration, 0) /
            decisions.filter(d => d.decision.waitDuration > 0).length
          : 0,
        posted: 0, // TODO: Track from outcomes
        timeouts: 0,
        errors: 0,
        redundant: 0,
        silent: 0,
        avgConfidenceThreshold: 0.7,
        coordinationEfficiency: decisions.filter(d => d.decision.granted.length === 1).length /
          Math.max(1, decisions.filter(d => d.decision.granted.length > 0).length) * 100
      };

      // Detect issues
      const slowEvaluations: Array<{ personaName: string; duration: number; messageId: string }> = [];
      const coordinationDeadlocks: Array<{ messageId: string; grantedPersona: string; outcome: string; reason: string }> = [];
      const lowConfidenceDecisions: Array<{ personaName: string; confidence: number; messageId: string }> = [];

      for (const decision of decisions) {
        // Detect slow evaluations (>5 seconds)
        if (decision.evaluationDuration > 5000) {
          for (const thought of decision.thoughts) {
            slowEvaluations.push({
              personaName: thought.personaName,
              duration: decision.evaluationDuration,
              messageId: decision.messageId
            });
          }
        }

        // Detect low confidence
        for (const thought of decision.thoughts) {
          if (thought.confidence && thought.confidence < 0.5) {
            lowConfidenceDecisions.push({
              personaName: thought.personaName,
              confidence: thought.confidence,
              messageId: decision.messageId
            });
          }
        }
      }

      const issues = {
        slowEvaluations,
        coordinationDeadlocks,
        lowConfidenceDecisions
      };

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        streams: decisions,
        summary,
        issues
      };

    } catch (error) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: error instanceof Error ? error.message : String(error),
        streams: [],
        summary: {
          totalStreams: 0,
          totalThoughts: 0,
          avgThoughtsPerStream: 0,
          avgEvaluationTime: 0,
          avgDecisionTime: 0,
          posted: 0,
          timeouts: 0,
          errors: 0,
          redundant: 0,
          silent: 0,
          avgConfidenceThreshold: 0,
          coordinationEfficiency: 0
        }
      };
    }
  }

  /**
   * Historical mode: Reconstruct ThoughtStream from AI decision logs
   * This allows time-travel debugging of past decisions
   */
  private async executeHistorical(params: ThoughtStreamParams): Promise<ThoughtStreamResult> {
    const AIDecisionLogger = (await import('../../../../system/ai/server/AIDecisionLogger')).AIDecisionLogger;
    const logPath = AIDecisionLogger.getLogPath();

    if (!logPath) {
      throw new Error('AI decision log path not available');
    }

    const fs = await import('fs');
    const path = await import('path');

    // Read AI decision log (logPath is already an absolute path)
    const fullLogPath = path.isAbsolute(logPath) ? logPath : path.join(process.cwd(), logPath);
    if (!fs.existsSync(fullLogPath)) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: `No AI decision logs found at: ${fullLogPath}`,
        streams: [],
        summary: this.getEmptySummary()
      };
    }

    const logContent = fs.readFileSync(fullLogPath, 'utf-8');
    const logLines = logContent.split('\n').filter(l => l.trim());

    // Parse log entries
    interface ParsedEntry {
      timestamp: Date;
      personaName: string;
      action: string;
      messageId?: string;
      messageContent?: string;
      roomId?: string;
      confidence?: number;
      reason?: string;
      ragContext?: any;
    }

    const entries: ParsedEntry[] = [];

    for (const line of logLines) {
      const match = line.match(/^\[(.*?)\] (.*?) → (RESPOND|SILENT|POSTED|ERROR|EVALUATING)/);
      if (!match) continue;

      const timestamp = new Date(match[1]);
      const personaName = match[2];
      const action = match[3];

      // Extract room ID
      const roomMatch = line.match(/Room: ([a-f0-9-]+)/);
      const roomId = roomMatch ? roomMatch[1] : undefined;

      // Extract message content
      const messageMatch = line.match(/Message: "([^"]+)"/);
      const messageContent = messageMatch ? messageMatch[1] : undefined;

      // Extract confidence
      const confMatch = line.match(/Confidence: ([\d.]+)/);
      const confidence = confMatch ? parseFloat(confMatch[1]) : undefined;

      // Extract reason
      const reasonMatch = line.match(/Reason: ([^|]+)/);
      const reason = reasonMatch ? reasonMatch[1].trim() : undefined;

      // Extract RAG context
      const ragMatch = line.match(/📊 RAG Context: (\d+)\/(\d+) messages/);
      const ragContext = ragMatch ? {
        filtered: parseInt(ragMatch[1]),
        total: parseInt(ragMatch[2])
      } : undefined;

      entries.push({
        timestamp,
        personaName,
        action,
        roomId,
        messageContent,
        confidence,
        reason,
        ragContext
      });
    }

    // Filter by time range
    let filteredEntries = entries;
    if (params.since) {
      const sinceMs = this.parseTimeRange(params.since);
      const cutoff = Date.now() - sinceMs;
      filteredEntries = entries.filter(e => e.timestamp.getTime() >= cutoff);
    }

    // Filter by room ID
    if (params.roomId) {
      filteredEntries = filteredEntries.filter(e => e.roomId === params.roomId);
    }

    // Group by message (using message content as key since messageId might not be in logs)
    const messageGroups = new Map<string, ParsedEntry[]>();
    for (const entry of filteredEntries) {
      const key = entry.messageContent || entry.timestamp.toISOString();
      if (!messageGroups.has(key)) {
        messageGroups.set(key, []);
      }
      messageGroups.get(key)!.push(entry);
    }

    // Build decision records for each message
    const decisions: ThoughtStreamDecision[] = [];

    for (const [messageKey, groupEntries] of Array.from(messageGroups.entries())) {
      const firstEntry = groupEntries[0];

      // Get thoughts (RESPOND decisions)
      const thoughtEntries = groupEntries.filter(e => e.action === 'RESPOND');
      const thoughts: Thought[] = [];

      for (const entry of thoughtEntries) {
        let ragContext;

        if (params.showRagContext && entry.roomId) {
          try {
            // Recreate RAG context using the SAME builder
            const ragBuilder = RAGBuilderFactory.getBuilder('chat');

            // Get persona ID from name
            const personaId = await this.getPersonaIdByName(entry.personaName, params);

            if (personaId) {
              const fullContext = await ragBuilder.buildContext(
                entry.roomId,
                personaId,
                {
                  maxMessages: 20,
                  maxMemories: 0,
                  includeArtifacts: false,
                  includeMemories: false
                }
              );

              ragContext = {
                totalMessages: fullContext.conversationHistory?.length ?? 0,
                filteredMessages: fullContext.conversationHistory?.length ?? 0,
                conversationHistory: fullContext.conversationHistory?.map(msg => ({
                  name: msg.name ?? msg.role,
                  content: msg.content,
                  timestamp: msg.timestamp
                }))
              };
            }
          } catch (error) {
            console.warn(`⚠️ Could not recreate RAG for ${entry.personaName}:`, error);
          }
        }

        thoughts.push({
          personaId: 'unknown', // Historical mode doesn't have personaId in logs
          personaName: entry.personaName,
          type: 'claiming',
          confidence: entry.confidence,
          reasoning: entry.reason,
          timestamp: entry.timestamp,
          ragContext
        });
      }

      // Get outcomes (POSTED/ERROR decisions)
      const outcomeEntries = groupEntries.filter(e => ['POSTED', 'ERROR', 'SILENT'].includes(e.action));
      const outcomes = outcomeEntries.map(e => ({
        personaId: 'unknown',
        personaName: e.personaName,
        action: e.action as 'POSTED' | 'ERROR' | 'SILENT',
        responseText: e.action === 'POSTED' ? e.messageContent : undefined,
        error: e.action === 'ERROR' ? e.reason : undefined,
        responseTime: e.timestamp.getTime()
      }));

      const decision: ThoughtStreamDecision = {
        messageId: `hist-${firstEntry.timestamp.getTime()}`,
        streamId: `stream-hist-${decisions.length + 1}`,
        messageContent: firstEntry.messageContent || 'Unknown',
        messageSender: 'Unknown', // Historical mode doesn't track sender
        messageTimestamp: firstEntry.timestamp,
        thoughts,
        evaluationDuration: thoughtEntries.length > 0
          ? Math.max(...groupEntries.map(e => e.timestamp.getTime())) -
            Math.min(...groupEntries.map(e => e.timestamp.getTime()))
          : 0,
        decision: {
          granted: outcomes.filter(o => o.action === 'POSTED').map(o => o.personaId),
          denied: outcomes.filter(o => o.action === 'SILENT').map(o => o.personaId),
          reasoning: `Historical: ${outcomes.length} outcomes`,
          decisionTime: firstEntry.timestamp.getTime(),
          waitDuration: 0
        },
        outcomes
      };

      decisions.push(decision);
    }

    // Apply limit
    const limit = params.limit ?? 10;
    const limitedDecisions = decisions.slice(-limit);

    // Calculate summary
    const summary = this.calculateSummary(limitedDecisions);

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      streams: limitedDecisions,
      summary,
      issues: {
        slowEvaluations: [],
        coordinationDeadlocks: [],
        lowConfidenceDecisions: limitedDecisions
          .flatMap(d => d.thoughts)
          .filter(t => t.confidence && t.confidence < 0.5)
          .map(t => ({
            personaName: t.personaName,
            confidence: t.confidence!,
            messageId: 'historical'
          }))
      }
    };
  }

  private async getPersonaIdByName(name: string, params: ThoughtStreamParams): Promise<string | null> {
    // Check cache first
    if (this.personaCache.has(name)) {
      return this.personaCache.get(name) ?? null;
    }

    try {
      // Query user collection by displayName field
      const result = await ORM.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: { displayName: name },
        limit: 1
      });

      if (result.success && result.data && result.data.length > 0) {
        const personaId = result.data[0].id;
        this.personaCache.set(name, personaId);
        return personaId;
      }

      // Cache null result to avoid repeated queries
      this.personaCache.set(name, null);
      return null;
    } catch (error) {
      console.warn(`⚠️ Could not find persona ID for name: ${name}`, error);
      this.personaCache.set(name, null);
      return null;
    }
  }

  private calculateSummary(decisions: ThoughtStreamDecision[]) {
    return {
      totalStreams: decisions.length,
      totalThoughts: decisions.reduce((sum, d) => sum + d.thoughts.length, 0),
      avgThoughtsPerStream: decisions.length > 0
        ? decisions.reduce((sum, d) => sum + d.thoughts.length, 0) / decisions.length
        : 0,
      avgEvaluationTime: decisions.length > 0
        ? decisions.reduce((sum, d) => sum + d.evaluationDuration, 0) / decisions.length
        : 0,
      avgDecisionTime: 0,
      posted: decisions.reduce((sum, d) => sum + d.outcomes.filter(o => o.action === 'POSTED').length, 0),
      timeouts: 0,
      errors: decisions.reduce((sum, d) => sum + d.outcomes.filter(o => o.action === 'ERROR').length, 0),
      redundant: 0,
      silent: decisions.reduce((sum, d) => sum + d.outcomes.filter(o => o.action === 'SILENT').length, 0),
      avgConfidenceThreshold: 0.7,
      coordinationEfficiency: 0
    };
  }

  private getEmptySummary() {
    return {
      totalStreams: 0,
      totalThoughts: 0,
      avgThoughtsPerStream: 0,
      avgEvaluationTime: 0,
      avgDecisionTime: 0,
      posted: 0,
      timeouts: 0,
      errors: 0,
      redundant: 0,
      silent: 0,
      avgConfidenceThreshold: 0,
      coordinationEfficiency: 0
    };
  }

  private parseTimeRange(timeStr: string): number {
    const match = timeStr.match(/^(\d+)(m|h|d)$/);
    if (!match) return 5 * 60 * 1000; // Default 5 minutes

    const value = parseInt(match[1]);
    const unit = match[2];

    switch (unit) {
      case 'm': return value * 60 * 1000;
      case 'h': return value * 60 * 60 * 1000;
      case 'd': return value * 24 * 60 * 60 * 1000;
      default: return 5 * 60 * 1000;
    }
  }

  private formatDecisionReasoning(stream: any): string {
    if (!stream.decision) return '';

    const granted = stream.decision.granted.length;
    const denied = stream.decision.denied.length;

    return `Granted ${granted} AI${granted !== 1 ? 's' : ''}, denied ${denied}`;
  }

  private async getPersonaName(personaId: string, params: ThoughtStreamParams): Promise<string> {
    try {
      const user = await ORM.read<UserEntity>(
        COLLECTIONS.USERS,
        personaId
      );

      if (user) {
        return user.displayName || personaId.slice(0, 8);
      }
      return personaId.slice(0, 8);
    } catch {
      return personaId.slice(0, 8);
    }
  }
}
