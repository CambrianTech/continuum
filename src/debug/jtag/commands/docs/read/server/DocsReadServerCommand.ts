import { DocsReadCommand } from '../shared/DocsReadCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DocsReadParams, DocsReadResult, SectionInfo } from '../shared/DocsReadTypes';
import { DocFileRegistry } from '../../shared/DocFileRegistry';
import * as fs from 'fs/promises';

export class DocsReadServerCommand extends DocsReadCommand {
  private registry = new DocFileRegistry();

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('docs/read', context, subpath, commander);
  }

  async execute(params: DocsReadParams): Promise<DocsReadResult> {
    // Find the doc file
    const allDocs = await this.registry.discover();
    const doc = allDocs.find(d => d.name === params.doc);
    if (!doc) throw new Error(`Doc not found: ${params.doc}`);

    // Read file
    const content = await fs.readFile(doc.filePath, 'utf-8');
    const lines = content.split('\n');

    // Extract table of contents if requested
    if (params.toc) {
      const toc = this.extractToC(lines);

      // If also requesting specific section, include content too
      if (params.section) {
        const sectionContent = this.extractSection(lines, params.section, toc);
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: true,
          doc: params.doc,
          toc,
          content: sectionContent,
          totalLines: lines.length
        };
      }

      // ToC only
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        doc: params.doc,
        toc,
        totalLines: lines.length
      };
    }

    // If section specified without --toc, just return that section
    if (params.section) {
      const toc = this.extractToC(lines);
      const sectionContent = this.extractSection(lines, params.section, toc);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        doc: params.doc,
        content: sectionContent,
        totalLines: lines.length
      };
    }

    // Regular read with line range
    let contentLines = lines;
    if (params.startLine && params.endLine) {
      contentLines = lines.slice(params.startLine - 1, params.endLine);
    }

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      doc: params.doc,
      content: contentLines.join('\n'),
      totalLines: lines.length
    };
  }

  private extractToC(lines: string[]): SectionInfo[] {
    const toc: SectionInfo[] = [];
    let currentSection: SectionInfo | null = null;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);

      if (headerMatch) {
        // Save previous section's end line
        if (currentSection) {
          currentSection.lines[1] = i - 1;
        }

        // Start new section
        currentSection = {
          level: headerMatch[1].length,
          title: headerMatch[2].trim(),
          lines: [i + 1, lines.length]  // 1-indexed, will update end when next section starts
        };
        toc.push(currentSection);
      }
    }

    return toc;
  }

  private extractSection(lines: string[], sectionTitle: string, toc: SectionInfo[]): string {
    const section = toc.find(s => s.title.toLowerCase() === sectionTitle.toLowerCase());
    if (!section) throw new Error(`Section not found: ${sectionTitle}`);

    return lines.slice(section.lines[0] - 1, section.lines[1]).join('\n');
  }
}
