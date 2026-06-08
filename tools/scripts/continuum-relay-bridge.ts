/**
 * continuum-relay-bridge — forward messages between Continuum chat and agent-relay
 *
 * Closes the loop for the cross-machine 3-Claude onboarding while the real
 * solution (shared Postgres for a grid-replicated room, or proper multi-
 * session agent-relay routing) isn't built yet.
 *
 * What it does:
 *
 *   1. Subscribes to the local Continuum chat via the same poll-with-
 *      afterMessageId pattern as continuum-chat-stream. For every NEW
 *      message (post-startup, not history), if the message was not itself
 *      bridged in, emit a directed `relay send <peer> <[bridge] ...>` to
 *      the configured relay peers.
 *
 *   2. Tails the local agent-relay messages.jsonl (default ~/.agent-relay).
 *      For every inbound relay message addressed to us that is NOT itself
 *      a bridged echo, post it into the local Continuum chat with a
 *      `[via <peer>]` prefix so humans and personas see the origin.
 *
 * Dedup:
 *
 *   - Every forwarded chat message carries a synthesized tag in its text:
 *       `[bridge:<origin-machine>:<chat-msg-id>]`
 *     On the receiving bridge, seeing that tag means "don't re-forward";
 *     the ring-buffer also remembers the last 200 tags seen and drops
 *     repeats.
 *
 *   - Every chat-to-relay direction also skips messages whose sender matches
 *     this bridge's own senderName (prevents the loop).
 *
 *   - The relay-to-chat direction skips messages that:
 *       a) don't contain a `[bridge-reply]` prefix (opt-in — relay traffic
 *          that isn't meant for the chat room stays private), OR
 *       b) have already been seen by msg signature.
 *
 * Runtime:
 *
 *   Manual today:
 *     npx tsx scripts/continuum-relay-bridge.ts \
 *       --peer=macbookpro-1724- --peer-display=memento-m1
 *
 *   Wired into npm start later as a managed subprocess alongside
 *   archive-worker / widget-server (workers-config.json entry).
 */

import { spawnSync } from 'child_process';
import { resolve, join } from 'path';
import { createReadStream, existsSync, statSync } from 'fs';
import { homedir } from 'os';
import * as readline from 'readline';

interface CliArgs {
  room: string;
  peer: string;         // relay peer name (e.g. "macbookpro-1724-")
  peerDisplay: string;  // human-readable label posted in chat (e.g. "memento-m1")
  pollMs: number;
  relayHome: string;    // AGENT_RELAY_HOME for outbound sends (defaults to ~/.agent-relay)
  originLabel: string;  // machine identity embedded in bridge tag (e.g. "m5-hub")
}

function parseArgs(): CliArgs {
  const a: CliArgs = {
    room: 'General',
    peer: '',
    peerDisplay: '',
    pollMs: 1000,
    relayHome: join(homedir(), '.agent-relay'),
    originLabel: 'm5-hub',
  };
  for (const arg of process.argv.slice(2)) {
    if (arg.startsWith('--room=')) a.room = arg.slice('--room='.length);
    else if (arg.startsWith('--peer=')) a.peer = arg.slice('--peer='.length);
    else if (arg.startsWith('--peer-display=')) a.peerDisplay = arg.slice('--peer-display='.length);
    else if (arg.startsWith('--pollMs=')) a.pollMs = parseInt(arg.slice('--pollMs='.length), 10) || 1000;
    else if (arg.startsWith('--relay-home=')) a.relayHome = arg.slice('--relay-home='.length);
    else if (arg.startsWith('--origin=')) a.originLabel = arg.slice('--origin='.length);
  }
  if (!a.peer) {
    process.stderr.write('[bridge] --peer=<relay peer name> is required\n');
    process.exit(2);
  }
  if (!a.peerDisplay) a.peerDisplay = a.peer;
  return a;
}

const JTAG_PATH = resolve(__dirname, '..', 'jtag');

function runJtag<T>(command: string, params: Record<string, unknown>): T | null {
  const flags: string[] = [];
  for (const [k, v] of Object.entries(params)) {
    if (v == null) continue;
    flags.push(`--${k}=${String(v)}`);
  }
  const result = spawnSync(JTAG_PATH, [command, ...flags], {
    encoding: 'utf-8',
    timeout: 10000,
  });
  if (result.status !== 0) {
    process.stderr.write(
      `[bridge] jtag ${command} failed (status=${result.status}): ${
        (result.stderr || result.error?.message || '').toString().trim()
      }\n`
    );
    return null;
  }
  const out = result.stdout || '';
  const start = out.indexOf('{');
  if (start < 0) return null;
  try {
    return JSON.parse(out.slice(start)) as T;
  } catch (e) {
    process.stderr.write(`[bridge] parse failure: ${e}\n`);
    return null;
  }
}

function relaySend(peer: string, message: string, relayHome: string): boolean {
  const result = spawnSync('relay', ['send', peer, message], {
    encoding: 'utf-8',
    env: { ...process.env, AGENT_RELAY_HOME: relayHome },
    timeout: 10000,
  });
  if (result.status !== 0) {
    process.stderr.write(
      `[bridge] relay send failed (status=${result.status}): ${(result.stderr || '').trim()}\n`
    );
    return false;
  }
  return true;
}

interface ChatMessage {
  id: string;
  senderId: string;
  senderName?: string;
  content?: { text?: string };
  timestamp?: string;
  replyToId?: string | null;
}

function shortId(id: string): string {
  return id.slice(0, 8);
}

/** Dedup ring buffer — remembers the last N tags/signatures we've acted on. */
class RecentSet {
  private order: string[] = [];
  private set = new Set<string>();
  constructor(private max: number) {}
  has(key: string): boolean {
    return this.set.has(key);
  }
  add(key: string): void {
    if (this.set.has(key)) return;
    this.set.add(key);
    this.order.push(key);
    while (this.order.length > this.max) {
      const old = this.order.shift();
      if (old !== undefined) this.set.delete(old);
    }
  }
}

async function sleep(ms: number): Promise<void> {
  return new Promise(r => setTimeout(r, ms));
}

// ─────────────────────────────────────────────────────────────────────────────
// Chat → Relay direction
// ─────────────────────────────────────────────────────────────────────────────

async function chatToRelayLoop(args: CliArgs, forwardedTags: RecentSet): Promise<void> {
  // Bootstrap: latest chat message id becomes the anchor so history
  // isn't re-forwarded on startup.
  const initial = runJtag<{ success: boolean; items?: ChatMessage[] }>(
    'data/list',
    {
      collection: 'chat_messages',
      limit: 1,
      orderBy: JSON.stringify([{ field: 'timestamp', direction: 'desc' }]),
    }
  );
  let afterMessageId: string | null = null;
  if (initial?.success && initial.items && initial.items.length > 0) {
    afterMessageId = initial.items[0].id;
  }
  process.stderr.write(
    `[bridge] chat→relay anchor=${afterMessageId ? shortId(afterMessageId) : 'none'}\n`
  );

  while (true) {
    if (!afterMessageId) {
      await sleep(args.pollMs);
      continue;
    }
    const poll = runJtag<{ success: boolean; messages?: ChatMessage[] }>(
      'collaboration/chat/poll',
      {
        afterMessageId,
        room: args.room,
        limit: 20,
      }
    );
    if (!poll?.success) {
      await sleep(args.pollMs);
      continue;
    }
    for (const msg of poll.messages ?? []) {
      afterMessageId = msg.id;
      const text = msg.content?.text ?? '';

      // Skip messages that came FROM the bridge itself (echoed back from
      // relay and posted into chat) — avoid ping-pong.
      if (text.startsWith(`[via ${args.peerDisplay}]`)) continue;

      // Skip messages whose text contains the bridge tag for this origin
      // (meaning we already forwarded it).
      if (text.includes(`[bridge:${args.originLabel}:`)) continue;

      // Skip messages where we already recorded forwarding this exact id.
      const tag = `${args.originLabel}:${msg.id}`;
      if (forwardedTags.has(tag)) continue;

      const bridgeLine = `[bridge:${tag}] ${msg.senderName ?? 'unknown'}: ${text}`;
      if (relaySend(args.peer, bridgeLine, args.relayHome)) {
        forwardedTags.add(tag);
        process.stderr.write(`[bridge] chat→relay ${shortId(msg.id)} → ${args.peer}\n`);
      }
    }
    await sleep(args.pollMs);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Relay → Chat direction
// ─────────────────────────────────────────────────────────────────────────────

interface RelayRow {
  from?: string;
  to?: string;
  ts?: string;
  msg?: string;
  sig?: string;
}

async function relayToChatLoop(args: CliArgs, seenRelayKeys: RecentSet): Promise<void> {
  const jsonlPath = join(args.relayHome, 'messages.jsonl');
  // Bootstrap: record current file size so we only process lines appended
  // after the bridge starts. Prevents replay of historical relay traffic.
  let offset = 0;
  if (existsSync(jsonlPath)) {
    offset = statSync(jsonlPath).size;
  }
  process.stderr.write(`[bridge] relay→chat tailing ${jsonlPath} from offset ${offset}\n`);

  while (true) {
    if (!existsSync(jsonlPath)) {
      await sleep(args.pollMs);
      continue;
    }
    const size = statSync(jsonlPath).size;
    if (size <= offset) {
      await sleep(args.pollMs);
      continue;
    }
    // Stream new bytes since last offset. ReadStream + readline handles
    // partial lines at the tail.
    const stream = createReadStream(jsonlPath, { start: offset, end: size });
    const rl = readline.createInterface({ input: stream, crlfDelay: Infinity });
    for await (const line of rl) {
      if (!line.trim()) continue;
      let row: RelayRow;
      try {
        row = JSON.parse(line) as RelayRow;
      } catch {
        continue;
      }
      const key = row.sig ?? `${row.from}:${row.ts}:${row.msg?.slice(0, 40)}`;
      if (seenRelayKeys.has(key)) continue;
      seenRelayKeys.add(key);

      const msg = row.msg ?? '';

      // Only forward relay messages opted-in via the [bridge-reply] prefix.
      // Every other relay message is peer-to-peer coordination that should
      // stay off the chat room.
      if (!msg.startsWith('[bridge-reply]')) continue;

      // Only forward messages FROM our configured peer (this bridge is
      // single-peer v1; multi-peer meshing comes later).
      if (row.from !== args.peer) continue;

      const clean = msg.slice('[bridge-reply]'.length).trim();
      const chatText = `[via ${args.peerDisplay}] ${clean}`;

      const sent = runJtag<{ success: boolean; shortId?: string }>(
        'collaboration/chat/send',
        {
          room: args.room,
          message: chatText,
        }
      );
      if (sent?.success) {
        process.stderr.write(
          `[bridge] relay→chat from=${row.from} → chat (posted as ${sent.shortId ?? '??'})\n`
        );
      } else {
        process.stderr.write(`[bridge] relay→chat send FAILED for from=${row.from}\n`);
      }
    }
    offset = size;
    await sleep(args.pollMs);
  }
}

async function main(): Promise<void> {
  const args = parseArgs();
  process.stderr.write(
    `[bridge] starting: room=${args.room} peer=${args.peer} ` +
    `peerDisplay=${args.peerDisplay} origin=${args.originLabel} relayHome=${args.relayHome}\n`
  );

  const forwardedTags = new RecentSet(200);
  const seenRelayKeys = new RecentSet(200);

  await Promise.all([
    chatToRelayLoop(args, forwardedTags),
    relayToChatLoop(args, seenRelayKeys),
  ]);
}

main().catch(err => {
  process.stderr.write(`[bridge] fatal: ${err instanceof Error ? err.message : err}\n`);
  process.exit(1);
});
