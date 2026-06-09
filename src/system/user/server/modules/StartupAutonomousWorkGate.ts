import fs from 'fs';
import path from 'path';
import { SystemPaths } from '../../../core/config/SystemPaths';

const DEFAULT_PAUSE_FILE = path.join(SystemPaths.root, 'jtag', 'startup-autonomous-work.paused');
const DEFAULT_MAX_WAIT_MS = 10 * 60 * 1000;
const DEFAULT_POLL_MS = 1000;

export class StartupAutonomousWorkGate {
  static get pauseFile(): string {
    return process.env.CONTINUUM_STARTUP_AUTONOMOUS_PAUSE_FILE || DEFAULT_PAUSE_FILE;
  }

  static isPaused(): boolean {
    if (process.env.CONTINUUM_AUTONOMOUS_WORK_PAUSED === '1' || process.env.CONTINUUM_AUTONOMOUS_WORK_PAUSED === 'true') {
      return true;
    }

    const pauseFile = this.pauseFile;
    if (!fs.existsSync(pauseFile)) {
      return false;
    }

    const ownerPid = this.readOwnerPid(pauseFile);
    if (ownerPid !== null && !this.isProcessAlive(ownerPid)) {
      fs.rmSync(pauseFile, { force: true });
      return false;
    }

    return true;
  }

  static async waitUntilOpen(
    log?: (message: string) => void,
    label: string = 'autonomous work',
    options: { maxWaitMs?: number; pollMs?: number } = {}
  ): Promise<void> {
    if (!this.isPaused()) return;

    const maxWaitMs = options.maxWaitMs ?? DEFAULT_MAX_WAIT_MS;
    const pollMs = options.pollMs ?? DEFAULT_POLL_MS;
    const startedAt = Date.now();
    log?.(`⏸️ Startup gate closed — deferring ${label} until seed completes`);
    while (this.isPaused()) {
      if (Date.now() - startedAt >= maxWaitMs) {
        log?.(`⚠️ Startup gate still closed after ${Math.round(maxWaitMs / 1000)}s — failing open for ${label}`);
        return;
      }
      await new Promise(resolve => setTimeout(resolve, pollMs));
    }
    log?.(`▶️ Startup gate open — resuming ${label}`);
  }

  private static readOwnerPid(pauseFile: string): number | null {
    try {
      const raw = fs.readFileSync(pauseFile, 'utf8').trim();
      if (!/^\d+$/.test(raw)) {
        return null;
      }
      return Number(raw);
    } catch {
      return null;
    }
  }

  private static isProcessAlive(pid: number): boolean {
    if (!Number.isSafeInteger(pid) || pid <= 0) {
      return false;
    }
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  }
}
