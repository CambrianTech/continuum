/**
 * Preflight — Prerequisite checks for TypeScript tools.
 *
 * Platform-aware: works on macOS, Linux, and WSL.
 * Same checks as preflight.sh, callable from TypeScript.
 *
 * Usage:
 *   import { Preflight } from './shared/Preflight';
 *   Preflight.checkBuildTools();
 *   Preflight.require('jq');
 */

import { spawnSync } from 'child_process';

export interface PreflightResult {
  check: string;
  passed: boolean;
  message: string;
}

export type Platform = 'macos' | 'linux' | 'wsl' | 'unknown';

export class Preflight {

  static get platform(): Platform {
    if (process.platform === 'darwin') return 'macos';
    if (process.platform === 'linux') {
      try {
        const version = spawnSync('cat', ['/proc/version'], { stdio: ['pipe', 'pipe', 'pipe'] });
        if (version.stdout?.toString().toLowerCase().includes('microsoft')) return 'wsl';
      } catch { /* not WSL */ }
      return 'linux';
    }
    return 'unknown';
  }

  /**
   * Platform-aware build tool check.
   * macOS: Xcode license + CLI tools
   * Linux/WSL: gcc, make, pkg-config
   */
  static checkBuildTools(): PreflightResult {
    switch (Preflight.platform) {
      case 'macos': return Preflight._checkMacosTools();
      case 'linux':
      case 'wsl':   return Preflight._checkLinuxTools();
      default:       return { check: 'build-tools', passed: true, message: 'Unknown platform — skipped' };
    }
  }

  /** @deprecated Use checkBuildTools() */
  static checkXcode(): PreflightResult {
    return Preflight.checkBuildTools();
  }

  /**
   * Check that a tool is available on PATH.
   */
  static require(tool: string): PreflightResult {
    const result = spawnSync('which', [tool], {
      stdio: ['pipe', 'pipe', 'pipe'],
      timeout: 5_000,
    });

    if (result.status === 0) {
      return { check: `require:${tool}`, passed: true, message: `${tool} found` };
    }

    const suggestion = Preflight._suggestInstall(tool);
    return {
      check: `require:${tool}`,
      passed: false,
      message: `Required tool not found: ${tool}. Install: ${suggestion}`,
    };
  }

  /**
   * Detect build tool issues in cargo build output.
   * Returns a PreflightResult if a known issue was detected, null otherwise.
   */
  static detectCargoBuildFailure(stderr: string): PreflightResult | null {
    if (stderr.includes('exit status: 69') || stderr.includes('xcodebuild -license')) {
      return {
        check: 'cargo-build',
        passed: false,
        message: 'Xcode license not accepted. Accept it, then try again: sudo xcodebuild -license accept',
      };
    }

    if (stderr.includes('xcrun') && stderr.includes('error')) {
      return {
        check: 'cargo-build',
        passed: false,
        message: 'Command-line build tools not found. Install: xcode-select --install',
      };
    }

    if (stderr.includes('linker') && stderr.includes('not found') || stderr.includes('cannot find -l')) {
      return {
        check: 'cargo-build',
        passed: false,
        message: 'Missing system libraries. Install build dependencies for your platform.',
      };
    }

    return null;
  }

  /** @deprecated Use detectCargoBuildFailure() */
  static detectCargoXcodeFailure(stderr: string): PreflightResult | null {
    return Preflight.detectCargoBuildFailure(stderr);
  }

  /**
   * Run all preflight checks. Returns array of results (does not throw).
   */
  static checkAll(): PreflightResult[] {
    const results: PreflightResult[] = [];
    results.push(Preflight.checkBuildTools());
    return results;
  }

  /**
   * Run all checks and throw on first failure.
   */
  static assertAll(): void {
    for (const r of Preflight.checkAll()) {
      if (!r.passed) {
        throw new Error(`Preflight failed [${r.check}]: ${r.message}`);
      }
    }
  }

  // --- Private ---

  private static _checkMacosTools(): PreflightResult {
    const result = spawnSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
      stdio: ['pipe', 'pipe', 'pipe'],
      timeout: 10_000,
    });

    if (result.status === 0) {
      return { check: 'build-tools', passed: true, message: 'Build tools OK' };
    }

    if (result.status === 69) {
      return {
        check: 'build-tools',
        passed: false,
        message: 'Xcode license not accepted. Open Xcode.app or run: sudo xcodebuild -license accept',
      };
    }

    return {
      check: 'build-tools',
      passed: false,
      message: 'Command-line build tools not found. Install Xcode or run: xcode-select --install',
    };
  }

  private static _checkLinuxTools(): PreflightResult {
    const missing: string[] = [];

    for (const tool of ['gcc', 'make', 'pkg-config']) {
      const r = spawnSync('which', [tool], { stdio: ['pipe', 'pipe', 'pipe'], timeout: 3_000 });
      if (r.status !== 0) {
        // gcc: also accept cc
        if (tool === 'gcc') {
          const cc = spawnSync('which', ['cc'], { stdio: ['pipe', 'pipe', 'pipe'], timeout: 3_000 });
          if (cc.status === 0) continue;
        }
        missing.push(tool);
      }
    }

    if (missing.length === 0) {
      return { check: 'build-tools', passed: true, message: 'Build tools OK' };
    }

    const mgr = Preflight._detectPkgManager();
    let suggestion = '(install build-essential or equivalent)';
    if (mgr === 'apt') suggestion = 'sudo apt-get install -y build-essential pkg-config';
    else if (mgr === 'dnf') suggestion = "sudo dnf groupinstall -y 'Development Tools'";
    else if (mgr === 'yum') suggestion = "sudo yum groupinstall -y 'Development Tools'";

    return {
      check: 'build-tools',
      passed: false,
      message: `Missing build tools: ${missing.join(', ')}. Install: ${suggestion}`,
    };
  }

  private static _detectPkgManager(): string {
    for (const mgr of ['brew', 'apt-get', 'dnf', 'yum']) {
      const r = spawnSync('which', [mgr], {
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: 3_000,
      });
      if (r.status === 0) return mgr === 'apt-get' ? 'apt' : mgr;
    }
    return '';
  }

  private static _suggestInstall(tool: string): string {
    const mgr = Preflight._detectPkgManager();
    switch (mgr) {
      case 'brew': return `brew install ${tool}`;
      case 'apt':  return `sudo apt-get install -y ${tool}`;
      case 'dnf':  return `sudo dnf install -y ${tool}`;
      case 'yum':  return `sudo yum install -y ${tool}`;
      default:     return `(install ${tool} using your system package manager)`;
    }
  }
}
