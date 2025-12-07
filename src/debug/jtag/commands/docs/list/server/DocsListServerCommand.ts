import { DocsListCommand } from '../shared/DocsListCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DocsListParams, DocsListResult, DocMetadata } from '../shared/DocsListTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

export class DocsListServerCommand extends DocsListCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('docs/list', context, subpath, commander);
  }

  async execute(params: DocsListParams): Promise<DocsListResult> {
    const projectRoot = process.cwd();
    const docs = await this.discoverDocs(projectRoot, params);

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

  private async discoverDocs(root: string, params: DocsListParams): Promise<DocMetadata[]> {
    const docs: DocMetadata[] = [];
    
    async function scan(dir: string): Promise<void> {
      try {
        const entries = await fs.readdir(dir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = path.join(dir, entry.name);
          
          // Skip node_modules and .continuum
          if (entry.isDirectory() && !entry.name.startsWith('.') && entry.name !== 'node_modules') {
            await scan(fullPath);
          } else if (entry.isFile() && entry.name.endsWith('.md')) {
            // Skip READMEs unless explicitly requested
            if (!params.includeReadmes && entry.name === 'README.md') continue;
            
            const stats = await fs.stat(fullPath);
            const content = await fs.readFile(fullPath, 'utf-8');
            const lines = content.split('\n');
            
            // Extract H1/H2 sections
            const sections = lines
              .filter(l => l.match(/^#{1,2}\s+/))
              .map(l => l.replace(/^#{1,2}\s+/, '').trim())
              .slice(0, 10);  // First 10 sections
            
            docs.push({
              name: pathToDocName(fullPath),
              filePath: fullPath,
              directory: path.dirname(fullPath).replace(root + '/', ''),
              sizeMB: stats.size / (1024 * 1024),
              lineCount: lines.length,
              lastModified: stats.mtime.toISOString(),
              sections
            });
          }
        }
      } catch (err) {
        // Skip inaccessible directories
      }
    }
    
    await scan(root);
    return docs.sort((a, b) => a.name.localeCompare(b.name));
  }
}

function pathToDocName(filePath: string): string {
  const relativePath = filePath.replace(/^.*\/(src\/debug\/jtag|continuum)\//, '');
  const withoutExt = relativePath.replace(/\.md$/, '');
  if (withoutExt.startsWith('docs/')) {
    return withoutExt.replace('docs/', '');
  }
  return withoutExt;
}
