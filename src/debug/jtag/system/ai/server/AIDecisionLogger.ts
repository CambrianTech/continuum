/**
 * AI Decision Logger - Separate log file for AI gating decisions
 *
 * Writes AI decision-making logs to dedicated file to avoid polluting server.log
 * Location: .continuum/jtag/sessions/system/{systemSessionId}/logs/ai-decisions.log
 */

import * as fs from 'fs';
import * as path from 'path';

export class AIDecisionLogger {
  private static logPath: string | null = null;

  /**
   * Initialize logger with session-specific log path
   */
  static initialize(sessionId: string): void {
    const logDir = path.join(
      process.cwd(),
      '.continuum/jtag/sessions/system',
      sessionId,
      'logs'
    );

    // Ensure log directory exists
    if (!fs.existsSync(logDir)) {
      fs.mkdirSync(logDir, { recursive: true });
    }

    this.logPath = path.join(logDir, 'ai-decisions.log');

    // Write header on initialization
    this.writeLog('='.repeat(80));
    this.writeLog(`AI Decision Log Session Started: ${new Date().toISOString()}`);
    this.writeLog('='.repeat(80));
  }

  /**
   * Log an AI decision (RESPOND or SILENT) with detailed context for debugging
   */
  static logDecision(
    personaName: string,
    decision: 'RESPOND' | 'SILENT',
    reason: string,
    context: {
      message: string;
      sender: string;
      roomId: string;
      mentioned?: boolean;
      humanSender?: boolean;
      confidence?: number;
      model?: string;
      ragContextSummary?: {
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
  ): void {
    const timestamp = new Date().toISOString();
    const roomIdShort = context.roomId.slice(0, 8);
    const messagePreview = context.message.slice(0, 80);

    // Main decision line
    const mainLine = [
      `[${timestamp}]`,
      `${personaName} → ${decision}`,
      `| Room: ${roomIdShort}`,
      context.confidence !== undefined ? `| Confidence: ${context.confidence.toFixed(2)}` : '',
      context.model ? `| Model: ${context.model}` : '',
      `| Reason: ${reason}`,
      `| Message: "${messagePreview}${context.message.length > 80 ? '...' : ''}"`,
      `| Sender: ${context.sender}`,
      context.mentioned ? '| MENTIONED' : '',
      context.humanSender !== undefined ? `| Human: ${context.humanSender}` : ''
    ]
      .filter(Boolean)
      .join(' ');

    this.writeLog(mainLine);

    // RAG context summary (if provided)
    if (context.ragContextSummary) {
      const { totalMessages, filteredMessages, timeWindowMinutes } = context.ragContextSummary;
      const ragSummary = `    📊 RAG Context: ${filteredMessages}/${totalMessages} messages (filtered by ${timeWindowMinutes}min window)`;
      this.writeLog(ragSummary);
    }

    // Conversation history (if provided and verbose logging enabled)
    if (context.conversationHistory && context.conversationHistory.length > 0) {
      try {
        this.writeLog(`    💬 Conversation History (${context.conversationHistory.length} messages):`);
        context.conversationHistory.forEach((msg, idx) => {
          const msgPreview = msg.content?.slice(0, 60) || '';
          // Defensive handling for undefined/invalid timestamps
          let timeAgo = 'unknown time';
          if (msg.timestamp && typeof msg.timestamp === 'number' && !isNaN(msg.timestamp)) {
            const secondsAgo = Math.floor((Date.now() - msg.timestamp) / 1000);
            timeAgo = !isNaN(secondsAgo) ? `${secondsAgo}s ago` : 'invalid time';
          }
          this.writeLog(`       ${idx + 1}. [${timeAgo}] ${msg.name || 'Unknown'}: "${msgPreview}${(msg.content?.length || 0) > 60 ? '...' : ''}"`);
        });
      } catch (error) {
        this.writeLog(`    ⚠️ Error logging conversation history: ${error}`);
      }
    }

    // Separator for readability
    if (context.conversationHistory && context.conversationHistory.length > 0) {
      this.writeLog(''); // Empty line for readability
    }

    // Also log to console with AI prefix for backward compatibility
    // console.log(`🤖 AI-DECISION: ${mainLine}`);
  }

  /**
   * Log redundancy check (self-review before posting)
   */
  static logRedundancyCheck(
    personaName: string,
    roomId: string,
    isRedundant: boolean,
    reason: string,
    responsePreview: string
  ): void {
    const timestamp = new Date().toISOString();
    const roomIdShort = roomId.slice(0, 8);
    const preview = responsePreview.slice(0, 80);
    const decision = isRedundant ? 'DISCARD' : 'ALLOW';

    const logLine = `[${timestamp}] ${personaName} → REDUNDANCY-CHECK: ${decision} | Room: ${roomIdShort} | Reason: ${reason} | Draft: "${preview}${responsePreview.length > 80 ? '...' : ''}"`;

    this.writeLog(logLine);
    console.log(`🤖 AI-REDUNDANCY: ${logLine}`);
  }

  /**
   * Log AI response generation (when AI actually posts)
   */
  static logResponse(
    personaName: string,
    roomId: string,
    responsePreview: string
  ): void {
    const timestamp = new Date().toISOString();
    const roomIdShort = roomId.slice(0, 8);
    const preview = responsePreview.slice(0, 100);

    const logLine = `[${timestamp}] ${personaName} → POSTED | Room: ${roomIdShort} | Response: "${preview}${responsePreview.length > 100 ? '...' : ''}"`;

    this.writeLog(logLine);
    console.log(`🤖 AI-RESPONSE: ${logLine}`);
  }

  /**
   * Log AI error (when decision-making fails)
   */
  static logError(
    personaName: string,
    operation: string,
    error: string
  ): void {
    const timestamp = new Date().toISOString();
    const logLine = `[${timestamp}] ${personaName} → ERROR | Operation: ${operation} | Error: ${error}`;

    this.writeLog(logLine);
    console.error(`❌ AI-ERROR: ${logLine}`);
  }

  /**
   * Write to log file (internal)
   */
  private static writeLog(line: string): void {
    if (!this.logPath) {
      console.warn('⚠️  AIDecisionLogger: Not initialized, skipping log write');
      return;
    }

    try {
      // Ensure directory exists before writing
      const logDir = path.dirname(this.logPath);
      if (!fs.existsSync(logDir)) {
        fs.mkdirSync(logDir, { recursive: true });
      }

      fs.appendFileSync(this.logPath, line + '\n', 'utf-8');
    } catch (error) {
      console.error('❌ AIDecisionLogger: Failed to write log:', error);
    }
  }

  /**
   * Get current log file path
   */
  static getLogPath(): string | null {
    return this.logPath;
  }
}
