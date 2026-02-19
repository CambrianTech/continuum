/**
 * Codebase Index Command - Shared Base
 *
 * Index TypeScript and Markdown files with domain-specific embeddings
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { CodebaseIndexParams, CodebaseIndexResult } from './CodebaseIndexTypes';

export abstract class CodebaseIndexCommand extends CommandBase<CodebaseIndexParams, CodebaseIndexResult> {
  getDescription(): string {
    return 'Index TypeScript and Markdown files with embeddings for semantic code search';
  }
}
