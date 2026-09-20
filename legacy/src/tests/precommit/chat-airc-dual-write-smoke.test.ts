#!/usr/bin/env npx tsx
/**
 * Stage-1 Chat -> AIRC dual-write smoke.
 *
 * Sends one real Continuum chat message through the public command bus, then
 * proves both stores received the same logical message:
 *   - ORM row exists in chat_messages.
 *   - AIRC event exists in the repo .airc event store, addressed by the JSON
 *     receipt id returned from chat/send.
 *
 * This intentionally uses sqlite3 -json for the AIRC event store instead of
 * parsing human CLI output. The command contract under test is the structured
 * chat-send result plus AIRC's persisted event record.
 */

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, parse, resolve } from 'node:path';
import { jtag } from '../../server-index';

const ROOM = process.env.AIRC_CHAT_SMOKE_ROOM ?? 'general';
const RUN_ID = `airc-dual-write-smoke-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
const MESSAGE = `${RUN_ID} prove ORM + AIRC dual-write receipt`;

interface ChatMessageRow {
  readonly id?: string;
  readonly roomId?: string;
  readonly content?: { readonly text?: string };
}

interface ChatSendAircResult {
  readonly ok?: boolean;
  readonly eventId?: string;
  readonly roomId?: string;
  readonly error?: string;
}

interface ChatSendResult {
  readonly success?: boolean;
  readonly message?: string;
  readonly messageEntity?: ChatMessageRow;
  readonly airc?: ChatSendAircResult;
}

interface CommandResult {
  readonly success?: boolean;
  readonly items?: readonly unknown[];
}

interface JtagClient {
  readonly commands: Record<string, (params: Record<string, unknown>) => Promise<unknown>>;
  readonly disconnect?: () => Promise<void>;
}

interface SqliteEventRow {
  readonly event_hex: string;
  readonly kind: string;
  readonly headers: string;
  readonly body: string | null;
}

interface AircJsonBody {
  readonly kind?: string;
  readonly value?: {
    readonly traceId?: string;
    readonly payload?: {
      readonly kind?: string;
      readonly payload?: {
        readonly schema?: string;
        readonly inline?: { readonly text?: string };
      };
    };
  };
}

async function main(): Promise<void> {
  const repoRoot = findRepoRoot();
  const aircHome = join(repoRoot, '.airc');

  console.log('chat-airc-dual-write smoke');
  console.log(`repo: ${repoRoot}`);
  console.log(`room: ${ROOM}`);

  await ensureAircRoom(repoRoot, aircHome, ROOM);

  let client: JtagClient | undefined;
  try {
    client = await jtag.connect() as unknown as JtagClient;
    const sendResult = await sendProbe(client);
    const messageId = assertOrmResult(sendResult);
    const aircEventId = assertAircReceipt(sendResult);

    await assertOrmRow(client, messageId);
    await assertAircEvent({
      dbPath: join(aircHome, 'events.sqlite'),
      eventId: aircEventId,
      messageId,
    });

    console.log('PASS chat-airc-dual-write smoke');
  } finally {
    if (client?.disconnect) {
      await client.disconnect();
    }
  }
}

async function ensureAircRoom(repoRoot: string, aircHome: string, room: string): Promise<void> {
  await runChecked('airc', ['--home', aircHome, 'room', room], {
    cwd: repoRoot,
    timeoutMs: 10_000,
  });
}

async function sendProbe(client: JtagClient): Promise<ChatSendResult> {
  const result = await client.commands['collaboration/chat/send']({
    room: ROOM,
    message: MESSAGE,
    isSystemTest: true,
  }) as ChatSendResult;

  if (!result?.success) {
    throw new Error(`collaboration/chat/send failed: ${JSON.stringify(result)}`);
  }
  return result;
}

function assertOrmResult(result: ChatSendResult): string {
  const messageId = result.messageEntity?.id;
  if (!messageId) {
    throw new Error(`chat/send did not return messageEntity.id: ${JSON.stringify(result)}`);
  }
  if (result.messageEntity?.content?.text !== MESSAGE) {
    throw new Error(`chat/send returned wrong message text for ${messageId}`);
  }
  return messageId;
}

function assertAircReceipt(result: ChatSendResult): string {
  if (!result.airc?.ok) {
    throw new Error(
      `chat/send AIRC dual-write failed or is unavailable. ` +
      `This usually means the running Continuum stack is not serving this checkout's code. ` +
      `airc=${JSON.stringify(result.airc)} resultKeys=${Object.keys(result).join(',')}`
    );
  }
  const eventId = result.airc.eventId;
  if (!eventId || !isUuid(eventId)) {
    throw new Error(`chat/send AIRC receipt missing valid event id: ${JSON.stringify(result.airc)}`);
  }
  if (!result.airc.roomId || !isUuid(result.airc.roomId)) {
    throw new Error(`chat/send AIRC receipt missing valid room id: ${JSON.stringify(result.airc)}`);
  }
  return eventId;
}

async function assertOrmRow(client: JtagClient, messageId: string): Promise<void> {
  const result = await client.commands['data/list']({
    collection: 'chat_messages',
    filter: { id: messageId },
    limit: 5,
  }) as CommandResult;

  if (!result?.success) {
    throw new Error(`data/list chat_messages failed: ${JSON.stringify(result)}`);
  }

  const rows = (result.items ?? []) as readonly ChatMessageRow[];
  const row = rows.find(item => item.id === messageId)
    ?? await findRecentOrmRow(client, messageId);
  if (!row) {
    throw new Error(`chat_messages row not found for ${messageId}`);
  }
  if (row.content?.text !== MESSAGE) {
    throw new Error(`chat_messages row ${messageId} has unexpected text`);
  }
}

async function findRecentOrmRow(client: JtagClient, messageId: string): Promise<ChatMessageRow | undefined> {
  const result = await client.commands['data/list']({
    collection: 'chat_messages',
    orderBy: [{ field: 'timestamp', direction: 'desc' }],
    limit: 100,
  }) as CommandResult;
  const rows = (result.items ?? []) as readonly ChatMessageRow[];
  return rows.find(item => item.id === messageId || item.content?.text === MESSAGE);
}

async function assertAircEvent(input: {
  dbPath: string;
  eventId: string;
  messageId: string;
}): Promise<void> {
  if (!existsSync(input.dbPath)) {
    throw new Error(`AIRC event store not found: ${input.dbPath}`);
  }

  const eventHex = uuidToHex(input.eventId);
  const sql = [
    'select',
    'hex(event_id) as event_hex,',
    'kind,',
    'headers,',
    'body',
    'from events',
    `where hex(event_id) = '${eventHex}'`,
    'limit 1;',
  ].join(' ');

  const stdout = await runChecked('sqlite3', ['-json', input.dbPath, sql], {
    cwd: dirname(input.dbPath),
    timeoutMs: 10_000,
  });
  const rows = JSON.parse(stdout || '[]') as readonly SqliteEventRow[];
  const row = rows[0];
  if (!row) {
    throw new Error(`AIRC event ${input.eventId} not found in ${input.dbPath}`);
  }
  if (row.kind !== 'message') {
    throw new Error(`AIRC event ${input.eventId} has kind=${row.kind}, expected message`);
  }

  const headers = parseHeaders(row);
  assertAircHeaders(headers, {
    eventId: input.eventId,
    messageId: input.messageId,
  });

  const body = parseAircJsonBody(row);
  assertAircBody(body, {
    eventId: input.eventId,
    messageId: input.messageId,
  });
}

function parseHeaders(row: SqliteEventRow): Record<string, string> {
  return JSON.parse(row.headers) as Record<string, string>;
}

function assertAircHeaders(
  headers: Record<string, string>,
  expected: { eventId: string; messageId: string },
): void {
  if (headers['forge.body_hint'] !== 'continuum.chat_transcript') {
    throw new Error(`AIRC event ${expected.eventId} missing forge.body_hint`);
  }
  if (headers['continuum.schema'] !== 'chat_transcript') {
    throw new Error(`AIRC event ${expected.eventId} missing continuum.schema`);
  }
  if (headers['continuum.trace_id'] !== expected.messageId) {
    throw new Error(`AIRC trace ${headers['continuum.trace_id']} != ORM message ${expected.messageId}`);
  }
}

function parseAircJsonBody(row: SqliteEventRow): AircJsonBody {
  return JSON.parse(row.body ?? '{}') as AircJsonBody;
}

function assertAircBody(
  body: AircJsonBody,
  expected: { eventId: string; messageId: string },
): void {
  if (body.kind !== 'json') {
    throw new Error(`AIRC event ${expected.eventId} body kind is not json`);
  }
  if (body.value?.traceId !== expected.messageId) {
    throw new Error(`AIRC body trace ${body.value?.traceId} != ORM message ${expected.messageId}`);
  }
  const payload = body.value?.payload?.payload;
  if (payload?.schema !== 'chat_transcript') {
    throw new Error(`AIRC body schema ${payload?.schema} != chat_transcript`);
  }
  if (payload.inline?.text !== MESSAGE) {
    throw new Error(`AIRC body text does not match probe`);
  }
}

function runChecked(
  command: string,
  args: readonly string[],
  options: { cwd: string; timeoutMs: number },
): Promise<string> {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, [...args], {
      cwd: options.cwd,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    let settled = false;
    const timer = setTimeout(() => {
      settled = true;
      child.kill('SIGTERM');
      reject(new Error(`${command} timed out after ${options.timeoutMs}ms`));
    }, options.timeoutMs);

    child.stdout?.on('data', (chunk: Buffer) => { stdout += chunk.toString('utf8'); });
    child.stderr?.on('data', (chunk: Buffer) => { stderr += chunk.toString('utf8'); });
    child.on('error', (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(error);
    });
    child.on('close', (exitCode) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (exitCode === 0) {
        resolvePromise(stdout);
      } else {
        reject(new Error(`${command} exited ${exitCode}: ${stderr.trim() || stdout.trim()}`));
      }
    });
  });
}

function findRepoRoot(): string {
  let dir = resolve(process.cwd());
  const root = parse(dir).root;
  while (dir !== root) {
    if (existsSync(join(dir, '.git')) && existsSync(join(dir, 'src', 'package.json'))) {
      return dir;
    }
    dir = dirname(dir);
  }
  throw new Error('Could not locate Continuum repo root');
}

function isUuid(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function uuidToHex(value: string): string {
  if (!isUuid(value)) {
    throw new Error(`Invalid UUID: ${value}`);
  }
  return value.replace(/-/g, '').toUpperCase();
}

main().catch((error: unknown) => {
  console.error('FAIL chat-airc-dual-write smoke');
  console.error(error instanceof Error ? error.stack ?? error.message : String(error));
  process.exit(2);
});
