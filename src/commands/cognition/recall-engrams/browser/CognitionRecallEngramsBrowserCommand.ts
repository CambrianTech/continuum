/**
 * Cognition Recall Engrams Command - Browser Implementation
 *
 * Query a persona's admitted-engram store. Modes: 'recent' (default) returns newest-first N engrams; 'by_id' looks up by exact engram id; 'by_keyword' does case-insensitive substring match; 'by_origin' filters by EngramOriginKind (chat | airc | tool | self_reflection). Wraps the Rust IPC handler shipped in #1121 PR-5.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CognitionRecallEngramsParams, CognitionRecallEngramsResult } from '../shared/CognitionRecallEngramsTypes';

export class CognitionRecallEngramsBrowserCommand extends CommandBase<CognitionRecallEngramsParams, CognitionRecallEngramsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/recall-engrams', context, subpath, commander);
  }

  async execute(params: CognitionRecallEngramsParams): Promise<CognitionRecallEngramsResult> {
    console.log('🌐 BROWSER: Delegating Cognition Recall Engrams to server');
    return await this.remoteExecute(params);
  }
}
