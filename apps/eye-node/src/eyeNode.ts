/**
 * EyeNode — a headless, opt-in worker that gives personas EYES.
 *
 * It connects to the continuum core over the IPC socket, registers as the
 * provider of `perception/observe`, `perception/hot-edit`, `perception/interact` and
 * `perception/session-close`, and fulfils each
 * call by driving a real browser (`@continuum/perception`). The core's provider
 * seam (`ipc/provider_bridge.rs`) forwards a persona's call down the socket;
 * this process answers with pixels + structure (and, for hot-edit, the applied
 * CSS patch's before/after delta).
 *
 * "Opt-in" is the browserless-core principle: NOT every core runs a browser. A
 * browser-capable node (a laptop, a render worker that chose to install
 * Chromium) starts an EyeNode; a rack instance does not. While at least one
 * EyeNode is connected, every persona on that core can SEE; when none is,
 * `perception/observe` fails loud ("no eye-node connected") rather than
 * fabricating an observation.
 */

import net from 'node:net';
import os from 'node:os';

import { NodeSocketTransport, type DuplexSocketLike } from '@continuum/sdk-typescript';

import { hotEdit } from './hotEditAdapter';
import { InteractSessions } from './interactAdapter';
import { observe } from './observeAdapter';

export interface EyeNodeOptions {
  /** The core IPC socket path (or `tcp://host:port`). */
  socketPath: string;
  /** Human label the core logs this provider under. */
  label?: string;
}

export class EyeNode {
  private readonly transport: NodeSocketTransport;
  private readonly label: string;
  /** Live `perception/interact` sessions: bounded, idle-expired, closed on stop. */
  private readonly sessions = new InteractSessions();

  constructor(opts: EyeNodeOptions) {
    this.label = opts.label ?? `eye-node@${os.hostname()}`;
    this.transport = new NodeSocketTransport(
      () => connect(opts.socketPath),
      this.label,
    );
  }

  /** Register the perception verbs this node fulfils (`perception/observe` +
   *  `perception/hot-edit`) and block until the core has bound them. After this
   *  resolves, personas on the core can SEE — and hot-edit — through this node. */
  async start(): Promise<void> {
    this.transport.provide('perception/observe', {
      handle: async (paramsJson) => JSON.stringify(await observe(JSON.parse(paramsJson))),
    });
    this.transport.provide('perception/hot-edit', {
      handle: async (paramsJson) => JSON.stringify(await hotEdit(JSON.parse(paramsJson))),
    });
    // A persistent session a persona drives across calls (card 3569675f).
    this.transport.provide('perception/interact', {
      handle: async (paramsJson) => JSON.stringify(await this.sessions.interact(JSON.parse(paramsJson))),
    });
    this.transport.provide('perception/session-close', {
      handle: async (paramsJson) => JSON.stringify(await this.sessions.close(JSON.parse(paramsJson))),
    });
    await this.transport.flush();
  }

  /** Disconnect — the core unregisters this node's provider and observe then
   *  fails loud until an eye-node reconnects. */
  stop(): void {
    this.transport.close();
    // release every live browser; a stopping eye-node must not orphan them
    void this.sessions.closeAll();
  }
}

/** Open a socket to the core: a `tcp://host:port` URL, or a Unix socket path.
 *  `net.Socket` structurally satisfies the transport's {@link DuplexSocketLike}. */
function connect(socketPathOrUrl: string): DuplexSocketLike {
  const tcp = socketPathOrUrl.match(/^tcp:\/\/([^:/]+):(\d+)$/);
  const socket = tcp
    ? net.createConnection({ host: tcp[1], port: Number(tcp[2]) })
    : net.createConnection(socketPathOrUrl.replace(/^unix:\/\//, ''));
  return socket as unknown as DuplexSocketLike;
}
