/**
 * AI Report Server Command
 *
 * Parse AI decision logs and generate actionable insights
 */

import * as fs from 'fs';
import * as path from 'path';
import { AIReportCommand } from '../shared/AIReportCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIReportParams, AIReportResult } from '../shared/AIReportTypes';
import { AIDecisionService, type AIDecisionContext } from '../../../../system/ai/server/AIDecisionService';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';
import type { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';

interface ParsedDecision {
  timestamp: string;
  persona: string;
  decision: 'RESPOND' | 'SILENT' | 'POSTED' | 'REDUNDANCY-CHECK' | 'ERROR';
  confidence?: number;
  model?: string;
  reason: string;
  message?: string;
  sender?: string;
  roomId: string;
  ragContext?: {
    totalMessages: number;
    filteredMessages: number;
    timeWindowMinutes?: number;
  };
  conversationHistory?: Array<{
    name: string;
    content: string;
    timestamp?: number;
  }>;
}

interface PersonaStats {
  decisions: number;
  responses: number;
  silences: number;
  avgConfidence: number;
  confidences?: number[];  // Optional because we delete it after calculating avgConfidence
}

interface RoomStats {
  decisions: number;
  responses: number;
  silences: number;
}

export class AIReportServerCommand extends AIReportCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/report', context, subpath, commander);
  }

  async execute(params: AIReportParams): Promise<AIReportResult> {
    try {
      // Get AI decision log path
      const logPath = path.join(
        process.cwd(),
        '.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log'
      );

      if (!fs.existsSync(logPath)) {
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: false,
          error: 'AI decision log not found',
          summary: this.getEmptySummary()
        };
      }

      // Parse log file
      const decisions = this.parseLogFile(logPath);

      // Handle persona filtering
      // Note: AI decision logs only contain persona display names, not IDs
      // For personaId/personaUniqueId filtering, we would need to:
      // 1. Load user records from database
      // 2. Map persona IDs/uniqueIds to display names
      // 3. Filter logs by mapped display names
      // For now, these parameters are accepted but not yet implemented
      const personaNameFilter: string | undefined = params.personaName;

      if (params.personaId ?? params.personaUniqueId) {
        // TODO: Implement personaId/uniqueId filtering by loading user data
        // and mapping to persona display names
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: false,
          error: 'Filtering by personaId/personaUniqueId not yet implemented. Use personaName instead.',
          summary: this.getEmptySummary()
        };
      }

      // Apply filters
      let filteredDecisions = decisions;

      if (params.roomId) {
        filteredDecisions = filteredDecisions.filter(d => d.roomId.startsWith(params.roomId!));
      }

      if (personaNameFilter) {
        filteredDecisions = filteredDecisions.filter(d => d.persona === personaNameFilter);
      }

      if (filteredDecisions.length === 0) {
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: true,
          summary: this.getEmptySummary()
        };
      }

      // Generate summary statistics
      const summary = this.generateSummary(filteredDecisions);

      // Generate room analysis (if filtering by room)
      const roomAnalysis = params.roomId
        ? this.generateRoomAnalysis(filteredDecisions, params.roomId)
        : undefined;

      // Generate persona analysis (if filtering by persona)
      const personaAnalysis = params.personaName
        ? this.generatePersonaAnalysis(filteredDecisions, params.personaName)
        : undefined;

      // Generate context analysis (if requested)
      const contextAnalysis = params.includeContextAnalysis
        ? this.generateContextAnalysis(filteredDecisions)
        : undefined;

      // Generate timeline (if requested)
      const timeline = params.includeTimeline
        ? this.generateTimeline(filteredDecisions)
        : undefined;

      // Detect issues (if requested)
      const issues = params.detectIssues
        ? this.detectIssues(filteredDecisions)
        : undefined;

      // Recreate specific decision (if requested)
      const recreatedDecision = params.recreateDecision
        ? await this.recreateDecision(decisions, params.recreateDecision)
        : undefined;

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        summary,
        roomAnalysis,
        personaAnalysis,
        contextAnalysis,
        timeline,
        issues,
        recreatedDecision
      };

    } catch (error) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: error instanceof Error ? error.message : String(error),
        summary: this.getEmptySummary()
      };
    }
  }

  private parseLogFile(logPath: string): ParsedDecision[] {
    const content = fs.readFileSync(logPath, 'utf-8');
    const lines = content.split('\n');
    const decisions: ParsedDecision[] = [];

    let currentDecision: Partial<ParsedDecision> | null = null;
    let inConversationHistory = false;
    let conversationHistory: Array<{ name: string; content: string; timestamp?: number }> = [];

    for (const line of lines) {
      if (line.trim().length === 0 || line.includes('===') || line.includes('Session Started')) {
        continue;
      }

      // Main decision line: [timestamp] Persona → DECISION | ...
      const mainMatch = line.match(/^\[([^\]]+)\]\s+([^→]+)\s+→\s+(RESPOND|SILENT|POSTED|REDUNDANCY-CHECK|ERROR)\s+\|(.*)/);
      if (mainMatch) {
        // Save previous decision if exists
        if (currentDecision && currentDecision.timestamp) {
          if (conversationHistory.length > 0) {
            currentDecision.conversationHistory = conversationHistory;
          }
          decisions.push(currentDecision as ParsedDecision);
        }

        // Start new decision
        const [, timestamp, persona, decision, details] = mainMatch;
        currentDecision = {
          timestamp,
          persona: persona.trim(),
          decision: decision as ParsedDecision['decision'],
          roomId: '',
          reason: ''
        };
        conversationHistory = [];
        inConversationHistory = false;

        // Parse details
        const confidenceMatch = details.match(/Confidence:\s+([\d.]+)/);
        if (confidenceMatch) {
          currentDecision.confidence = parseFloat(confidenceMatch[1]);
        }

        const modelMatch = details.match(/Model:\s+([^|]+)/);
        if (modelMatch) {
          currentDecision.model = modelMatch[1].trim();
        }

        const roomMatch = details.match(/Room:\s+([^|]+)/);
        if (roomMatch) {
          currentDecision.roomId = roomMatch[1].trim();
        }

        const reasonMatch = details.match(/Reason:\s+([^|]+)/);
        if (reasonMatch) {
          currentDecision.reason = reasonMatch[1].trim();
        }

        const messageMatch = details.match(/Message:\s+"([^"]+)"/);
        if (messageMatch) {
          currentDecision.message = messageMatch[1];
        }

        const senderMatch = details.match(/Sender:\s+([^|]+)/);
        if (senderMatch) {
          currentDecision.sender = senderMatch[1].trim();
        }

        continue;
      }

      // RAG Context line
      const ragMatch = line.match(/📊 RAG Context:\s+(\d+)\/(\d+)\s+messages\s+\(filtered by\s+(\d+)min window\)/);
      if (ragMatch && currentDecision) {
        const [, filtered, total, timeWindow] = ragMatch;
        currentDecision.ragContext = {
          totalMessages: parseInt(total),
          filteredMessages: parseInt(filtered),
          timeWindowMinutes: parseInt(timeWindow)
        };
        continue;
      }

      // Conversation History header
      if (line.includes('💬 Conversation History')) {
        inConversationHistory = true;
        continue;
      }

      // Conversation History message
      if (inConversationHistory && line.trim().match(/^\d+\./)) {
        const msgMatch = line.match(/\[(\d+)s ago\]\s+([^:]+):\s+"([^"]+)/);
        if (msgMatch) {
          const [, secondsAgo, name, content] = msgMatch;
          const timestamp = Date.now() - (parseInt(secondsAgo) * 1000);
          conversationHistory.push({
            name: name.trim(),
            content: content.trim(),
            timestamp
          });
        }
      }
    }

    // Save last decision
    if (currentDecision && currentDecision.timestamp) {
      if (conversationHistory.length > 0) {
        currentDecision.conversationHistory = conversationHistory;
      }
      decisions.push(currentDecision as ParsedDecision);
    }

    return decisions;
  }

  private generateSummary(decisions: ParsedDecision[]): AIReportResult['summary'] {
    const responseDecisions = decisions.filter(d => d.decision === 'RESPOND').length;
    const silentDecisions = decisions.filter(d => d.decision === 'SILENT').length;
    const totalDecisions = responseDecisions + silentDecisions;

    const confidences = decisions
      .filter(d => d.confidence !== undefined)
      .map(d => d.confidence!);
    const avgConfidence = confidences.length > 0
      ? confidences.reduce((sum, c) => sum + c, 0) / confidences.length
      : 0;

    const timestamps = decisions.map(d => new Date(d.timestamp).getTime());
    const startTime = Math.min(...timestamps);
    const endTime = Math.max(...timestamps);

    return {
      totalDecisions,
      responseDecisions,
      silentDecisions,
      responseRate: totalDecisions > 0 ? (responseDecisions / totalDecisions) * 100 : 0,
      avgConfidence,
      timeRange: {
        start: new Date(startTime).toISOString(),
        end: new Date(endTime).toISOString()
      }
    };
  }

  private generateRoomAnalysis(decisions: ParsedDecision[], roomId: string): AIReportResult['roomAnalysis'] {
    const roomDecisions = decisions.filter(d => d.roomId.startsWith(roomId));
    const totalDecisions = roomDecisions.length;
    const responseDecisions = roomDecisions.filter(d => d.decision === 'RESPOND').length;

    const confidences = roomDecisions
      .filter(d => d.confidence !== undefined)
      .map(d => d.confidence!);
    const avgConfidence = confidences.length > 0
      ? confidences.reduce((sum, c) => sum + c, 0) / confidences.length
      : 0;

    const personaBreakdown: Record<string, PersonaStats> = {};
    for (const decision of roomDecisions) {
      personaBreakdown[decision.persona] ??= {
        decisions: 0,
        responses: 0,
        silences: 0,
        avgConfidence: 0,
        confidences: []
      };

      personaBreakdown[decision.persona].decisions++;
      if (decision.decision === 'RESPOND') {
        personaBreakdown[decision.persona].responses++;
      } else if (decision.decision === 'SILENT') {
        personaBreakdown[decision.persona].silences++;
      }

      if (decision.confidence !== undefined) {
        personaBreakdown[decision.persona].confidences?.push(decision.confidence);
      }
    }

    // Calculate average confidences
    for (const persona in personaBreakdown) {
      const confidences = personaBreakdown[persona].confidences;
      personaBreakdown[persona].avgConfidence = (confidences && confidences.length > 0)
        ? confidences.reduce((sum: number, c: number) => sum + c, 0) / confidences.length
        : 0;
      delete personaBreakdown[persona].confidences;
    }

    return {
      roomId,
      totalDecisions,
      responseRate: totalDecisions > 0 ? (responseDecisions / totalDecisions) * 100 : 0,
      avgConfidence,
      personaBreakdown
    };
  }

  private generatePersonaAnalysis(decisions: ParsedDecision[], personaName: string): AIReportResult['personaAnalysis'] {
    const personaDecisions = decisions.filter(d => d.persona === personaName);
    const totalDecisions = personaDecisions.length;
    const responseDecisions = personaDecisions.filter(d => d.decision === 'RESPOND').length;

    // Confidence distribution
    const confidences = personaDecisions
      .filter(d => d.confidence !== undefined)
      .map(d => d.confidence!);
    const confidenceDistribution = {
      low: confidences.filter(c => c < 0.4).length,
      medium: confidences.filter(c => c >= 0.4 && c < 0.7).length,
      high: confidences.filter(c => c >= 0.7).length
    };

    // Room breakdown
    const roomBreakdown: Record<string, RoomStats> = {};
    for (const decision of personaDecisions) {
      roomBreakdown[decision.roomId] ??= {
        decisions: 0,
        responses: 0,
        silences: 0
      };

      roomBreakdown[decision.roomId].decisions++;
      if (decision.decision === 'RESPOND') {
        roomBreakdown[decision.roomId].responses++;
      } else if (decision.decision === 'SILENT') {
        roomBreakdown[decision.roomId].silences++;
      }
    }

    // Model usage
    const modelUsage: Record<string, number> = {};
    for (const decision of personaDecisions) {
      if (decision.model) {
        modelUsage[decision.model] = (modelUsage[decision.model] ?? 0) + 1;
      }
    }

    return {
      personaName,
      totalDecisions,
      responseRate: totalDecisions > 0 ? (responseDecisions / totalDecisions) * 100 : 0,
      confidenceDistribution,
      roomBreakdown,
      modelUsage
    };
  }

  private generateContextAnalysis(decisions: ParsedDecision[]): AIReportResult['contextAnalysis'] {
    const decisionsWithContext = decisions.filter(d => d.ragContext);

    if (decisionsWithContext.length === 0) {
      return {
        avgMessagesAvailable: 0,
        avgMessagesFiltered: 0,
        insufficientContextCount: 0,
        timeWindowIssues: 0
      };
    }

    const avgMessagesAvailable = decisionsWithContext.reduce((sum, d) => sum + (d.ragContext?.totalMessages ?? 0), 0) / decisionsWithContext.length;
    const avgMessagesFiltered = decisionsWithContext.reduce((sum, d) => sum + (d.ragContext?.filteredMessages ?? 0), 0) / decisionsWithContext.length;

    const insufficientContextCount = decisionsWithContext.filter(d => (d.ragContext?.filteredMessages ?? 0) < 2).length;

    const timeWindowIssues = decisionsWithContext.filter(d => {
      const filtered = d.ragContext?.filteredMessages ?? 0;
      const total = d.ragContext?.totalMessages ?? 0;
      return total > 0 && (filtered / total) < 0.2; // Less than 20% of messages survived filtering
    }).length;

    return {
      avgMessagesAvailable,
      avgMessagesFiltered,
      insufficientContextCount,
      timeWindowIssues
    };
  }

  private generateTimeline(decisions: ParsedDecision[]): AIReportResult['timeline'] {
    return decisions
      .filter(d => d.decision === 'RESPOND' || d.decision === 'SILENT')
      .map(d => ({
        timestamp: d.timestamp,
        persona: d.persona,
        decision: d.decision as 'RESPOND' | 'SILENT',
        confidence: d.confidence ?? 0,
        reason: d.reason,
        roomId: d.roomId
      }));
  }

  private detectIssues(decisions: ParsedDecision[]): AIReportResult['issues'] {
    return {
      lowConfidenceDecisions: decisions
        .filter(d => d.confidence !== undefined && d.confidence < 0.4)
        .map(d => ({
          timestamp: d.timestamp,
          persona: d.persona,
          confidence: d.confidence!,
          reason: d.reason,
          roomId: d.roomId
        })),

      insufficientContext: decisions
        .filter(d => d.ragContext && d.ragContext.filteredMessages < 2)
        .map(d => ({
          timestamp: d.timestamp,
          persona: d.persona,
          messagesAvailable: d.ragContext!.filteredMessages,
          roomId: d.roomId
        })),

      errors: decisions
        .filter(d => d.decision === 'ERROR')
        .map(d => ({
          timestamp: d.timestamp,
          persona: d.persona,
          error: d.reason,
          operation: 'Unknown'
        })),

      redundancyDiscards: [] // TODO: Parse redundancy check lines
    };
  }

  private async recreateDecision(decisions: ParsedDecision[], timestamp: string): Promise<AIReportResult['recreatedDecision'] | undefined> {
    const decision = decisions.find(d => d.timestamp.includes(timestamp));
    if (!decision) {
      return undefined;
    }

    // Build AIDecisionContext using the same structure as PersonaUser
    const decisionContext: AIDecisionContext = {
      personaId: 'unknown', // Log doesn't contain persona ID
      personaName: decision.persona,
      roomId: decision.roomId,
      triggerMessage: {
        id: '',
        roomId: decision.roomId,
        senderId: '',
        senderName: decision.sender ?? 'Unknown',
        senderType: 'user',
        content: { text: decision.message ?? '', attachments: [] },
        timestamp: new Date(decision.timestamp),
        collection: COLLECTIONS.CHAT_MESSAGES,
        version: 1,
        createdAt: new Date(decision.timestamp),
        updatedAt: new Date(decision.timestamp),
        status: 'sent',
        priority: 0,
        reactions: []
      } as unknown as ChatMessageEntity,
      ragContext: {
        domain: 'chat',
        contextId: decision.roomId,
        personaId: 'unknown',
        identity: {
          name: decision.persona,
          systemPrompt: ''
        },
        conversationHistory: (decision.conversationHistory ?? []).map(msg => ({
          role: msg.name === decision.persona ? 'assistant' : 'user',
          content: msg.content,
          name: msg.name,
          timestamp: msg.timestamp
        })),
        artifacts: [],
        privateMemories: [],
        metadata: {
          messageCount: decision.conversationHistory?.length ?? 0,
          artifactCount: 0,
          memoryCount: 0,
          builtAt: new Date()
        }
      }
    };

    // Use AIDecisionService to recreate the decision (same code path as PersonaUser)
    const result = await AIDecisionService.evaluateGating(decisionContext, {
      model: decision.model ?? 'llama3.2:1b',
      temperature: 0.3
    });

    // Reconstruct debug prompt for manual testing
    const conversationContext = decision.conversationHistory
      ? decision.conversationHistory.map(msg => `${msg.name}: ${msg.content}`).join('\n')
      : '';

    const debugPrompt = `You are ${decision.persona}, an AI assistant. Based on this conversation:

${conversationContext}

New message from ${decision.sender}: "${decision.message}"

Should you respond? Consider:
- Is this relevant to your expertise?
- Is someone asking a question?
- Would your response add value?

Answer with "RESPOND" or "SILENT" and explain why.`;

    return {
      timestamp: decision.timestamp,
      persona: decision.persona,
      decision: result.shouldRespond ? 'RESPOND' : 'SILENT',
      confidence: result.confidence,
      reason: result.reason,
      model: result.model,
      conversationHistory: decision.conversationHistory ?? [],
      ragContextSummary: decision.ragContext ?? {
        totalMessages: 0,
        filteredMessages: 0
      },
      debugPrompt
    };
  }

  private getEmptySummary(): AIReportResult['summary'] {
    return {
      totalDecisions: 0,
      responseDecisions: 0,
      silentDecisions: 0,
      responseRate: 0,
      avgConfidence: 0,
      timeRange: {
        start: new Date().toISOString(),
        end: new Date().toISOString()
      }
    };
  }
}
