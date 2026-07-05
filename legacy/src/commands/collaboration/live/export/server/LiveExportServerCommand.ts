/**
 * Live Export Command - Server Implementation
 *
 * Exports recent utterances from the active live voice session
 * to markdown format. Mirrors collaboration/chat/export.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { getTSVoiceOrchestrator } from '@system/voice/server';
import { extractSentiment, formatEmotionLabel } from '@system/rag/shared/TextSentiment';
import type { LiveExportParams, LiveExportResult } from '../shared/LiveExportTypes';
import * as fs from 'fs';
import * as path from 'path';

export class LiveExportServerCommand extends CommandBase<LiveExportParams, LiveExportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/export', context, subpath, commander);
  }

  async execute(params: LiveExportParams): Promise<LiveExportResult> {
    const orchestrator = getTSVoiceOrchestrator();

    // Auto-discover active session
    const sessionId = params.callSessionId ?? orchestrator.activeSessionId;
    if (!sessionId) {
      return transformPayload(params, {
        success: false,
        message: 'No active live session.',
        utteranceCount: 0,
        callSessionId: '',
      });
    }

    const limit = params.limit ?? 20;
    const includeEmotions = params.includeEmotions ?? true;

    // Get utterances from VoiceOrchestrator's in-memory session context
    const utterances = orchestrator.getRecentUtterances(sessionId, limit);
    const participants = orchestrator.getParticipants(sessionId);

    if (utterances.length === 0) {
      const markdown = '# Live Call Transcript\n\nNo utterances yet.\n';
      console.log(markdown);
      return transformPayload(params, {
        success: true,
        message: 'No utterances in session',
        utteranceCount: 0,
        markdown,
        callSessionId: sessionId,
      });
    }

    // Generate markdown
    const markdown = this.generateMarkdown(utterances, participants, includeEmotions, sessionId);

    // Write to file or return as string
    if (params.output) {
      const filepath = path.resolve(params.output);
      const dir = path.dirname(filepath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(filepath, markdown, 'utf-8');
      console.log(`Exported ${utterances.length} utterances to ${filepath}`);

      return transformPayload(params, {
        success: true,
        message: `Exported ${utterances.length} utterances to ${filepath}`,
        utteranceCount: utterances.length,
        filepath,
        callSessionId: sessionId,
      });
    }

    console.log(markdown);
    return transformPayload(params, {
      success: true,
      message: `Exported ${utterances.length} utterances`,
      utteranceCount: utterances.length,
      markdown,
      callSessionId: sessionId,
    });
  }

  private generateMarkdown(
    utterances: Array<{
      sessionId: string;
      speakerId: string;
      speakerName: string;
      speakerType: 'human' | 'persona' | 'agent';
      transcript: string;
      confidence: number;
      timestamp: number;
    }>,
    participants: Array<{ userId: string; displayName: string; type: string }>,
    includeEmotions: boolean,
    sessionId: string,
  ): string {
    const lines: string[] = [];

    // Header
    lines.push('# Live Call Transcript');
    lines.push('');
    lines.push(`Session: ${sessionId}`);
    lines.push(`Exported: ${new Date().toISOString()}`);
    lines.push(`Utterances: ${utterances.length}`);
    lines.push(`Participants: ${participants.map(p => p.displayName).join(', ')}`);
    lines.push('');
    lines.push('---');
    lines.push('');

    // Utterances
    for (const u of utterances) {
      const timestamp = new Date(u.timestamp).toLocaleString();
      const speakerLabel = this.getSpeakerLabel(u.speakerType);

      let emotionTag = '';
      if (includeEmotions) {
        const sentiment = extractSentiment(u.transcript);
        const label = formatEmotionLabel(sentiment);
        if (label) emotionTag = ` (${label})`;
      }

      lines.push(`**${speakerLabel} ${u.speakerName}${emotionTag}** — *${timestamp}*`);
      lines.push('');
      lines.push(u.transcript);
      lines.push('');
      lines.push('---');
      lines.push('');
    }

    return lines.join('\n');
  }

  private getSpeakerLabel(type: string): string {
    switch (type) {
      case 'human': return '[HUMAN]';
      case 'persona': return '[AI]';
      case 'agent': return '[AGENT]';
      default: return '[UNKNOWN]';
    }
  }
}
