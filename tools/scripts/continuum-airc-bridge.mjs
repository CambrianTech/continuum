#!/usr/bin/env node
/**
 * continuum-airc-bridge
 *
 * Development harness for feeding AIRC traffic into Continuum. In stdin mode,
 * each input line becomes one airc/bridge command. JSON lines may provide
 * senderNick/channel/message; plain lines use CLI defaults.
 */

import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const JTAG_PATH = resolve(__dirname, '..', 'jtag');
const JTAG_CWD = dirname(JTAG_PATH);

function parseArgs() {
  const args = {
    senderNick: process.env.AIRC_NICK || 'airc-peer',
    channel: 'general',
    room: '',
    mirrorResponse: false,
    dryRun: false,
  };

  for (const arg of process.argv.slice(2)) {
    if (arg.startsWith('--senderNick=')) args.senderNick = arg.slice('--senderNick='.length);
    else if (arg.startsWith('--channel=')) args.channel = arg.slice('--channel='.length);
    else if (arg.startsWith('--room=')) args.room = arg.slice('--room='.length);
    else if (arg === '--mirror-response') args.mirrorResponse = true;
    else if (arg === '--dry-run') args.dryRun = true;
  }

  return args;
}

function parseLine(line, defaults) {
  const trimmed = line.trim();
  if (!trimmed) return null;

  if (trimmed.startsWith('{')) {
    const parsed = JSON.parse(trimmed);
    if (!parsed.message) throw new Error('JSON bridge line must include message');
    return {
      senderNick: parsed.senderNick || defaults.senderNick,
      channel: parsed.channel || defaults.channel,
      room: parsed.room || defaults.room,
      message: parsed.message,
    };
  }

  const match = trimmed.match(/^([^:]{1,80}):\s+(.+)$/);
  if (!match) {
    return { senderNick: defaults.senderNick, channel: defaults.channel, room: defaults.room, message: trimmed };
  }

  return { senderNick: match[1], channel: defaults.channel, room: defaults.room, message: match[2] };
}

function runBridge(line, defaults) {
  const params = {
    senderNick: line.senderNick || defaults.senderNick,
    channel: line.channel || defaults.channel,
    message: line.message,
  };

  const room = line.room || defaults.room;
  if (room) params.room = room;
  if (defaults.mirrorResponse) params.mirrorResponse = 'true';
  if (defaults.dryRun) params.dryRun = 'true';

  const argv = ['airc/bridge', ...Object.entries(params).map(([key, value]) => `--${key}=${value}`)];
  const result = spawnSync(JTAG_PATH, argv, { encoding: 'utf8', cwd: JTAG_CWD, timeout: 30000 });

  if (result.status !== 0) {
    process.stderr.write(`[continuum-airc-bridge] jtag failed (${result.status}): ${result.stderr || result.error?.message || ''}\n`);
    return;
  }

  process.stdout.write(result.stdout);
}

const args = parseArgs();
const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
process.stderr.write(`[continuum-airc-bridge] stdin mode channel=${args.channel} sender=${args.senderNick}\n`);

for await (const line of rl) {
  try {
    const bridgeLine = parseLine(line, args);
    if (bridgeLine) runBridge(bridgeLine, args);
  } catch (error) {
    process.stderr.write(`[continuum-airc-bridge] ${error instanceof Error ? error.message : String(error)}\n`);
  }
}
