import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { StartupAutonomousWorkGate } from '../../system/user/server/modules/StartupAutonomousWorkGate';

const originalPauseFile = process.env.CONTINUUM_STARTUP_AUTONOMOUS_PAUSE_FILE;
const originalEnvPause = process.env.CONTINUUM_AUTONOMOUS_WORK_PAUSED;

afterEach(() => {
  if (originalPauseFile === undefined) {
    delete process.env.CONTINUUM_STARTUP_AUTONOMOUS_PAUSE_FILE;
  } else {
    process.env.CONTINUUM_STARTUP_AUTONOMOUS_PAUSE_FILE = originalPauseFile;
  }

  if (originalEnvPause === undefined) {
    delete process.env.CONTINUUM_AUTONOMOUS_WORK_PAUSED;
  } else {
    process.env.CONTINUUM_AUTONOMOUS_WORK_PAUSED = originalEnvPause;
  }
});

describe('StartupAutonomousWorkGate', () => {
  it('removes stale owner-pid pause files instead of blocking forever', () => {
    const dir = mkdtempSync(join(tmpdir(), 'continuum-startup-gate-'));
    const pauseFile = join(dir, 'startup-autonomous-work.paused');
    process.env.CONTINUUM_STARTUP_AUTONOMOUS_PAUSE_FILE = pauseFile;
    writeFileSync(pauseFile, '999999999');

    expect(StartupAutonomousWorkGate.isPaused()).toBe(false);

    rmSync(dir, { recursive: true, force: true });
  });

  it('fails open after max wait when an explicit env pause is left set', async () => {
    const messages: string[] = [];
    process.env.CONTINUUM_AUTONOMOUS_WORK_PAUSED = '1';

    await StartupAutonomousWorkGate.waitUntilOpen(
      message => messages.push(message),
      'unit test',
      { maxWaitMs: 5, pollMs: 1 }
    );

    expect(messages.some(message => message.includes('failing open'))).toBe(true);
  });
});
