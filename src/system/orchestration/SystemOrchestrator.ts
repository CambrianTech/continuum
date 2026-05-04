/**
 * System Orchestrator - Universal Entry Point Logic with Milestone-Based Orchestration
 * 
 * Provides single orchestration point for all entry points (npm start, npm test, CLI, etc.)
 * Ensures proper milestone execution order and fixes browser timing issues through signaling.
 * 
 * CRITICAL: Browser launch ONLY happens after SERVER_READY milestone is reached.
 */

import { EventEmitter } from 'events';
import { spawn, spawnSync, ChildProcess, exec } from 'child_process';
import { promisify } from 'util';
import { existsSync, readFileSync } from 'fs';
import { stat } from 'fs/promises';
import * as net from 'net';
import * as path from 'path';
import { WorkingDirConfig } from '../core/config/WorkingDirConfig';

const execAsync = promisify(exec);

/** Platform-aware browser open command. */
function openBrowserCommand(url: string): { cmd: string; args: string[] } {
  if (process.platform === 'darwin') {
    return { cmd: 'open', args: [url] };
  }
  if (process.platform === 'win32') {
    return { cmd: 'cmd.exe', args: ['/c', 'start', '', url] };
  }
  // Linux — check for WSL (open Windows default browser via explorer.exe)
  try {
    const version = readFileSync('/proc/version', 'utf-8');
    if (version.toLowerCase().includes('microsoft')) {
      // explorer.exe is always available on WSL and opens the user's DEFAULT browser.
      // No extra packages needed. Works with Edge, Chrome, Firefox — whatever they've set.
      return { cmd: '/mnt/c/Windows/explorer.exe', args: [url] };
    }
  } catch { /* not WSL */ }
  return { cmd: 'xdg-open', args: [url] };
}
import { SystemReadySignaler } from '../../scripts/signal-system-ready';
import { 
  SYSTEM_MILESTONES, 
  MILESTONE_DEPENDENCIES, 
  ENTRY_POINT_REQUIREMENTS,
  milestoneEmitter,
  SystemMilestone,
  EntryPointType,
  MilestoneEvent,
  MilestoneProgress
} from './SystemMilestones';

/**
 * Orchestration options for different entry points
 */
export interface OrchestrationOptions {
  verbose?: boolean;
  skipBrowser?: boolean;
  browserUrl?: string;
  testMode?: boolean;
  workingDir?: string;
  timeoutMs?: number;
}

/**
 * System state after orchestration
 */
export interface SystemState {
  readonly success: boolean;
  readonly completedMilestones: string[];
  readonly failedMilestone?: string;
  readonly error?: string;
  readonly serverProcess?: ChildProcess;
  readonly browserOpened: boolean;
}

/**
 * Universal System Orchestrator - Single Entry Point for All System Startup
 */
export class SystemOrchestrator extends EventEmitter {
  private signaler: SystemReadySignaler;
  private serverProcess: ChildProcess | null = null;
  private currentEntryPoint: string = 'unknown';

  // continuum#722 — Rust core supervisor state
  private coreProcess: ChildProcess | null = null;
  private coreShuttingDown = false;
  // Panic-loop detector: track restart timestamps within a rolling window.
  // If we see >5 restarts within 60s the binary is structurally broken
  // (e.g. missing dylib, port collision, model dir gone). Stop restarting
  // and surface the failure rather than burning CPU on a doomed loop.
  private coreRestartTimestamps: number[] = [];
  private static readonly CORE_RESTART_WINDOW_MS = 60_000;
  private static readonly CORE_RESTART_LIMIT = 5;
  private static readonly CORE_READY_TIMEOUT_MS = 30_000;
  private static readonly CORE_RESTART_BACKOFF_BASE_MS = 1_000;
  private static readonly CORE_RESTART_BACKOFF_MAX_MS = 30_000;

  // M5-QA Task 8 (live-observed 2026-05-01): if parallel-start.sh
  // (or a previous orchestrator, or a manual user spawn) put a
  // continuum-core-server up before our executeCoreStart ran, the
  // pre-existing socket-alive check makes us SKIP the spawn — which
  // means we have no this.coreProcess + no on('exit') handler. When
  // that core dies (SIGABRT on Mac Metal init = NEW-A), the supervisor
  // is blind to the death + doesn't respawn.
  //
  // Fix: when we skip the spawn, attach a PID-poll watcher. If the
  // adopted core dies, we spawn a managed replacement (which we DO
  // own via on('exit') for further restarts). After the first death-
  // detect, the watcher is no longer needed because the replacement
  // is in this.coreProcess.
  private adoptedCorePid: number | null = null;
  private adoptedCoreWatcher: ReturnType<typeof setInterval> | null = null;
  private static readonly ADOPTED_CORE_POLL_MS = 2_000;

  constructor() {
    super();
    this.signaler = new SystemReadySignaler();
    
    // Forward milestone events
    milestoneEmitter.on('milestone-completed', (event: MilestoneEvent) => {
      this.emit('milestone', event);
    });
    milestoneEmitter.on('milestone-failed', (event: MilestoneEvent) => {
      this.emit('milestone-failed', event);
    });
  }

  /**
   * Main orchestration entry point - handles all system startup scenarios
   */
  async orchestrate(entryPoint: EntryPointType, options: OrchestrationOptions = {}): Promise<SystemState> {
    this.currentEntryPoint = entryPoint;
    
    try {
      console.debug(`🎯 ORCHESTRATING: ${entryPoint}${options.testMode ? ' (TEST MODE)' : ''}`);
      
      // 1. Determine required milestones for this entry point
      const requiredMilestones = this.getRequiredMilestones(entryPoint);
      console.debug(`📋 Required milestones: ${requiredMilestones.join(' → ')}`);
      
      // 2. Set up working directory context
      await this.setupWorkingDirectory(options.workingDir);
      
      // 3. Check current system state
      const currentState = await this.getCurrentState();
      
      // 4. Calculate missing milestones in dependency order
      const missingMilestones = this.calculateMissingMilestones(requiredMilestones, currentState);
      
      if (missingMilestones.length === 0) {
        console.debug('✅ All required milestones already completed');
        
        // Special case: npm-test and npm-start should always ensure browser is opened
        // even if browser milestones are already completed
        if (entryPoint === 'npm-test' || entryPoint === 'npm-start') {
          console.debug('🔄 Entry point requires browser launch - ensuring browser is opened');
          await this.ensureBrowserOpened(options);
        }
        
        const finalState = {
          success: true,
          completedMilestones: requiredMilestones,
          browserOpened: requiredMilestones.includes(SYSTEM_MILESTONES.BROWSER_READY)
        };
        
        // TEST MODE: Generate signal and let caller handle exit
        if (options.testMode) {
          console.debug('🧪 Test mode - generating final system ready signal');
          await this.signaler.generateReadySignal();
        }
        
        return finalState;
      }
      
      console.debug(`🔄 Missing milestones: ${missingMilestones.join(' → ')}`);
      
      // 5. Execute milestones in proper dependency order
      for (const milestone of missingMilestones) {
        const success = await this.executeMilestone(milestone, options);
        if (!success) {
          return {
            success: false,
            completedMilestones: milestoneEmitter.getProgress(requiredMilestones).completedMilestones,
            failedMilestone: milestone,
            error: `Failed to complete milestone: ${milestone}`,
            browserOpened: false
          };
        }
      }
      
      // 6. Verify final system state
      const finalState = await this.verifySystemState(requiredMilestones);
      console.debug('🎉 Orchestration complete');
      
      // TEST MODE: Generate final signal after successful orchestration
      if (options.testMode) {
        console.debug('🧪 Test mode - generating final system ready signal');
        await this.signaler.generateReadySignal();
        console.debug('📡 Final system signal generated - ready for testing');
      }
      
      return finalState;
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Orchestration failed: ${errorMessage}`);
      
      await milestoneEmitter.failMilestone(
        'orchestration' as SystemMilestone, 
        entryPoint, 
        errorMessage
      );
      
      return {
        success: false,
        completedMilestones: [],
        error: errorMessage,
        browserOpened: false
      };
    }
  }

  /**
   * Get required milestones for an entry point
   */
  private getRequiredMilestones(entryPoint: EntryPointType): SystemMilestone[] {
    const requirements = ENTRY_POINT_REQUIREMENTS[entryPoint];
    if (!requirements) {
      console.warn(`⚠️ Unknown entry point: ${entryPoint}, using default requirements`);
      return [SYSTEM_MILESTONES.SERVER_READY];
    }
    // Spread to convert readonly array to mutable
    return [...requirements];
  }

  /**
   * Set up working directory context for per-project isolation
   */
  private async setupWorkingDirectory(workingDir?: string): Promise<void> {
    if (workingDir) {
      WorkingDirConfig.setWorkingDir(workingDir);
      console.debug(`📁 Working directory: ${workingDir}`);
    } else {
      // Use active example configuration
      try {
        const { getActiveExampleName } = await import('../../examples/server/ExampleConfigServer');
        const activeExample = getActiveExampleName();
        const defaultWorkingDir = `examples/${activeExample}`;
        WorkingDirConfig.setWorkingDir(defaultWorkingDir);
        console.debug(`📁 Working directory: ${defaultWorkingDir} (auto-detected)`);
      } catch (error) {
        console.warn('⚠️ Could not auto-detect working directory, using current');
      }
    }
  }

  /**
   * Check current system state to avoid redundant work
   */
  private async getCurrentState(): Promise<Set<string>> {
    const completedMilestones = new Set<string>();
    
    try {
      // First try the signaler for complete system readiness (increase timeout for CLI)
      const systemReady = await this.signaler.checkSystemReady(3000); // More generous timeout
      if (systemReady) {
        const milestonesToComplete: SystemMilestone[] = [SYSTEM_MILESTONES.SERVER_START, SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.SYSTEM_HEALTHY];

        // If browser is already ready according to signal, mark browser milestones as completed
        // but the browser launch execution will check if it actually needs to open a new tab
        if (systemReady.browserReady) {
          milestonesToComplete.push(
            SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED,
            SYSTEM_MILESTONES.BROWSER_READY
          );
        }
        
        // Inform milestone emitter about existing completed milestones
        for (const milestone of milestonesToComplete) {
          completedMilestones.add(milestone);
          await milestoneEmitter.completeMilestone(milestone, this.currentEntryPoint);
        }
        
        console.debug(`✅ System already ready (signal detected, browser: ${systemReady.browserReady ? 'ready' : 'not ready'})`);
        return completedMilestones;
      }
    } catch (error) {
      // Signaler failed, try direct port checks as fallback
    }
    
    // Fallback: Check if ports are in use (indicating servers are running)
    try {
      const { getActivePorts } = await import('../../examples/server/ExampleConfigServer');
      const activePorts = getActivePorts();
      
      const portChecks = await Promise.all([
        this.checkPortReady(activePorts.websocket_server),
        this.checkPortReady(activePorts.http_server)
      ]);
      
      if (portChecks.every(ready => ready)) {
        // Ports are active, do additional health check to confirm system is ready
        const healthCheck = await this.checkServerHealth(activePorts.http_server);
        if (healthCheck) {
          const milestonesToComplete = [SYSTEM_MILESTONES.SERVER_START, SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.SYSTEM_HEALTHY];
          
          // Inform milestone emitter about existing completed milestones
          for (const milestone of milestonesToComplete) {
            completedMilestones.add(milestone);
            await milestoneEmitter.completeMilestone(milestone, this.currentEntryPoint);
          }
          
          console.debug(`✅ Server already ready (ports active + health check passed: ${activePorts.websocket_server}, ${activePorts.http_server})`);
          return completedMilestones;
        } else {
          console.debug(`⚠️ Ports active but health check failed (${activePorts.websocket_server}, ${activePorts.http_server})`);
        }
      }
    } catch (error) {
      console.debug('🔄 Server needs to be started (port check failed)');
    }
    
    console.debug('🔄 Server needs to be started');
    return completedMilestones;
  }

  /**
   * Calculate missing milestones in proper dependency order
   */
  private calculateMissingMilestones(
    requiredMilestones: SystemMilestone[], 
    currentState: Set<string>
  ): SystemMilestone[] {
    const missing: SystemMilestone[] = [];
    const visited = new Set<string>();
    
    const addMissingWithDeps = (milestone: SystemMilestone) => {
      if (visited.has(milestone) || currentState.has(milestone)) {
        return;
      }
      
      visited.add(milestone);
      
      // Add dependencies first
      const deps = MILESTONE_DEPENDENCIES[milestone] || [];
      deps.forEach(dep => addMissingWithDeps(dep as SystemMilestone));
      
      // Add this milestone if not already completed
      if (!currentState.has(milestone) && !missing.includes(milestone)) {
        missing.push(milestone);
      }
    };
    
    requiredMilestones.forEach(milestone => addMissingWithDeps(milestone));
    return missing;
  }

  /**
   * Execute a specific milestone
   */
  private async executeMilestone(milestone: SystemMilestone, options: OrchestrationOptions): Promise<boolean> {
    console.debug(`🚀 Executing milestone: ${milestone}`);
    
    try {
      switch (milestone) {
        case SYSTEM_MILESTONES.BUILD_START:
          return await this.executeBuildStart();
          
        case SYSTEM_MILESTONES.BUILD_TYPESCRIPT_COMPLETE:
          return await this.executeBuildTypeScript();
          
        case SYSTEM_MILESTONES.BUILD_STRUCTURE_COMPLETE:
          return await this.executeBuildStructure();
          
        case SYSTEM_MILESTONES.BUILD_COMPLETE:
          return await this.executeBuildComplete();
          
        case SYSTEM_MILESTONES.DEPLOY_START:
          return await this.executeDeployStart();
          
        case SYSTEM_MILESTONES.DEPLOY_FILES_COMPLETE:
          return await this.executeDeployFiles();
          
        case SYSTEM_MILESTONES.DEPLOY_PORTS_ALLOCATED:
          return await this.executeDeployPorts();
          
        case SYSTEM_MILESTONES.DEPLOY_COMPLETE:
          return await this.executeDeployComplete();
          
        case SYSTEM_MILESTONES.CORE_START:
          return await this.executeCoreStart();

        case SYSTEM_MILESTONES.CORE_READY:
          return await this.executeCoreReady();

        case SYSTEM_MILESTONES.SERVER_START:
          return await this.executeServerStart();
          
        case SYSTEM_MILESTONES.SERVER_PROCESS_READY:
          return await this.executeServerProcess();
          
        case SYSTEM_MILESTONES.SERVER_WEBSOCKET_READY:
          return await this.executeServerWebSocket();
          
        case SYSTEM_MILESTONES.SERVER_HTTP_READY:
          return await this.executeServerHTTP();
          
        case SYSTEM_MILESTONES.SERVER_BOOTSTRAP_COMPLETE:
          return await this.executeServerBootstrap();
          
        case SYSTEM_MILESTONES.SERVER_COMMANDS_LOADED:
          return await this.executeServerCommands();
          
        case SYSTEM_MILESTONES.SERVER_READY:
          return await this.executeServerReady();
          
        case SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED:
          return await this.executeBrowserLaunch(options);
          
        case SYSTEM_MILESTONES.BROWSER_PROCESS_STARTED:
          return await this.executeBrowserProcess();
          
        case SYSTEM_MILESTONES.BROWSER_WEBSOCKET_CONNECTED:
          return await this.executeBrowserWebSocket();
          
        case SYSTEM_MILESTONES.BROWSER_INTERFACE_LOADED:
          return await this.executeBrowserInterface();
          
        case SYSTEM_MILESTONES.BROWSER_READY:
          return await this.executeBrowserReady();
          
        case SYSTEM_MILESTONES.SYSTEM_HEALTHY:
          return await this.executeSystemHealthy();
          
        case SYSTEM_MILESTONES.SYSTEM_READY:
          return await this.executeSystemReady();
          
        default:
          console.warn(`⚠️ Unknown milestone: ${milestone}`);
          return true; // Don't fail on unknown milestones
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Milestone ${milestone} failed: ${errorMessage}`);
      
      await milestoneEmitter.failMilestone(milestone, this.currentEntryPoint, errorMessage);
      return false;
    }
  }

  /**
   * BUILD MILESTONES
   */
  private async executeBuildStart(): Promise<boolean> {
    console.debug('🔨 Starting build process...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_START, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBuildTypeScript(): Promise<boolean> {
    console.debug('📝 Compiling TypeScript...');
    // TypeScript compilation would happen here
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_TYPESCRIPT_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBuildStructure(): Promise<boolean> {
    console.debug('🏗️ Building structure...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_STRUCTURE_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBuildComplete(): Promise<boolean> {
    console.debug('✅ Build complete');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * DEPLOY MILESTONES
   */
  private async executeDeployStart(): Promise<boolean> {
    console.debug('🚀 Starting deployment...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_START, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeDeployFiles(): Promise<boolean> {
    console.debug('📁 Deploying files...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_FILES_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeDeployPorts(): Promise<boolean> {
    console.debug('🔌 Allocating ports...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_PORTS_ALLOCATED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeDeployComplete(): Promise<boolean> {
    console.debug('✅ Deployment complete');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * RUST CORE MILESTONES (continuum#722)
   *
   * continuum-core-server is the Rust IPC backbone — Unix socket at
   * .continuum/sockets/continuum-core.sock, talked to by the data daemon
   * (ORMRustClient), AI provider daemon, code daemon, etc. Pre-fix the
   * binary was BUILT by parallel-start.sh:203 but never LAUNCHED — users
   * ended up with the all-widgets-blank-on-refresh symptom because every
   * IPC call returned "All IPC connections to continuum-core failed."
   *
   * The orchestrator now owns the core's lifecycle:
   *   - executeCoreStart spawns the binary (or yields if one is already
   *     running per pidfile / socket-existence — supports the "user
   *     manually launched it in another tab" case)
   *   - executeCoreReady waits for the socket to accept a TCP-equivalent
   *     connect (for Unix sockets, just connect() succeeds when the
   *     server is listen()ing) — gates SERVER_READY which the browser
   *     depends on
   *   - on('exit') handler restarts the binary with exponential backoff
   *     up to a panic-loop cap (5 restarts / 60s rolling window)
   *
   * Skip the spawn entirely when JTAG_SKIP_HTTP is set — that's the
   * Docker-mode signal (widget-server container handles HTTP, the
   * continuum-core container handles the Rust core, orchestrator does
   * neither).
   */
  private async executeCoreStart(): Promise<boolean> {
    if (process.env.JTAG_SKIP_HTTP) {
      console.debug('⏭️ Skipping core spawn (JTAG_SKIP_HTTP set — docker stack owns continuum-core-server)');
      await milestoneEmitter.completeMilestone(
        SYSTEM_MILESTONES.CORE_START,
        this.currentEntryPoint
      );
      return true;
    }

    // If a continuum-core-server is already running (user pre-launched it
    // in another tab, or a previous orchestrator left one, or
    // parallel-start.sh's Phase 3 spawn beat us to it), don't double-
    // spawn. Detect via socket existence + a connect-test.
    //
    // M5-QA T8 fix (2026-05-01): we ALSO need to attach a PID-poll
    // watcher on the inherited core so we still notice + respawn when
    // it dies. Pre-fix this branch just returned, which left no
    // on('exit') handler anywhere → SIGABRT in inherited core → no
    // respawn → user-visible "AI dead" with no recovery.
    const socketPath = await this.getCoreSocketPath();
    const corePath = await this.resolveCoreBinaryPath();

    if (await this.isCoreSocketAlive(socketPath)) {
      console.debug(`✅ continuum-core-server already running (socket ${socketPath} alive) — adopting via PID watcher`);
      if (corePath) {
        await this.adoptInheritedCore(corePath, socketPath);
      } else {
        console.warn('   ⚠ corePath not resolvable — adopted core won\'t be re-spawnable on death; will surface as orchestrator-blind crash');
      }
      await milestoneEmitter.completeMilestone(
        SYSTEM_MILESTONES.CORE_START,
        this.currentEntryPoint
      );
      return true;
    }

    if (!corePath) {
      console.error('❌ continuum-core-server binary not found — run npm start to build it (parallel-start.sh:203)');
      console.error('   Searched: src/workers/target/release/, workers/target/release/');
      await milestoneEmitter.failMilestone(
        SYSTEM_MILESTONES.CORE_START,
        this.currentEntryPoint,
        'continuum-core-server binary not found'
      );
      return false;
    }

    this.spawnCoreProcess(corePath, socketPath);

    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.CORE_START,
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * Adopt an externally-spawned continuum-core-server.
   *
   * Set up a PID-poll watcher (kill -0 every ADOPTED_CORE_POLL_MS) that
   * fires `spawnCoreProcess` when the adopted PID dies. Once we spawn
   * a replacement, that one is fully owned (this.coreProcess +
   * on('exit') handler from spawnCoreProcess), so subsequent restarts
   * use the normal supervisor path.
   *
   * If we can't find the PID via `pgrep`, log loudly + skip the watcher
   * — the inherited core will be invisible to supervision, but the rest
   * of the orchestrator's milestones still complete. Same intent as the
   * never-swallow-errors rule (CLAUDE.md): the gap is real + we surface
   * it rather than pretend everything's fine.
   */
  private async adoptInheritedCore(corePath: string, socketPath: string): Promise<void> {
    const pid = await this.findCoreProcessPid();
    if (pid <= 0) {
      console.warn('   ⚠ couldn\'t resolve adopted core PID via pgrep — supervisor will be blind to its death');
      return;
    }
    this.adoptedCorePid = pid;
    // Promoted debug → info: this is the supervisor's adoption signal +
    // critical to seeing in logs when later debugging "why didn't respawn fire?"
    // (#980 Bug 4 + the silent-success-is-failure rule applied to supervisor).
    console.info(`   adopted continuum-core-server PID ${pid}; watcher polling every ${SystemOrchestrator.ADOPTED_CORE_POLL_MS}ms`);

    this.adoptedCoreWatcher = setInterval(() => {
      if (this.coreShuttingDown) {
        return;
      }
      const adoptedPid = this.adoptedCorePid;
      if (adoptedPid === null) {
        return;
      }
      try {
        // kill -0: signal-0 only checks if PID exists + we have permission.
        // Throws ESRCH if dead, EPERM if alive-but-not-ours (we're the
        // user that started it via parallel-start.sh, so EPERM
        // shouldn't happen here — if it does, treat as not-ours +
        // stop watching).
        process.kill(adoptedPid, 0);
      } catch (err) {
        // PID is gone (or permission flipped). Stop watching, spawn a
        // managed replacement.
        const code = (err as NodeJS.ErrnoException).code;
        console.warn(`📋 adopted continuum-core-server PID ${adoptedPid} no longer alive (${code ?? 'unknown'}); spawning managed replacement`);
        this.stopAdoptedCoreWatcher();
        this.adoptedCorePid = null;
        this.spawnCoreProcess(corePath, socketPath);
      }
    }, SystemOrchestrator.ADOPTED_CORE_POLL_MS);
  }

  /**
   * Find the PID of the running continuum-core-server via `pgrep -x`.
   * Returns 0 if not found.
   */
  private async findCoreProcessPid(): Promise<number> {
    // Use pgrep -f (full command-line match) instead of -x (exact comm
    // match). On Linux `pgrep -x` checks /proc/PID/comm which is
    // truncated to 15 chars (TASK_COMM_LEN); the binary name
    // `continuum-core-server` is 22 chars → -x silently fails to match
    // on Linux even when the process is running. macOS pgrep doesn't
    // have this limit, but using -f works on both. Without this the
    // adopted-core PID watcher silently never installs on Linux →
    // supervisor blind to inherited-core death (#980 Bug 4 family).
    return new Promise<number>((resolve) => {
      const child = spawn('pgrep', ['-f', 'continuum-core-server'], {
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      let stdout = '';
      child.stdout.on('data', (chunk: Buffer) => { stdout += chunk.toString('utf8'); });
      child.on('error', () => resolve(0));
      child.on('close', () => {
        // pgrep -f also matches the orchestrator's own pgrep invocation
        // (briefly) + any tail/grep on the log. Filter to PIDs where the
        // process name is exactly continuum-core-server using a second pass.
        const candidates = stdout.trim().split('\n')
          .map(line => Number.parseInt(line, 10))
          .filter(n => Number.isFinite(n) && n > 0);
        if (candidates.length === 0) { resolve(0); return; }
        // Cross-check via ps to find the candidate whose argv[0] basename is the binary.
        // Best-effort — if ps fails, fall back to first candidate.
        const ps = spawn('ps', ['-o', 'pid=,comm=', ...candidates.flatMap(p => ['-p', String(p)])], {
          stdio: ['ignore', 'pipe', 'pipe'],
        });
        let psOut = '';
        ps.stdout.on('data', (c: Buffer) => { psOut += c.toString('utf8'); });
        ps.on('error', () => resolve(candidates[0] ?? 0));
        ps.on('close', () => {
          for (const line of psOut.trim().split('\n')) {
            const m = line.trim().match(/^(\d+)\s+(.+)$/);
            if (m && (m[2].endsWith('continuum-core-server') || m[2].includes('continuum-core'))) {
              resolve(Number.parseInt(m[1], 10));
              return;
            }
          }
          resolve(candidates[0] ?? 0);
        });
      });
    });
  }

  /**
   * Stop the adopted-core PID watcher (interval timer). Idempotent.
   */
  private stopAdoptedCoreWatcher(): void {
    if (this.adoptedCoreWatcher !== null) {
      clearInterval(this.adoptedCoreWatcher);
      this.adoptedCoreWatcher = null;
    }
  }

  private async executeCoreReady(): Promise<boolean> {
    if (process.env.JTAG_SKIP_HTTP) {
      console.debug('⏭️ Skipping core readiness gate (JTAG_SKIP_HTTP — docker stack health-checks separately)');
      await milestoneEmitter.completeMilestone(
        SYSTEM_MILESTONES.CORE_READY,
        this.currentEntryPoint
      );
      return true;
    }

    const socketPath = await this.getCoreSocketPath();
    const deadline = Date.now() + SystemOrchestrator.CORE_READY_TIMEOUT_MS;
    const pollMs = 200;

    console.debug(`⏳ Waiting for continuum-core-server to accept connections (socket ${socketPath})...`);

    while (Date.now() < deadline) {
      if (await this.isCoreSocketAlive(socketPath)) {
        const elapsedMs = SystemOrchestrator.CORE_READY_TIMEOUT_MS - (deadline - Date.now());
        console.debug(`✅ continuum-core-server ready (${elapsedMs}ms)`);
        await milestoneEmitter.completeMilestone(
          SYSTEM_MILESTONES.CORE_READY,
          this.currentEntryPoint
        );
        return true;
      }
      // Cheap exit check — if the spawn errored synchronously, don't burn 30s.
      if (this.coreProcess && this.coreProcess.exitCode !== null) {
        console.error(`❌ continuum-core-server exited code=${this.coreProcess.exitCode} during startup`);
        await milestoneEmitter.failMilestone(
          SYSTEM_MILESTONES.CORE_READY,
          this.currentEntryPoint,
          `continuum-core-server exited code=${this.coreProcess.exitCode} before becoming ready`
        );
        return false;
      }
      await new Promise(r => setTimeout(r, pollMs));
    }

    console.error(`❌ continuum-core-server did not become ready within ${SystemOrchestrator.CORE_READY_TIMEOUT_MS}ms`);
    await milestoneEmitter.failMilestone(
      SYSTEM_MILESTONES.CORE_READY,
      this.currentEntryPoint,
      `continuum-core-server readiness timeout (${SystemOrchestrator.CORE_READY_TIMEOUT_MS}ms)`
    );
    return false;
  }

  /**
   * Resolve the absolute path of the continuum-core-server binary.
   * Candidates ordered by likelihood given typical CWD on `npm start`:
   *   1. <repoRoot>/src/workers/target/release/continuum-core-server
   *   2. <repoRoot>/workers/target/release/continuum-core-server
   *   3. <repoRoot>/src/workers/target/debug/continuum-core-server  (dev fallback)
   */
  private async resolveCoreBinaryPath(): Promise<string | null> {
    const repoRoot = await this.findRepoRoot();
    const candidates = [
      path.join(repoRoot, 'src/workers/target/release/continuum-core-server'),
      path.join(repoRoot, 'workers/target/release/continuum-core-server'),
      path.join(repoRoot, 'src/workers/target/debug/continuum-core-server'),
    ];
    for (const candidate of candidates) {
      if (existsSync(candidate)) return candidate;
    }
    return null;
  }

  /**
   * Find repo root by walking up from CWD looking for a marker (package.json
   * with the right name, or .git directory). Falls back to CWD if nothing found.
   */
  private async findRepoRoot(): Promise<string> {
    let dir = process.cwd();
    const root = path.parse(dir).root;
    while (dir !== root) {
      if (existsSync(path.join(dir, '.git'))) return dir;
      const pkgPath = path.join(dir, 'package.json');
      if (existsSync(pkgPath)) {
        try {
          const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'));
          if (pkg.name === 'continuum' || pkg.name === '@continuum/root') return dir;
        } catch { /* ignore parse errors */ }
      }
      dir = path.dirname(dir);
    }
    return process.cwd();
  }

  /**
   * Get the canonical Unix socket path for continuum-core-server.
   * Mirror of the bindings' getContinuumCoreSocketPath() to avoid pulling
   * in the entire bindings module here (which has its own initialization
   * order concerns).
   */
  private async getCoreSocketPath(): Promise<string> {
    const repoRoot = await this.findRepoRoot();
    return path.join(repoRoot, '.continuum/sockets/continuum-core.sock');
  }

  /**
   * Probe a Unix socket for liveness. Returns true if connect() succeeds
   * AND the socket exists as a file (kernel has bound it for accept()).
   *
   * Why both checks: the file can exist as a stale socket file from a
   * crashed previous process. connect() will fail in that case (ECONNREFUSED)
   * — that's the discriminator. We treat any connect error as "not alive."
   */
  private async isCoreSocketAlive(socketPath: string): Promise<boolean> {
    try {
      const stats = await stat(socketPath);
      if (!stats.isSocket()) return false;
    } catch {
      return false;
    }
    return new Promise<boolean>((resolve) => {
      const sock = net.createConnection(socketPath);
      const cleanup = () => {
        try { sock.destroy(); } catch { /* ignore */ }
      };
      const timer = setTimeout(() => { cleanup(); resolve(false); }, 1000);
      sock.once('connect', () => { clearTimeout(timer); cleanup(); resolve(true); });
      sock.once('error', () => { clearTimeout(timer); cleanup(); resolve(false); });
    });
  }

  /**
   * Spawn continuum-core-server with lifecycle handlers. The on('exit')
   * handler restarts the process unless we're shutting down OR the panic-
   * loop detector trips.
   */
  private spawnCoreProcess(corePath: string, socketPath: string): void {
    console.debug(`🦀 Spawning continuum-core-server: ${corePath} ${socketPath}`);

    const childCwd = path.dirname(path.dirname(path.dirname(corePath))); // workers/target/release → workers
    this.coreProcess = spawn(corePath, [socketPath], {
      cwd: childCwd,
      stdio: ['ignore', 'pipe', 'pipe'],
      // Detached false: tie lifecycle to orchestrator; if orchestrator dies,
      // node sends SIGTERM to the group on cleanup. Detached true would
      // orphan the core to launchd reaping which we don't want here.
      detached: false,
      env: { ...process.env },
    });

    this.coreProcess.stdout?.on('data', (data) => {
      // Filter to debug — core writes a LOT to stdout in dev. Aggregating
      // it here keeps it findable while not dominating the orchestrator log.
      console.debug(`[core] ${data.toString().trimEnd()}`);
    });
    this.coreProcess.stderr?.on('data', (data) => {
      console.error(`[core:err] ${data.toString().trimEnd()}`);
    });

    this.coreProcess.on('error', (err) => {
      console.error(`❌ continuum-core-server spawn error: ${err.message}`);
    });

    this.coreProcess.on('exit', (code, signal) => {
      const ts = Date.now();
      // Promoted from debug → info so the supervisor's lifecycle is
      // visible in default logs. Carl's #980 Bug 4 reported "no respawn"
      // partly because the respawn-related debug logs weren't visible —
      // can't diagnose what didn't happen if the logs hide what did.
      console.info(`📋 continuum-core-server exited: code=${code} signal=${signal}`);
      this.coreProcess = null;

      if (this.coreShuttingDown) {
        console.info('   (orchestrator shutting down — not restarting)');
        return;
      }

      // Panic-loop detection: prune timestamps outside the rolling window,
      // then check the rate.
      const cutoff = ts - SystemOrchestrator.CORE_RESTART_WINDOW_MS;
      this.coreRestartTimestamps = this.coreRestartTimestamps.filter(t => t >= cutoff);
      this.coreRestartTimestamps.push(ts);

      if (this.coreRestartTimestamps.length > SystemOrchestrator.CORE_RESTART_LIMIT) {
        console.error(
          `❌ continuum-core-server panic-loop: ${this.coreRestartTimestamps.length} restarts in ` +
          `${SystemOrchestrator.CORE_RESTART_WINDOW_MS / 1000}s — STOPPING auto-restart.`
        );
        console.error('   The binary is structurally broken (missing dylib, port collision, model dir gone, etc).');
        console.error('   Inspect the core stderr above + restart orchestrator after fixing.');
        return;
      }

      // Exponential backoff: 1s, 2s, 4s, 8s, 16s, capped at 30s.
      const attemptIdx = this.coreRestartTimestamps.length - 1;
      const delay = Math.min(
        SystemOrchestrator.CORE_RESTART_BACKOFF_BASE_MS * Math.pow(2, attemptIdx),
        SystemOrchestrator.CORE_RESTART_BACKOFF_MAX_MS
      );
      console.info(`🔁 Restarting continuum-core-server in ${delay}ms (attempt ${this.coreRestartTimestamps.length})`);
      setTimeout(() => {
        if (!this.coreShuttingDown) {
          console.info(`🔁 Spawning continuum-core-server now (restart attempt ${this.coreRestartTimestamps.length})`);
          this.spawnCoreProcess(corePath, socketPath);
        }
      }, delay);
    });
  }

  /**
   * SERVER MILESTONES
   */
  private async executeServerStart(): Promise<boolean> {
    console.debug('🔌 Starting server process...');

    // Clear any existing signals
    await this.signaler.clearSignals();

    // Start the server using the existing launch-active-example script
    // but WITHOUT the premature browser opening
    const { getActivePorts } = await import('../../examples/server/ExampleConfigServer');
    const activePorts = await getActivePorts();

    // Import and start the JTAG system server
    const { JTAGSystemServer } = await import('../core/system/server/JTAGSystemServer');
    const jtagServer = await JTAGSystemServer.connect();
    console.debug(`✅ JTAG WebSocket Server running on port ${activePorts.websocket_server}`);

    // Start the example HTTP server DIRECTLY (no npm-within-npm nesting).
    // Two separate servers:
    //   1. JTAGSystemServer (WebSocket + daemons) - core backend
    //   2. minimal-server.ts (HTTP) - serves UI and static files
    //
    // In Docker, the widget-server container handles HTTP separately,
    // so skip spawning the HTTP server when JTAG_SKIP_HTTP is set.
    if (!process.env.JTAG_SKIP_HTTP) {
      const { getActiveExamplePath } = await import('../../examples/server/ExampleConfigServer');
      const activeExamplePath = getActiveExamplePath();
      const serverScript = `${activeExamplePath}/src/minimal-server.ts`;

      console.debug(`🎯 Starting HTTP server directly: ${serverScript}`);

      this.serverProcess = spawn('npx', ['tsx', serverScript], {
        cwd: activeExamplePath,
        stdio: ['ignore', 'pipe', 'pipe'],
        shell: false
      });

      this.serverProcess.stdout?.on('data', (data) => {
        console.debug(`📄 HTTP Server: ${data.toString().trim()}`);
      });

      this.serverProcess.stderr?.on('data', (data) => {
        console.debug(`⚠️ HTTP Server Error: ${data.toString().trim()}`);
      });

      this.serverProcess.on('error', (error) => {
        console.error(`❌ Server process failed: ${error.message}`);
      });

      this.serverProcess.on('exit', (code, signal) => {
        console.debug(`📋 HTTP Server process exited: code=${code}, signal=${signal}`);
      });
    } else {
      console.debug(`⏭️ Skipping HTTP server (JTAG_SKIP_HTTP set — widget-server handles HTTP)`);
    }

    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_START,
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerProcess(): Promise<boolean> {
    console.debug('🔄 Server process ready...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_PROCESS_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerWebSocket(): Promise<boolean> {
    console.debug('🔌 WebSocket server ready...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_WEBSOCKET_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerHTTP(): Promise<boolean> {
    console.debug('🌐 HTTP server ready...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_HTTP_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerBootstrap(): Promise<boolean> {
    console.debug('⚡ Server bootstrap complete...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_BOOTSTRAP_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerCommands(): Promise<boolean> {
    console.debug('📋 Server commands loaded...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_COMMANDS_LOADED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerReady(): Promise<boolean> {
    console.debug('⏳ Waiting for server to be ready...');

    const { getActivePorts } = await import('../../examples/server/ExampleConfigServer');
    const activePorts = await getActivePorts();

    // Phase 1: Wait for ports to be listening (fast — usually 1-3 seconds)
    const maxPortRetries = 30;
    let attempt = 0;

    while (attempt < maxPortRetries) {
      try {
        const checks = [this.checkPortReady(activePorts.websocket_server)];
        if (!process.env.JTAG_SKIP_HTTP) {
          checks.push(this.checkPortReady(activePorts.http_server));
        }
        const portChecks = await Promise.all(checks);

        if (portChecks.every(ready => ready)) {
          console.debug(`✅ Ports listening: WS=${activePorts.websocket_server}${process.env.JTAG_SKIP_HTTP ? ' (HTTP skipped)' : `, HTTP=${activePorts.http_server}`}`);
          break;
        }
      } catch {
        // Continue waiting
      }

      attempt++;
      await new Promise(resolve => setTimeout(resolve, 1000));
    }

    if (attempt >= maxPortRetries) {
      throw new Error(`Server ports not listening within ${maxPortRetries} seconds`);
    }

    // Phase 2: Wait for server to finish bootstrapping (commands registered).
    // This prevents the browser from opening to a white screen while the
    // WebSocket server is still registering commands and daemons.
    //
    // Check readiness DIRECTLY via the server instance — no subprocess.
    // The old approach (./jtag ping) spawned a CLI that connected via WebSocket
    // back to itself, which is circular in Docker and fragile everywhere.
    console.debug('⏳ Waiting for server bootstrap (commands + daemons)...');
    const maxBootstrapRetries = 30;
    let bootstrapAttempt = 0;
    const { JTAGSystemServer: ServerClass } = await import('../core/system/server/JTAGSystemServer');

    while (bootstrapAttempt < maxBootstrapRetries) {
      try {
        const server = ServerClass.instance;
        if (server) {
          const commandDaemon = server.getCommandDaemon() as any;
          const cmds = commandDaemon?.commands?.size ?? 0;
          const daemons = server.systemDaemons.length;
          if (cmds > 0) {
            console.debug(`✅ Server bootstrapped: ${cmds} commands, ${daemons} daemons`);
            break;
          }
        }
      } catch {
        // Server not ready yet
      }

      bootstrapAttempt++;
      await new Promise(resolve => setTimeout(resolve, 1000));
    }

    if (bootstrapAttempt >= maxBootstrapRetries) {
      console.warn('⚠️ Server bootstrap timeout — proceeding anyway (ports are listening)');
    }

    console.debug('✅ Server is ready');

    // Auto-seed database if empty (first run or after data:clear).
    // This is part of readiness, not background maintenance: chat/send,
    // room routing, persona allocation, and Carl's first-page experience all
    // require seeded rooms/users to exist. Fire-and-forget seeding let
    // widget-server become healthy while #general was still missing.
    try {
      const { seedDatabase } = await import('../../server/seed-in-process');
      const seeded = await seedDatabase();
      console.log(seeded ? '✅ Database seeded (in-process)' : '✅ Database already seeded');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      throw new Error(`Auto-seed failed before server readiness: ${msg}`);
    }

    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_READY,
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * Clean modular port checking with strong types
   */
  private async checkPortReady(port: number): Promise<boolean> {
    const { PortChecker, PortCheckResult } = await import('../core/ports/PortChecker');
    const checker = new PortChecker();
    
    const status = await checker.checkPortNumber(port);
    
    if (status.isActive && status.result === PortCheckResult.ACTIVE) {
      console.debug(`✅ Port ${port} is active (${status.method})`);
    } else if (status.result === PortCheckResult.ERROR) {
      console.debug(`⚠️ Port ${port} check error (${status.method}): ${status.error}`);
    } else {
      console.debug(`⚠️ Port ${port} not active (${status.method})`);
    }
    
    return status.isActive && status.result === PortCheckResult.ACTIVE;
  }

  /**
   * Signal-based server health check (replaces HTTP polling with timeouts)
   * 
   * TIMEOUT ELIMINATION: This replaces the 2-second HTTP timeout polling
   * with comprehensive signal-based health detection from the signal system.
   */
  private async checkServerHealth(port: number): Promise<boolean> {
    try {
      // Use the existing signal system for comprehensive health checking
      const { SystemReadySignaler } = await import('../../scripts/signaling/server/SystemReadySignaler');
      const signaler = new SystemReadySignaler();
      
      // Fast event-driven health check (500ms max)
      const signal = await signaler.checkSystemReady(500);
      
      if (signal) {
        // Use comprehensive signal-based health instead of primitive HTTP polling
        const isHealthy = signal.systemHealth === 'healthy';
        const hasPort = signal.portsActive?.includes(port) || false;
        const hasCommands = signal.commandCount > 0;
        const browserReady = signal.browserReady;
        
        if (isHealthy && hasPort && hasCommands) {
          console.debug(`✅ Server health confirmed via signal: ${signal.commandCount} commands, browser: ${browserReady} (no HTTP polling)`);
          return true;
        } else {
          console.debug(`⚠️ Server health degraded - health: ${signal.systemHealth}, port: ${hasPort}, commands: ${signal.commandCount}`);
          return false;
        }
      }
      
      // No signal found - server is not healthy
      console.debug(`⚠️ Server health check failed for port ${port} - no signal detected`);
      return false;
      
    } catch (error) {
      console.debug(`⚠️ Signal-based health check failed for port ${port}: ${error}`);
      return false;
    }
  }

  /**
   * BROWSER MILESTONES - CRITICAL: Only after server ready
   */
  private async executeBrowserLaunch(options: OrchestrationOptions): Promise<boolean> {
    if (options.skipBrowser) {
      console.debug('⏭️ Skipping browser launch (skipBrowser option)');
      await milestoneEmitter.completeMilestone(
        SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED,
        this.currentEntryPoint
      );
      return true;
    }

    await this.detectAndManageBrowser(options);

    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED,
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * Single source of truth for browser detection and management.
   *
   * Flow:
   * 1. Ping server to check if a browser is already connected
   * 2. If connected → refresh it (interface/navigate, fallback to location.reload())
   * 3. If not connected → open a new tab
   *
   * Called from:
   * - executeBrowserLaunch() during fresh startup milestone chain
   * - ensureBrowserOpened() when all milestones already complete
   */
  /**
   * Ping the server and check if a browser is connected.
   * Returns true if browser is detected, false otherwise.
   */
  private async pingForBrowser(): Promise<boolean> {
    try {
      const { stdout } = await execAsync('./jtag ping', { timeout: 5000 });
      const pingResponse = JSON.parse(stdout);
      return !!(pingResponse.success && pingResponse.browser);
    } catch {
      return false;
    }
  }

  private async detectAndManageBrowser(options: OrchestrationOptions): Promise<void> {
    // Step 1: Check if browser is already connected.
    // After a server restart, existing browser tabs need a few seconds to
    // reconnect their WebSocket. Retry ping up to 3 times with delays
    // before concluding no browser is present.
    let browserConnected = await this.pingForBrowser();

    if (!browserConnected) {
      // Wait and retry — the browser tab may be reconnecting after restart
      for (let attempt = 1; attempt <= 2; attempt++) {
        console.log(`🔍 No browser on attempt ${attempt} — waiting 3s for reconnect...`);
        await new Promise(resolve => setTimeout(resolve, 3000));
        browserConnected = await this.pingForBrowser();
        if (browserConnected) break;
      }
    }

    // Step 2a: Browser found — refresh it
    if (browserConnected) {
      console.log('🔄 Browser connected — refreshing to pick up new code');
      try {
        await execAsync('./jtag interface/navigate', { timeout: 5000 });
        console.log('✅ Browser refreshed');
      } catch {
        console.warn('⚠️ interface/navigate failed, trying location.reload()');
        try {
          await execAsync('./jtag development/exec --code="location.reload()"', { timeout: 5000 });
          console.log('✅ Browser reloaded via exec');
        } catch {
          console.warn('⚠️ Browser reload also failed');
        }
      }
      console.log('✅ Browser already connected — no new tab needed');
      return;
    }

    // Step 2b: No browser detected after retries — open new tab
    console.log('🌐 No browser detected — opening new tab');
    const browserUrl = options.browserUrl || await this.getDefaultBrowserUrl();

    try {
      const { cmd, args } = openBrowserCommand(browserUrl);
      spawn(cmd, args, {
        detached: true,
        stdio: 'ignore'
      }).unref();
      console.log(`✅ Browser launched: ${browserUrl}`);
    } catch (error) {
      console.warn(`⚠️ Failed to auto-open browser: ${error}`);
    }
  }

  private async executeBrowserProcess(): Promise<boolean> {
    console.debug('🌐 Browser process started...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_PROCESS_STARTED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBrowserWebSocket(): Promise<boolean> {
    console.debug('🔗 Browser WebSocket connected...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_WEBSOCKET_CONNECTED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBrowserInterface(): Promise<boolean> {
    console.debug('🖥️ Browser interface loaded...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_INTERFACE_LOADED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBrowserReady(): Promise<boolean> {
    console.debug('⏳ Waiting for browser to be ready...');
    
    // For now, assume browser is ready after launch
    // Future: implement browser readiness detection via WebSocket
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    console.debug('✅ Browser is ready');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * SYSTEM MILESTONES
   */
  private async executeSystemHealthy(): Promise<boolean> {
    console.debug('💚 System is healthy...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SYSTEM_HEALTHY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeSystemReady(): Promise<boolean> {
    console.debug('🎉 System is fully ready');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SYSTEM_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * Verify final system state
   */
  private async verifySystemState(requiredMilestones: SystemMilestone[]): Promise<SystemState> {
    const progress = milestoneEmitter.getProgress(requiredMilestones);
    const allCompleted = progress.completed === progress.total;
    
    if (!allCompleted) {
      const missingMilestones = requiredMilestones.filter(m => !progress.completedMilestones.includes(m));
      const errorMessage = `Final verification failed. Missing milestones: ${missingMilestones.join(', ')}`;
      console.error(`❌ ${errorMessage}`);
      
      return {
        success: false,
        completedMilestones: progress.completedMilestones,
        failedMilestone: progress.current || missingMilestones[0],
        error: errorMessage,
        browserOpened: progress.completedMilestones.includes(SYSTEM_MILESTONES.BROWSER_READY),
        serverProcess: this.serverProcess ?? undefined
      };
    }

    return {
      success: true,
      completedMilestones: progress.completedMilestones,
      failedMilestone: undefined,
      error: undefined,
      browserOpened: progress.completedMilestones.includes(SYSTEM_MILESTONES.BROWSER_READY),
      serverProcess: this.serverProcess ?? undefined
    };
  }

  /**
   * Ensure browser is opened for entry points that require browser interaction.
   * Delegates to detectAndManageBrowser() — single source of truth for browser detection.
   */
  private async ensureBrowserOpened(options: OrchestrationOptions): Promise<void> {
    if (options.skipBrowser) {
      console.debug('⏭️ Skipping browser launch (skipBrowser option)');
      return;
    }

    await this.detectAndManageBrowser(options);
  }

  /**
   * Get default browser URL based on configuration
   */
  private async getDefaultBrowserUrl(): Promise<string> {
    try {
      const { getActivePorts } = require('../../examples/server/ExampleConfigServer');
      const activePorts = await getActivePorts();
      const { getServiceUrl } = require('../config/server/NetworkIdentity');
      return getServiceUrl(activePorts.http_server);
    } catch (error) {
      console.error('❌ FATAL: Could not get active ports - no fallback:', error);
      throw error;
    }
  }

  /**
   * Get milestone progress for monitoring
   */
  getProgress(entryPoint: EntryPointType): MilestoneProgress {
    const requiredMilestones = this.getRequiredMilestones(entryPoint);
    return milestoneEmitter.getProgress(requiredMilestones);
  }

  /**
   * Cleanup resources — sets shutdown flag FIRST so the core's
   * on('exit') handler doesn't restart the process during teardown.
   */
  async cleanup(): Promise<void> {
    // Set shutdown flag before killing — without this the on('exit')
    // handler would interpret the SIGTERM as a crash and respawn (#722
    // panic-loop self-inflicted). The same flag stops the adopted-core
    // PID watcher from re-spawning during shutdown.
    this.coreShuttingDown = true;

    // Stop the adopted-core PID watcher first (M5-QA T8 path); it
    // doesn't own a process, just an interval timer.
    this.stopAdoptedCoreWatcher();
    this.adoptedCorePid = null;

    if (this.coreProcess) {
      console.debug('🛑 Cleaning up continuum-core-server process...');
      try { this.coreProcess.kill('SIGTERM'); } catch { /* already dead */ }
      this.coreProcess = null;
    }

    if (this.serverProcess) {
      console.debug('🛑 Cleaning up server process...');
      this.serverProcess.kill('SIGTERM');
      this.serverProcess = null;
    }
  }
}

/**
 * Global orchestrator instance
 */
export const systemOrchestrator = new SystemOrchestrator();
