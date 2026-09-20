/**
 * Grid Deploy Command - Server Implementation
 *
 * Pull latest code and rebuild on grid nodes via SSH over Tailscale.
 */

import { execFileSync } from 'child_process';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridDeployParams, GridDeployResult } from '../shared/GridDeployTypes';
import { createGridDeployResultFromParams } from '../shared/GridDeployTypes';
import { Commands } from '@system/core/shared/Commands';
import { COMMANDS } from '@shared/generated-command-constants';

interface NodeDeployResult {
  nodeId: string;
  status: 'success' | 'failed' | 'unreachable';
  branch?: string;
  buildSuccess?: boolean;
  error?: string;
}

const shellQuote = (value: string): string => `'${value.replace(/'/g, `'\\''`)}'`;

export class GridDeployServerCommand extends CommandBase<GridDeployParams, GridDeployResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/deploy', context, subpath, commander);
  }

  async execute(params: GridDeployParams): Promise<GridDeployResult> {
    console.log('🚀 GRID DEPLOY: Updating remote nodes');

    let nodeIps: string[];
    if (params.nodes) {
      nodeIps = (params.nodes as string).split(',').map(n => n.trim());
    } else {
      try {
        const gridList = await Commands.execute(COMMANDS.GRID_NODES, {}) as unknown as Record<string, unknown>;
        const nodes = (gridList.nodes ?? []) as Array<{ ip: string }>;
        nodeIps = nodes.map(n => n.ip).filter(Boolean);
      } catch {
        nodeIps = [];
      }
    }

    if (nodeIps.length === 0) {
      return createGridDeployResultFromParams(params, {
        success: false,
        deployedNodes: [],
        totalDeployed: 0,
      });
    }

    const results: NodeDeployResult[] = [];

    for (const ip of nodeIps) {
      console.log(`   Deploying to ${ip}...`);
      const result = await this.deployToNode(ip, params.branch, params.skipBuild, params.restart);
      results.push(result);
      console.log(`   ${ip}: ${result.status}${result.error ? ` (${result.error})` : ''}`);
    }

    const deployed = results.filter(r => r.status === 'success').length;
    console.log(`✅ GRID DEPLOY: ${deployed}/${nodeIps.length} nodes updated`);

    return createGridDeployResultFromParams(params, {
      success: deployed > 0,
      deployedNodes: results,
      totalDeployed: deployed,
    });
  }

  private async deployToNode(
    ip: string,
    branch?: string,
    skipBuild?: boolean,
    restart?: boolean,
  ): Promise<NodeDeployResult> {
    const sshUser = process.env.CONTINUUM_SSH_USER ?? process.env.USER ?? process.env.LOGNAME;
    if (!sshUser) {
      return { nodeId: ip, status: 'failed', error: 'CONTINUUM_SSH_USER or USER must be set' };
    }

    const ssh = (cmd: string) =>
      execFileSync(
        'ssh',
        ['-o', 'ConnectTimeout=10', '-o', 'StrictHostKeyChecking=no', `${sshUser}@${ip}`, cmd],
        { encoding: 'utf-8', timeout: 180_000 },
      ).trim();

    try {
      // Find repo
      const repoDir = ssh('ls -d ~/continuum ~/Development/cambrian/continuum 2>/dev/null | head -1');
      if (!repoDir) {
        return { nodeId: ip, status: 'failed', error: 'Repo not found' };
      }

      // Git pull
      let gitCmd = `cd ${shellQuote(repoDir)} && git fetch origin`;
      if (branch) gitCmd += ` && git checkout ${shellQuote(branch)}`;
      gitCmd += ' && git pull';
      ssh(gitCmd);

      const currentBranch = ssh(`cd ${shellQuote(repoDir)} && git branch --show-current`);

      // Build
      let buildSuccess = true;
      if (!skipBuild) {
        try {
          ssh(`cd ${shellQuote(`${repoDir}/src`)} && npm run build:ts 2>&1 | tail -1`);
        } catch {
          buildSuccess = false;
        }
      }

      // Restart
      if (restart) {
        try {
          ssh(`cd ${shellQuote(`${repoDir}/src`)} && npm stop 2>/dev/null; nohup npm start > /dev/null 2>&1 &`);
        } catch { /* backgrounded process — timeout expected */ }
      }

      return { nodeId: ip, status: 'success', branch: currentBranch, buildSuccess };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('Connection refused') || msg.includes('timed out')) {
        return { nodeId: ip, status: 'unreachable', error: 'SSH connection failed' };
      }
      return { nodeId: ip, status: 'failed', error: msg.slice(0, 200) };
    }
  }
}
