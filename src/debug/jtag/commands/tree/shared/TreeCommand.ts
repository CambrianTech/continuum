/**
 * Tree Command - Shared Implementation
 *
 * Auto-generates hierarchical command structure from the dynamic command registry.
 * No hardcoding - builds tree from actual command list.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type TreeParams, type TreeResult, type TreeNode, createTreeResultFromParams } from './TreeTypes';
import { Commands } from '../../../system/core/shared/Commands';
import type { CommandSignature } from '../../list/shared/ListTypes';

export class TreeCommand extends CommandBase<TreeParams, TreeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('tree', context, subpath, commander);
  }

  /**
   * Execute tree command - auto-generate hierarchical structure from command list
   */
  async execute(params: JTAGPayload): Promise<TreeResult> {
    const treeParams = params as TreeParams;
    const env = this.context.environment;

    console.log(`🌳 ${env.toUpperCase()}: Building command tree (filter: ${treeParams.filter ?? 'all'})...`);

    try {
      // Get all commands from list command (dynamic discovery)
      const listResult = await Commands.execute('list', {}) as unknown as {
        commands?: CommandSignature[];
        success: boolean;
        error?: string;
      };

      if (!listResult.success || !listResult.commands) {
        throw new Error(`Failed to get command list: ${listResult.error}`);
      }

      // Filter commands if needed
      let commands = listResult.commands;
      if (treeParams.filter) {
        commands = commands.filter(cmd =>
          cmd.name.startsWith(treeParams.filter + '/') ||
          cmd.name === treeParams.filter
        );
      }

      // Build tree structure
      const tree = this.buildTree(commands, treeParams);

      // Generate ASCII visualization
      const visualization = this.generateVisualization(tree, treeParams);

      return createTreeResultFromParams(treeParams, {
        success: true,
        tree,
        commandCount: commands.length,
        visualization
      });

    } catch (error) {
      console.error(`❌ ${env.toUpperCase()}: Failed to build command tree:`, error);

      return createTreeResultFromParams(treeParams, {
        success: false,
        tree: [],
        commandCount: 0,
        visualization: '',
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  /**
   * Build hierarchical tree structure from flat command list
   */
  private buildTree(commands: CommandSignature[], params: TreeParams): TreeNode[] {
    const root: TreeNode[] = [];
    const nodeMap = new Map<string, TreeNode>();

    // Sort commands alphabetically for consistent tree structure
    const sortedCommands = [...commands].sort((a, b) => a.name.localeCompare(b.name));

    for (const cmd of sortedCommands) {
      const parts = cmd.name.split('/');
      let currentPath = '';

      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        const isLeaf = i === parts.length - 1;
        currentPath = currentPath ? `${currentPath}/${part}` : part;

        // Skip if we already created this node
        if (nodeMap.has(currentPath)) {
          continue;
        }

        // Create new node
        const node: TreeNode = {
          name: part,
          fullPath: currentPath,
          isCommand: isLeaf,
          description: isLeaf ? cmd.description : undefined,
          children: []
        };

        // Add to map
        nodeMap.set(currentPath, node);

        // Add to parent or root
        if (i === 0) {
          root.push(node);
        } else {
          const parentPath = parts.slice(0, i).join('/');
          const parent = nodeMap.get(parentPath);
          if (parent) {
            parent.children.push(node);
          }
        }
      }
    }

    // Apply maxDepth filtering if specified
    if (params.maxDepth !== undefined) {
      return this.filterByDepth(root, params.maxDepth, 1);
    }

    return root;
  }

  /**
   * Filter tree to max depth
   */
  private filterByDepth(nodes: TreeNode[], maxDepth: number, currentDepth: number): TreeNode[] {
    if (currentDepth >= maxDepth) {
      // At max depth - only keep leaf commands, remove parent groups
      return nodes.filter(n => n.isCommand);
    }

    return nodes.map(node => ({
      ...node,
      children: this.filterByDepth(node.children, maxDepth, currentDepth + 1)
    }));
  }

  /**
   * Generate ASCII tree visualization
   */
  private generateVisualization(tree: TreeNode[], params: TreeParams): string {
    const lines: string[] = [];

    lines.push(''); // Empty line for spacing
    lines.push('COMMAND TREE:');
    lines.push('');

    this.renderNode(tree, lines, '', params);

    return lines.join('\n');
  }

  /**
   * Recursively render tree nodes as ASCII art
   */
  private renderNode(
    nodes: TreeNode[],
    lines: string[],
    prefix: string,
    params: TreeParams
  ): void {
    nodes.forEach((node, index) => {
      const isLast = index === nodes.length - 1;
      const connector = isLast ? '└── ' : '├── ';
      const childPrefix = isLast ? '    ' : '│   ';

      // Build line with node name
      let line = prefix + connector + node.name;

      // Add description if requested and available
      if (params.showDescriptions && node.description) {
        line += ` - ${node.description}`;
      }

      lines.push(line);

      // Recursively render children
      if (node.children.length > 0) {
        this.renderNode(node.children, lines, prefix + childPrefix, params);
      }
    });
  }
}
