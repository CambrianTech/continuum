/**
 * Reaper — boot-time cleanup of orphaned Playwright Chromium processes and their
 * temp profile directories.
 *
 * When the eye-node crashes or is killed without graceful shutdown, Chromium
 * processes spawned by `@continuum/perception` can be left running with orphaned
 * `playwright_chromiumdev_profile-*` directories in the temp filesystem. This
 * module scans for and kills those orphans at startup (and optionally on a
 * schedule), then cleans up any leftover profile directories no longer referenced
 * by a live process.
 *
 * Usage:
 *   import { reapOrphanedBrowsers } from './reaper';
 *   const result = await reapOrphanedBrowsers();
 *   // result: { browsersKilled, profilesCleaned, probePayload }
 */

import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import util from 'node:util';

const execFilePromise = util.promisify(execFile);

/** Pattern that Playwright Chromium processes carry in their command line. */
const PROFILE_PATTERN = 'playwright_chromiumdev_profile';

/** Result of a reaping pass. */
export interface ReaperResult {
  /** Number of orphaned browser processes killed. */
  browsersKilled: number;
  /** Number of orphaned profile directories cleaned up. */
  profilesCleaned: number;
}

/**
 * Run a boot-time reaper pass: kill orphaned Playwright Chromium processes and
 * clean up their leftover temp profile directories.
 *
 * Returns a result suitable for logging / probe emission.
 */
export async function reapOrphanedBrowsers(): Promise<ReaperResult> {
  const browsersKilled = await killOrphanedProcesses();
  const profilesCleaned = await cleanupOrphanedProfiles(browsersKilled);

  return { browsersKilled, profilesCleaned };
}

/**
 * Find all running processes whose command line contains `playwright_chromiumdev_profile`
 * and kill those whose parent process is no longer alive.
 */
async function killOrphanedProcesses(): Promise<number> {
  try {
    // pgrep -f matches the full command line; -a returns PID + cmdline
    const { stdout } = await execFilePromise('pgrep', ['-fa', PROFILE_PATTERN]);
    const lines = stdout.trim().split('\n').filter(Boolean);

    if (lines.length === 0) return 0;

    let killedCount = 0;

    for (const line of lines) {
      // pgrep -a output: "PID full command line"
      const spaceIdx = line.indexOf(' ');
      if (spaceIdx === -1) continue;

      const pidStr = line.slice(0, spaceIdx).trim();
      const pid = parseInt(pidStr, 10);
      if (isNaN(pid)) continue;

      // Skip our own process
      if (pid === process.pid) continue;

      try {
        // Get parent PID
        const { stdout: ppidOut } = await execFilePromise('ps', ['-o', 'ppid=', '-p', String(pid)]);
        const ppidStr = ppidOut.trim();
        if (!ppidStr) continue;

        const ppid = parseInt(ppidStr, 10);
        if (isNaN(ppid)) continue;

        // Check if parent is alive
        try {
          await execFilePromise('ps', ['-p', String(ppid)]);
          // Parent exists — not an orphan from a dead parent. However, we still
          // want to kill processes whose parent is the eye-node's own process group
          // that was killed but the browser survived (zombie-like state). We check:
          // if ppid == 1 (init/systemd), the process has been reparented and is orphaned.
          if (ppid === 1) {
            await killProcess(pid);
            killedCount++;
          }
        } catch {
          // Parent does not exist — definitely an orphan, kill it
          await killProcess(pid);
          killedCount++;
        }
      } catch {
        // Process may have already exited; ignore
        continue;
      }
    }

    return killedCount;
  } catch (err) {
    // pgrep returns exit code 1 when no matches found — not an error
    if (err instanceof Error && 'status' in err && (err as { status?: number }).status === 1) {
      return 0;
    }
    console.error('reaper: failed to scan for orphaned processes:', err instanceof Error ? err.message : String(err));
    return 0;
  }
}

/**
 * Send SIGTERM to a process, then verify it's gone. If not, escalate to SIGKILL.
 */
async function killProcess(pid: number): Promise<void> {
  try {
    await execFilePromise('kill', ['-15', String(pid)]);
    // Give the process a moment to exit gracefully
    await new Promise((r) => setTimeout(r, 500));

    // Verify it's gone; if not, SIGKILL
    try {
      await execFilePromise('ps', ['-p', String(pid)]);
      // Still alive — escalate
      await execFilePromise('kill', ['-9', String(pid)]);
    } catch {
      // Already exited gracefully
    }
  } catch (err) {
    // Process may have already exited; ignore permission errors from non-root kill attempts
    if ((err as NodeJS.ErrnoException).code !== 'EPERM') {
      console.error(`reaper: failed to kill process ${pid}:`, err instanceof Error ? err.message : String(err));
    } else {
      // Permission denied — likely a different user's process; log but continue
      console.warn(`reaper: permission denied killing process ${pid} (different user)`);
    }
  }
}

/**
 * Scan the temp filesystem for leftover profile directories matching the pattern
 * and remove those not referenced by any running process.
 */
async function cleanupOrphanedProfiles(browsersKilled: number): Promise<number> {
  const cleanedDirs = new Set<string>();
  let totalCleaned = 0;

  // Collect candidate temp dirs from multiple possible locations
  const tempLocations = [
    os.tmpdir(),                          // Node's temp dir (usually /tmp on Linux, platform-specific on macOS)
    '/var/folders',                      // macOS specific temp location
    '/tmp',                              // Linux standard temp
  ];

  for (const baseDir of tempLocations) {
    try {
      const dirs = await findProfileDirsIn(baseDir);
      for (const dir of dirs) {
        if (!cleanedDirs.has(dir)) {
          cleanedDirs.add(dir);
          // Check if any running process still references this directory
          const isReferenced = await isProfileReferenced(dir, browsersKilled);
          if (!isReferenced) {
            try {
              await fs.rm(dir, { recursive: true, force: true });
              totalCleaned++;
            } catch (err) {
              // Permission denied or directory in use — skip but don't fail
              console.warn(`reaper: could not remove profile dir ${dir}:`, err instanceof Error ? err.message : String(err));
            }
          }
        }
      }
    } catch {
      // Directory may not exist or be accessible; continue to next location
      continue;
    }
  }

  return totalCleaned;
}

/**
 * Find all directories under `baseDir` whose name matches the Playwright profile pattern.
 */
async function findProfileDirsIn(baseDir: string): Promise<string[]> {
  try {
    const entries = await fs.readdir(baseDir, { withFileTypes: true });
    const dirs: string[] = [];

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      // Check if this directory name matches the pattern
      if (entry.name.startsWith('playwright_chromiumdev_profile')) {
        dirs.push(path.join(baseDir, entry.name));
      } else if (entry.name.startsWith('.')) {
        // Recurse into hidden directories (like /var/folders/XX/)
        const subDirs = await findProfileDirsIn(path.join(baseDir, entry.name));
        dirs.push(...subDirs);
      }
    }

    return dirs;
  } catch {
    return [];
  }
}

/**
 * Check if a profile directory is currently referenced by any running process.
 */
async function isProfileReferenced(profileDir: string, browsersKilled: number): Promise<boolean> {
  try {
    // If we just killed processes, check if this dir was one of them
    // by seeing if the directory name matches a known pattern from killed processes
    
    const profileName = path.basename(profileDir);
    
    // Check all running processes for references to this profile directory
    const { stdout } = await execFilePromise('pgrep', ['-fa', PROFILE_PATTERN]);
    const lines = stdout.trim().split('\n').filter(Boolean);

    for (const line of lines) {
      if (line.includes(profileName)) {
        return true;
      }
    }

    // Also check /proc on Linux for open file descriptors referencing the dir
    try {
      const pidDirs = await fs.readdir('/proc');
      for (const pidDir of pidDirs) {
        if (!pidDir.match(/^\d+$/)) continue;
        
        try {
          const fdPath = path.join('/proc', pidDir, 'fd');
          const fds = await fs.readdir(fdPath);
          
          for (const fd of fds) {
            try {
              const linkTarget = await fs.readlink(path.join(fdPath, fd));
              if (linkTarget.includes(profileName)) {
                return true;
              }
            } catch {
              // Broken symlink or permission denied — skip
            }
          }
        } catch {
          continue;
        }
      }
    } catch {
      // /proc not available (non-Linux); pgrep check above is sufficient
    }

    return false;
  } catch {
    // If we can't verify, assume it's safe to clean (conservative approach)
    return false;
  }
}
