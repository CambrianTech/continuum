/**
 * continuum-chat-stream — stream new chat messages as line-delimited events
 *
 * Designed for the Claude-native Monitor primitive (same pattern as
 * agent-relay's `relay connect`): each new chat message in the target
 * room becomes ONE line of stdout, so `Monitor(command="continuum-chat-stream
 * --room=General")` surfaces it as an inline notification.
 *
 * Usage:
 *   tsx scripts/continuum-chat-stream.ts --room=<name-or-unique-id>
 *   tsx scripts/continuum-chat-stream.ts --roomId=<uuid>
 *   tsx scripts/continuum-chat-stream.ts                (defaults to --room=General)
 *
 * Output format (one per new message):
 *   [<short-id>] <sender>: <one-line-summary>
 *
 * The script bootstraps by finding the newest message in the target room
 * at startup, then polls `collaboration/chat/poll` with that message id
 * as `afterMessageId`. Only messages AFTER script start are emitted —
 * no history replay (which was the earlier wrapper's bug).
 *
 * Polls every 1s. Runs forever until SIGTERM/SIGINT. Exits 0 on clean
 * shutdown, 1 on repeated command failure.
 */

import { spawnSync } from 'child_process';
import { resolve } from 'path';

interface CliArgs {
  room?: string;
  roomId?: string;
  pollMs: number;
  limit: number;
}

function parseArgs(): CliArgs {
  const args: CliArgs = { pollMs: 1000, limit: 50 };
  for (const a of process.argv.slice(2)) {
    if (a.startsWith('--room=')) args.room = a.slice('--room='.length);
    else if (a.startsWith('--roomId=')) args.roomId = a.slice('--roomId='.length);
    else if (a.startsWith('--pollMs=')) args.pollMs = parseInt(a.slice('--pollMs='.length), 10) || 1000;
    else if (a.startsWith('--limit=')) args.limit = parseInt(a.slice('--limit='.length), 10) || 50;
  }
  if (!args.room && !args.roomId) args.room = 'General';
  return args;
}

/**
 * Run a jtag command via the project's CLI. Returns parsed JSON result.
 * We shell out rather than import Commands.execute() because this script
 * is intended to be run standalone (e.g. from a user's ~/.claude/skills)
 * without needing to boot an entire TS runtime context.
 */
// Script lives at .../continuum/src/scripts/, jtag lives at .../continuum/src/jtag.
// One level up, not two. The earlier two-level resolve pointed to a
// nonexistent path and spawnSync silently returned status=-2 / ENOENT with
// empty stdout, which the old error check swallowed because stderr was also
// empty. That's why "no history" kept printing even though the DB had data.
const JTAG_PATH = resolve(__dirname, '..', 'jtag');

function runJtag<T>(command: string, params: Record<string, unknown>): T | null {
  const paramFlags: string[] = [];
  for (const [k, v] of Object.entries(params)) {
    if (v == null) continue;
    paramFlags.push(`--${k}=${String(v)}`);
  }
  const result = spawnSync(JTAG_PATH, [command, ...paramFlags], {
    encoding: 'utf-8',
    timeout: 10000,
  });
  // Any non-zero status (including -1 from spawn failure) is a real error
  // that should be visible — not silently swallowed.
  if (result.status !== 0) {
    process.stderr.write(
      `[stream] jtag ${command} failed (status=${result.status}): ${
        (result.stderr || result.error?.message || '').toString().trim()
      }\n`
    );
    return null;
  }
  const out = result.stdout || '';
  const start = out.indexOf('{');
  if (start < 0) {
    process.stderr.write(`[stream] jtag ${command} returned no JSON\n`);
    return null;
  }
  try {
    return JSON.parse(out.slice(start)) as T;
  } catch (e) {
    process.stderr.write(`[stream] failed to parse jtag output: ${e}\n`);
    return null;
  }
}

interface ChatMessage {
  id: string;
  senderId: string;
  senderName?: string;
  content?: { text?: string };
  timestamp?: string;
  replyToId?: string | null;
}

/**
 * Format a short 6-char hex id like the existing chat export does
 * (`#cd1f32`). Falls back to a truncated UUID if no short form exists.
 */
function shortId(id: string): string {
  return id.slice(0, 8);
}

function renderLine(msg: ChatMessage): string {
  const text = (msg.content?.text ?? '').replace(/\s+/g, ' ').slice(0, 200);
  const sender = msg.senderName ?? shortId(msg.senderId);
  const replyTag = msg.replyToId ? ` ⇢ ${shortId(msg.replyToId)}` : '';
  return `[${shortId(msg.id)}${replyTag}] ${sender}: ${text}`;
}

/**
 * Find the newest existing message in the target room — used as the
 * bootstrap anchor so we skip history on startup.
 */
function findAnchor(args: CliArgs): string | null {
  const exportResult = runJtag<{ success: boolean; messages?: ChatMessage[]; markdown?: string; message?: string }>(
    'collaboration/chat/export',
    {
      ...(args.roomId ? { roomId: args.roomId } : { room: args.room }),
      limit: 1,
    }
  );
  if (!exportResult || !exportResult.success) {
    return null;
  }
  // The export response is free-form (markdown), so we fall back to the
  // poll-then-discard bootstrap: if no messages exist yet we won't have
  // an anchor and the first real poll will find whatever arrives first.
  // (We don't strictly need an anchor — can emit the first-seen message.)
  return null;
}

async function sleep(ms: number): Promise<void> {
  return new Promise(r => setTimeout(r, ms));
}

async function main(): Promise<void> {
  const args = parseArgs();

  // Bootstrap: the export command gives us the current-latest messageId
  // if the room has any history. Skip anchor-finding for now and rely
  // on a "seen set" — simpler and avoids the export-format parsing edge
  // case. We remember message ids we've emitted this session and dedupe.

  process.stderr.write(
    `[stream] starting (${args.roomId ? 'roomId=' + args.roomId : 'room=' + args.room}, pollMs=${args.pollMs})\n`
  );

  // First poll: grab the most recent message as anchor. If room is empty,
  // start with null anchor and accept first message that appears.
  // Using data/list directly to get latest message by timestamp desc.
  // data/list returns `items: ChatMessage[]` (flat, not wrapped in `data:`).
  const initial = runJtag<{ success: boolean; items?: ChatMessage[] }>(
    'data/list',
    {
      collection: 'chat_messages',
      limit: 1,
      orderBy: JSON.stringify([{ field: 'timestamp', direction: 'desc' }]),
      ...(args.roomId
        ? { filter: JSON.stringify({ roomId: args.roomId }) }
        : {}),
    }
  );
  let afterMessageId: string | null = null;
  if (initial && initial.success && initial.items && initial.items.length > 0) {
    afterMessageId = initial.items[0].id;
    process.stderr.write(`[stream] anchor=${shortId(afterMessageId)} (history skipped)\n`);
  } else {
    process.stderr.write(`[stream] no history; waiting for first message\n`);
  }

  const seen = new Set<string>();
  let consecutiveFailures = 0;

  while (true) {
    if (!afterMessageId) {
      // No anchor yet — fall back to data/list polling for latest message.
      const latest = runJtag<{ success: boolean; items?: ChatMessage[] }>(
        'data/list',
        {
          collection: 'chat_messages',
          limit: 1,
          orderBy: JSON.stringify([{ field: 'timestamp', direction: 'desc' }]),
          ...(args.roomId
            ? { filter: JSON.stringify({ roomId: args.roomId }) }
            : {}),
        }
      );
      if (latest && latest.success && latest.items && latest.items.length > 0) {
        const msg = latest.items[0];
        if (!seen.has(msg.id)) {
          process.stdout.write(renderLine(msg) + '\n');
          seen.add(msg.id);
          afterMessageId = msg.id;
        }
      }
      await sleep(args.pollMs);
      continue;
    }

    const poll = runJtag<{ success: boolean; messages?: ChatMessage[]; error?: string }>(
      'collaboration/chat/poll',
      {
        afterMessageId,
        ...(args.roomId ? { roomId: args.roomId } : { room: args.room }),
        limit: args.limit,
      }
    );

    if (!poll || !poll.success) {
      consecutiveFailures++;
      if (consecutiveFailures > 30) {
        process.stderr.write(`[stream] giving up after 30 consecutive failures\n`);
        process.exit(1);
      }
      await sleep(args.pollMs);
      continue;
    }
    consecutiveFailures = 0;

    for (const msg of poll.messages ?? []) {
      if (seen.has(msg.id)) continue;
      seen.add(msg.id);
      process.stdout.write(renderLine(msg) + '\n');
      afterMessageId = msg.id;
    }

    await sleep(args.pollMs);
  }
}

main().catch(err => {
  process.stderr.write(`[stream] fatal: ${err instanceof Error ? err.message : err}\n`);
  process.exit(1);
});
