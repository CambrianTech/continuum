import { DocsListCommand } from '../shared/DocsListCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DocsListParams, DocsListResult } from '../shared/DocsListTypes';
import { DocFileRegistry } from '../../shared/DocFileRegistry';

const registry = new DocFileRegistry();

export class DocsListServerCommand extends DocsListCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('utilities/docs/list', context, subpath, commander);
  }

  async execute(params: DocsListParams): Promise<DocsListResult> {
    let docs = await registry.discover();

    // Filter out READMEs unless explicitly requested
    if (!params.includeReadmes) {
      docs = docs.filter(d => !d.filePath.endsWith('/README.md'));
    }

    const byDirectory: Record<string, number> = {};
    docs.forEach(doc => {
      byDirectory[doc.directory] = (byDirectory[doc.directory] || 0) + 1;
    });

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      docs,
      summary: {
        totalDocs: docs.length,
        totalSizeMB: docs.reduce((sum, d) => sum + d.sizeMB, 0),
        byDirectory
      }
    };
  }
}
