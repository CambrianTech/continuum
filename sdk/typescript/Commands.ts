/**
 * Commands — the typed Command primitive (CALL + SERVE) for the TS SDK.
 *
 * `execute<K>(name, params, target?)` infers BOTH params and result from the
 * command-name literal via the generated `CommandMap` — no manual `<T,U>`.
 * `provide<K>(name, adapter)` is the SERVE side: this client implements a command
 * the core routes here (client-provided commands like `interface/screenshot`,
 * capture — rust-origin contract, per-platform adapter). Commands = call + serve.
 *
 * Zero logic ([[headless-core-many-clients]]): thin typed wrapper over the facade
 * `Transport`. `CommandMap` is GENERATED (never a hand-maintained registry).
 * See docs/architecture/SDK-API-SURFACE.md.
 */

import type { Transport, Registration, Target } from './transport';
import { buildCommandUri, stampContext } from './transport';
import type { CommandMap, CommandName } from './generated/CommandMap';

export class Commands {
  /**
   * @param transport the facade binding
   * @param contextId the conversation/room scope this client is bound to (set by
   *   `Continuum.scoped(ctx)`); when present, `execute` stamps it into the request
   *   envelope as the `contextId` sibling — the third ID tier — so callers never
   *   re-thread the scope. Mirrors `continuum-client`'s `CommandClient`.
   */
  constructor(
    private readonly transport: Transport,
    private readonly contextId?: string,
  ) {}

  /**
   * CALL a command. The name literal infers params + result; `target` is the
   * cross-environment selector (omitted = local; `{peer, env:'web'}` etc.).
   * `execute('data/list', { collection })` → `Promise<DataListResult>`.
   * When this client is scoped, `contextId` is stamped into the envelope.
   */
  async execute<K extends CommandName>(
    name: K,
    params: CommandMap[K]['params'],
    target?: Target,
  ): Promise<CommandMap[K]['result']> {
    const uri = buildCommandUri(name, target);
    const envelope = stampContext(params, this.contextId);
    const resultJson = await this.transport.execute(uri, JSON.stringify(envelope));
    return JSON.parse(resultJson) as CommandMap[K]['result'];
  }

  /**
   * PROVIDE (serve) a command this client implements — the platform adapter for a
   * client-provided command. Contract is rust-origin (typed off `CommandMap`); the
   * impl is this platform's adapter (web DOM · desktop OS · AR/VR renderer).
   *
   *   commands.provide('interface/screenshot', async (p) => webCapture(p));
   *
   * One command identity, N platform adapters (OpenCV-style polymorphism).
   */
  provide<K extends CommandName>(
    name: K,
    adapter: (params: CommandMap[K]['params']) => Promise<CommandMap[K]['result']>,
  ): Registration {
    return this.transport.provide(name, {
      handle: async (paramsJson) => {
        const result = await adapter(JSON.parse(paramsJson) as CommandMap[K]['params']);
        return JSON.stringify(result);
      },
    });
  }
}
