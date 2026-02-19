/**
 * Help Command - Server Implementation
 *
 * Discovers READMEs from command tree, auto-generates templates for gaps.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { HelpParams, HelpResult, HelpTopic } from '../shared/HelpTypes';
import { createHelpResultFromParams } from '../shared/HelpTypes';

export class HelpServerCommand extends CommandBase<HelpParams, HelpResult> {
  private commandsDir: string;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('help', context, subpath, commander);
    this.commandsDir = path.join(__dirname, '..', '..');
  }

  async execute(params: HelpParams): Promise<HelpResult> {
    try {
      // List all topics
      if (params.list || !params.path) {
        const topics = this.discoverTopics();
        return createHelpResultFromParams(params, {
          success: true,
          topics,
          content: this.formatTopicList(topics, params.format),
          path: '',
          generated: false,
          format: params.format || 'markdown'
        });
      }

      // Get specific help
      const content = await this.getHelpContent(params.path, params.format);
      return createHelpResultFromParams(params, {
        success: true,
        path: params.path,
        content: content.text,
        generated: content.generated,
        format: params.format || 'markdown',
        topics: []
      });

    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return createHelpResultFromParams(params, {
        success: false,
        path: params.path || '',
        content: `Error: ${message}`,
        generated: false,
        format: params.format || 'markdown',
        topics: []
      });
    }
  }

  /**
   * Discover all command groups that have READMEs or commands
   */
  private discoverTopics(): HelpTopic[] {
    const topics: HelpTopic[] = [];

    if (!fs.existsSync(this.commandsDir)) {
      return topics;
    }

    const entries = fs.readdirSync(this.commandsDir, { withFileTypes: true });

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      if (entry.name === 'help') continue; // Skip ourselves

      const groupPath = path.join(this.commandsDir, entry.name);
      const readmePath = path.join(groupPath, 'README.md');
      const hasReadme = fs.existsSync(readmePath);

      // Find commands in this group
      const commands = this.findCommands(groupPath, entry.name);

      if (commands.length > 0 || hasReadme) {
        topics.push({
          path: entry.name,
          title: this.formatTitle(entry.name),
          hasReadme,
          commands
        });
      }
    }

    return topics.sort((a, b) => a.path.localeCompare(b.path));
  }

  /**
   * Find all commands under a path
   */
  private findCommands(dirPath: string, prefix: string): string[] {
    const commands: string[] = [];

    const searchDir = (dir: string, currentPath: string) => {
      if (!fs.existsSync(dir)) return;

      const entries = fs.readdirSync(dir, { withFileTypes: true });

      // Check if this dir has a shared/*Types.ts (indicates a command)
      const hasTypes = entries.some(e =>
        e.name === 'shared' &&
        fs.existsSync(path.join(dir, 'shared')) &&
        fs.readdirSync(path.join(dir, 'shared')).some(f => f.endsWith('Types.ts'))
      );

      if (hasTypes && currentPath !== prefix) {
        commands.push(currentPath);
      }

      // Recurse into subdirectories
      for (const entry of entries) {
        if (entry.isDirectory() && !['shared', 'server', 'browser', 'test', 'node_modules'].includes(entry.name)) {
          searchDir(path.join(dir, entry.name), `${currentPath}/${entry.name}`);
        }
      }
    };

    searchDir(dirPath, prefix);
    return commands;
  }

  /**
   * Get help content for a specific path
   */
  private async getHelpContent(helpPath: string, format?: string): Promise<{ text: string; generated: boolean }> {
    const fullPath = path.join(this.commandsDir, ...helpPath.split('/'));
    const readmePath = path.join(fullPath, 'README.md');

    // Check for existing README
    if (fs.existsSync(readmePath)) {
      let content = fs.readFileSync(readmePath, 'utf-8');

      // Condense for RAG format
      if (format === 'rag') {
        content = this.condenseForRag(content);
      }

      return { text: content, generated: false };
    }

    // Generate template from command schema
    return {
      text: this.generateTemplate(helpPath, fullPath),
      generated: true
    };
  }

  /**
   * Generate a help template for commands without READMEs
   */
  private generateTemplate(helpPath: string, fullPath: string): string {
    const commandName = helpPath;

    // Try to find schema info
    let params = '';
    const sharedDir = path.join(fullPath, 'shared');
    if (fs.existsSync(sharedDir)) {
      const typesFile = fs.readdirSync(sharedDir).find(f => f.endsWith('Types.ts'));
      if (typesFile) {
        const typesContent = fs.readFileSync(path.join(sharedDir, typesFile), 'utf-8');
        params = this.extractParamsFromTypes(typesContent);
      }
    }

    // Find subcommands
    const subcommands = this.findCommands(fullPath, helpPath)
      .map(cmd => `- \`./jtag ${cmd}\``)
      .join('\n');

    return `# ${commandName}

> Auto-generated help - no README found. Edit \`commands/${helpPath}/README.md\` to customize.

## Usage

\`\`\`bash
./jtag ${commandName} [options]
\`\`\`

${params ? `## Parameters\n\n${params}\n` : ''}
${subcommands ? `## Subcommands\n\n${subcommands}\n` : ''}
## Examples

\`\`\`bash
# Basic usage
./jtag ${commandName}

# With options
./jtag ${commandName} --help
\`\`\`

## Scenarios

<!-- TODO: Add common use cases -->

---
*This help was auto-generated. Create \`commands/${helpPath}/README.md\` for custom documentation.*
`;
  }

  /**
   * Extract parameter info from Types.ts file
   */
  private extractParamsFromTypes(content: string): string {
    const params: string[] = [];

    // Match interface Params block (can be multi-line)
    const interfaceMatch = content.match(/interface \w+Params[^{]*\{([\s\S]*?)\n\}/);
    if (interfaceMatch) {
      const propsContent = interfaceMatch[1];

      // Match JSDoc + property pairs more carefully
      // We need to match: /** JSDoc */ name?: Type;
      const propPattern = /\/\*\*\s*([\s\S]*?)\s*\*\/\s*(\w+)(\?)?:\s*([^;]+);/g;
      let match;

      while ((match = propPattern.exec(propsContent)) !== null) {
        const jsdocRaw = match[1];
        const name = match[2];
        const optional = match[3] === '?';
        const type = match[4].trim();

        // Clean up JSDoc content - remove * prefixes and collapse whitespace
        const description = jsdocRaw
          .split('\n')
          .map(line => line.replace(/^\s*\*\s?/, '').trim())
          .filter(line => line.length > 0)
          .join(' ');

        const optionalStr = optional ? ' (optional)' : '';
        params.push(`- \`--${name}\` (${type})${optionalStr}: ${description || 'No description'}`);
      }
    }

    return params.join('\n');
  }

  /**
   * Condense content for RAG (remove verbose sections)
   */
  private condenseForRag(content: string): string {
    // Remove code blocks that are just examples
    let condensed = content.replace(/```[\s\S]*?```/g, '[code example]');

    // Remove excessive whitespace
    condensed = condensed.replace(/\n{3,}/g, '\n\n');

    // Keep it under ~500 tokens
    if (condensed.length > 2000) {
      condensed = condensed.substring(0, 2000) + '\n\n[truncated for RAG]';
    }

    return condensed;
  }

  /**
   * Format topic list for display
   */
  private formatTopicList(topics: HelpTopic[], format?: string): string {
    if (format === 'json') {
      return JSON.stringify(topics, null, 2);
    }

    let output = '# JTAG Command Help\n\n';
    output += 'Available command groups:\n\n';

    for (const topic of topics) {
      const icon = topic.hasReadme ? '📖' : '📝';
      output += `## ${icon} ${topic.title}\n`;
      output += `\`./jtag help --path=${topic.path}\`\n\n`;

      if (topic.commands && topic.commands.length > 0) {
        output += 'Commands:\n';
        for (const cmd of topic.commands.slice(0, 5)) {
          output += `- \`./jtag ${cmd}\`\n`;
        }
        if (topic.commands.length > 5) {
          output += `- ... and ${topic.commands.length - 5} more\n`;
        }
        output += '\n';
      }
    }

    output += '---\n';
    output += '📖 = Has documentation | 📝 = Auto-generated (needs README)\n';

    return output;
  }

  /**
   * Format title from path
   */
  private formatTitle(pathPart: string): string {
    return pathPart
      .split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }
}
