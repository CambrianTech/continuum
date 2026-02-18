/**
 * Runtime Metrics Command - Server Implementation
 *
 * Query Rust module performance metrics including latency percentiles, command counts, and slow command tracking.
 * Enables AI-driven system analysis and optimization (Ares pattern).
 *
 * Routes to Rust RuntimeModule via continuum-core IPC:
 * - runtime/list: Module configurations
 * - runtime/metrics/all: All module metrics
 * - runtime/metrics/module: Specific module metrics
 * - runtime/metrics/slow: Recent slow commands
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  RuntimeMetricsParams,
  RuntimeMetricsResult,
  ModuleMetrics,
  SlowCommand,
  ModuleConfig,
} from '../shared/RuntimeMetricsTypes';
import { createRuntimeMetricsResultFromParams } from '../shared/RuntimeMetricsTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class RuntimeMetricsServerCommand extends CommandBase<RuntimeMetricsParams, RuntimeMetricsResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('runtime/metrics', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: RuntimeMetricsParams): Promise<RuntimeMetricsResult> {
    const mode = params.mode ?? 'all';

    // Validate module parameter when mode='module'
    if (mode === 'module' && (!params.module || params.module.trim() === '')) {
      throw new ValidationError(
        'module',
        `Missing required parameter 'module' when mode='module'. ` +
        `Use --module=<name> to specify the module (e.g., --module=data, --module=embedding).`
      );
    }

    await this.rustClient.connect();

    try {
      switch (mode) {
        case 'list': {
          const result = await this.rustClient.runtimeList();
          const moduleConfigs: ModuleConfig[] = result.modules.map((m) => ({
            name: m.name,
            priority: m.priority,
            commandPrefixes: m.command_prefixes,
            needsDedicatedThread: m.needs_dedicated_thread,
            maxConcurrency: m.max_concurrency,
          }));

          return createRuntimeMetricsResultFromParams(params, {
            success: true,
            modules: [],
            slowCommands: [],
            moduleConfigs,
            count: result.count,
            thresholdMs: 0,
          });
        }

        case 'module': {
          const result = await this.rustClient.runtimeMetricsModule(params.module!);
          const modules: ModuleMetrics[] = [{
            moduleName: result.moduleName,
            totalCommands: result.totalCommands,
            avgTimeMs: result.avgTimeMs,
            slowCommandCount: result.slowCommandCount,
            p50Ms: result.p50Ms,
            p95Ms: result.p95Ms,
            p99Ms: result.p99Ms,
          }];

          return createRuntimeMetricsResultFromParams(params, {
            success: true,
            modules,
            slowCommands: [],
            moduleConfigs: [],
            count: 1,
            thresholdMs: 0,
          });
        }

        case 'slow': {
          const result = await this.rustClient.runtimeMetricsSlow();
          const slowCommands: SlowCommand[] = result.slow_commands.map((c) => ({
            module: c.module,
            command: c.command,
            totalMs: c.total_ms,
            executeMs: c.execute_ms,
            queueMs: c.queue_ms,
          }));

          return createRuntimeMetricsResultFromParams(params, {
            success: true,
            modules: [],
            slowCommands,
            moduleConfigs: [],
            count: result.count,
            thresholdMs: result.threshold_ms,
          });
        }

        case 'all':
        default: {
          const result = await this.rustClient.runtimeMetricsAll();
          const modules: ModuleMetrics[] = result.modules.map((m) => ({
            moduleName: m.moduleName,
            totalCommands: m.totalCommands,
            avgTimeMs: m.avgTimeMs,
            slowCommandCount: m.slowCommandCount,
            p50Ms: m.p50Ms,
            p95Ms: m.p95Ms,
            p99Ms: m.p99Ms,
          }));

          return createRuntimeMetricsResultFromParams(params, {
            success: true,
            modules,
            slowCommands: [],
            moduleConfigs: [],
            count: result.count,
            thresholdMs: 0,
          });
        }
      }
    } finally {
      this.rustClient.disconnect();
    }
  }
}
