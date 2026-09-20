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
      publisher: 'airc-publish';
      lamport: number;
      occurredAtMs: number;
      channelName: string;
    }
  | {
      ok: false;
      eventId: string;
      roomId: string;
      publisher: 'airc-publish';
      error: string;
      exitCode?: number;
    };

export interface AircChatPublisher {
  publish(request: AircChatPublishRequest): Promise<AircChatPublishResult>;
}

export interface AircCliChatPublisherOptions {
  repoRoot?: string;
  timeoutMs?: number;
  runner?: AircCommandRunner;
}

export class AircCliChatPublisher implements AircChatPublisher {
  private readonly repoRoot: string;
  private readonly timeoutMs: number;
  private readonly runner: AircCommandRunner;

  constructor(options: AircCliChatPublisherOptions = {}) {
    this.repoRoot = options.repoRoot ?? findRepoRoot();
    this.timeoutMs = options.timeoutMs ?? 2500;
    this.runner = options.runner ?? runAirc;
  }

  async publish(request: AircChatPublishRequest): Promise<AircChatPublishResult> {
    const envelopeEventId = request.envelope.eventId;
    const roomId = request.envelope.roomId;
    const payload = serializeAircRealtimeEnvelope(request.envelope);
    const aircHome = path.join(this.repoRoot, '.airc');

    const result = await this.runner(
      buildPublishArgs(request),
      {
        cwd: this.repoRoot,
        env: { ...process.env, AIRC_HOME: aircHome },
        timeoutMs: this.timeoutMs,
        stdin: payload,
      },
    );

    if (result.exitCode === 0) {
      const receipt = parsePublishReceipt(result.stdout);
      if (!receipt.ok) {
        return {
          ok: false,
          eventId: envelopeEventId,
          roomId,
          publisher: 'airc-publish',
          exitCode: result.exitCode,
          error: receipt.error,
        };
      }
      return {
        ok: true,
        eventId: receipt.value.event_id,
        roomId: receipt.value.channel_id,
        publisher: 'airc-publish',
        lamport: receipt.value.lamport,
        occurredAtMs: receipt.value.occurred_at_ms,
        channelName: receipt.value.channel_name,
      };
    }

    return {
      ok: false,
      eventId: envelopeEventId,
      roomId,
      publisher: 'airc-publish',
      exitCode: result.exitCode,
      error: compactProcessError(result),
    };
  }
}

export interface RunAircOptions {
  cwd: string;
  env: NodeJS.ProcessEnv;
  timeoutMs: number;
  stdin?: string;
}

export interface RunAircResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}

export type AircCommandRunner = (argv: string[], options: RunAircOptions) => Promise<RunAircResult>;

export function buildPublishArgs(request: AircChatPublishRequest): string[] {
  return [
    'publish',
    '--room',
    request.roomName,
    '--kind',
    'message',
    '--body-json',
    '-',
    '--header',
    'forge.body_hint=continuum.chat_transcript',
    '--header',
    'continuum.schema=chat_transcript',
    '--header',
    `continuum.trace_id=${request.envelope.traceId ?? request.envelope.eventId}`,
    '--header',
    `continuum.room_id=${request.envelope.roomId}`,
  ];
}

interface AircPublishReceipt {
  event_id: string;
  lamport: number;
  occurred_at_ms: number;
  channel_id: string;
  channel_name: string;
}

type ParseReceiptResult =
  | { ok: true; value: AircPublishReceipt }
  | { ok: false; error: string };

export function parsePublishReceipt(stdout: string): ParseReceiptResult {
  const trimmed = stdout.trim();
  if (!trimmed) {
    return { ok: false, error: 'airc publish returned empty receipt' };
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (error) {
    return {
      ok: false,
      error: `airc publish returned invalid JSON receipt: ${error instanceof Error ? error.message : String(error)}`,
    };
  }

  if (!isPublishReceipt(parsed)) {
    return { ok: false, error: 'airc publish receipt missing required fields' };
  }

  return { ok: true, value: parsed };
}

function isPublishReceipt(value: unknown): value is AircPublishReceipt {
  if (!value || typeof value !== 'object') return false;
  const receipt = value as Partial<AircPublishReceipt>;
  return typeof receipt.event_id === 'string'
    && typeof receipt.lamport === 'number'
    && typeof receipt.occurred_at_ms === 'number'
    && typeof receipt.channel_id === 'string'
    && typeof receipt.channel_name === 'string';
}

function runAirc(argv: string[], options: RunAircOptions): Promise<RunAircResult> {
  return new Promise((resolve) => {
    const child = spawn('airc', argv, {
      stdio: options.stdin === undefined ? ['ignore', 'pipe', 'pipe'] : ['pipe', 'pipe', 'pipe'],
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

    child.stdout?.on('data', (chunk: Buffer) => { stdout += chunk.toString('utf8'); });
    child.stderr?.on('data', (chunk: Buffer) => { stderr += chunk.toString('utf8'); });
    if (options.stdin !== undefined) {
      child.stdin?.write(options.stdin);
      child.stdin?.end();
    }
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
