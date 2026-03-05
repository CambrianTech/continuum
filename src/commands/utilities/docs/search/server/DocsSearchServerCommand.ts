import { DocsSearchCommand } from '../shared/DocsSearchCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DocsSearchParams, DocsSearchResult } from '../shared/DocsSearchTypes';
import { DocFileRegistry } from '../../shared/DocFileRegistry';

const registry = new DocFileRegistry();

export class DocsSearchServerCommand extends DocsSearchCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('utilities/docs/search', context, subpath, commander);
  }

  async execute(params: DocsSearchParams): Promise<DocsSearchResult> {
    if (!params.pattern || params.pattern.trim().length === 0) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: 'pattern parameter is required',
        pattern: params.pattern ?? '',
        matches: [],
        totalMatches: 0,
      };
    }

    const maxMatches = params.maxMatches ?? 50;
    const flags = params.caseSensitive ? '' : 'i';

    let regex: RegExp;
    try {
      regex = new RegExp(params.pattern, flags);
    } catch {
      // Fall back to literal match if regex is invalid
      regex = new RegExp(params.pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), flags);
    }

    const docs = await registry.discover();
    const matches: Array<{ doc: string; lineNumber: number; content: string }> = [];

    for (const doc of docs) {
      if (matches.length >= maxMatches) break;

      try {
        const content = await this.readFileContent(doc.filePath);
        const lines = content.split('\n');

        for (let i = 0; i < lines.length; i++) {
          if (matches.length >= maxMatches) break;
          if (regex.test(lines[i])) {
            matches.push({
              doc: doc.name,
              lineNumber: i + 1,
              content: lines[i].trim().slice(0, 200), // Cap line length
            });
          }
        }
      } catch {
        // Skip files we can't read
      }
    }

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      pattern: params.pattern,
      matches,
      totalMatches: matches.length,
    };
  }

  private async readFileContent(filePath: string): Promise<string> {
    const fs = await import('fs/promises');
    return fs.readFile(filePath, 'utf-8');
  }
}
