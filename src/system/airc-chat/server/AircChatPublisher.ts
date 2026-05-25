import { spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import * as path from 'node:path';
import type { AircRealtimeEnvelope } from '@shared/generated/airc/AircRealtimeEnvelope';
import { serializeAircRealtimeEnvelope } from '../shared/AircChatEnvelope';

export interface AircChatPublishRequest {
  roomName: string;
  envelope: AircRealtimeEnvelope;
}

export type AircChatPublishResult =
  | {
      ok: true;
      eventId: string;
      roomId: string;
      publisher: 'airc-cli';
    }
  | {
      ok: false;
      eventId: string;
      roomId: string;
      publisher: 'airc-cli';
      error: string;
      exitCode?: number;
    };

export interface AircChatPublisher {
  publish(request: AircChatPublishRequest): Promise<AircChatPublishResult>;
}

export interface AircCliChatPublisherOptions {
  repoRoot?: string;
  timeoutMs?: number;
}

export class AircCliChatPublisher implements AircChatPublisher {
  private readonly repoRoot: string;
  private readonly timeoutMs: number;

  constructor(options: AircCliChatPublisherOptions = {}) {
    this.repoRoot = options.repoRoot ?? findRepoRoot();
    this.timeoutMs = options.timeoutMs ?? 2500;
  }

  async publish(request: AircChatPublishRequest): Promise<AircChatPublishResult> {
    const eventId = request.envelope.eventId;
    const roomId = request.envelope.roomId;
    const payload = serializeAircRealtimeEnvelope(request.envelope);
    const aircHome = path.join(this.repoRoot, '.airc');

    const result = await runAirc(
      ['msg', payload],
      {
        cwd: this.repoRoot,
        env: { ...process.env, AIRC_HOME: aircHome },
        timeoutMs: this.timeoutMs,
      },
    );

    if (result.exitCode === 0) {
      return { ok: true, eventId, roomId, publisher: 'airc-cli' };
    }

    return {
      ok: false,
      eventId,
      roomId,
      publisher: 'airc-cli',
      exitCode: result.exitCode,
      error: compactProcessError(result),
    };
  }
}

interface RunAircOptions {
  cwd: string;
  env: NodeJS.ProcessEnv;
  timeoutMs: number;
}

interface RunAircResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}

function runAirc(argv: string[], options: RunAircOptions): Promise<RunAircResult> {
  return new Promise((resolve) => {
    const child = spawn('airc', argv, {
      stdio: ['ignore', 'pipe', 'pipe'],
      cwd: options.cwd,
      env: options.env,
    });

    let stdout = '';
    let stderr = '';
    let settled = false;
    const timer = setTimeout(() => {
      settled = true;
      child.kill('SIGTERM');
      resolve({
        exitCode: -1,
        stdout,
        stderr,
        timedOut: true,
      });
    }, options.timeoutMs);

    child.stdout.on('data', (chunk: Buffer) => { stdout += chunk.toString('utf8'); });
    child.stderr.on('data', (chunk: Buffer) => { stderr += chunk.toString('utf8'); });
    child.on('error', (error: NodeJS.ErrnoException) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({
        exitCode: -1,
        stdout,
        stderr: error.code === 'ENOENT'
          ? 'airc CLI not found on PATH'
          : error.message,
        timedOut: false,
      });
    });
    child.on('close', (exitCode) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({ exitCode: exitCode ?? -1, stdout, stderr, timedOut: false });
    });
  });
}

function compactProcessError(result: RunAircResult): string {
  if (result.timedOut) {
    return 'airc publish timed out';
  }
  const detail = [result.stderr.trim(), result.stdout.trim()].filter(Boolean).join(' | ');
  return detail || `airc exited with code ${result.exitCode}`;
}

function findRepoRoot(): string {
  let dir = process.cwd();
  const root = path.parse(dir).root;
  while (dir !== root) {
    if (existsSync(path.join(dir, '.git'))) return dir;
    const pkgPath = path.join(dir, 'package.json');
    if (existsSync(pkgPath)) {
      try {
        const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8')) as { name?: string };
        if (pkg.name === 'continuum' || pkg.name === '@continuum/root') return dir;
      } catch {
        // Keep walking.
      }
    }
    dir = path.dirname(dir);
  }
  return process.cwd();
}
