/**
 * StateStorage — positron's durable local-state seam (the "Twitter model").
 *
 * A positron client's whole renderable state is *latest envelope per kind*:
 * every `StateEnvelope` is a full snapshot of its widget kind, so persisting
 * just the newest one per kind is sufficient to repaint the entire interface.
 * That makes local durability a tiny, inherent property of the state feed —
 * NOT an app feature:
 *
 *   - boot   → hydrate: cached envelopes render instantly (even offline),
 *   - live   → write-through: every incoming envelope replaces its kind's row,
 *   - drop   → the UI keeps last-known state + a visible "reconnecting" status;
 *              it never quietly degrades into a lesser app.
 *
 * `StateStorageAdapter` is the ONE seam. Platform adapters implement it —
 * IndexedDB here for the browser, in-memory for tests/ephemeral surfaces; the
 * swift/kotlin/flutter SDKs mirror this exact interface over their native
 * stores. No renderer ever touches localStorage or invents its own cache
 * ([[one-logical-decision-one-place]]): apps pass an adapter to
 * `StateConnection` and inherit durability.
 *
 * `scope` partitions the cache — one citizen+endpoint's state must never bleed
 * into another's. `StateConnection` defaults it to the scoped connect URL
 * (which carries `?me=<citizen>`).
 */

import type { StateEnvelope } from './generated/positron/StateEnvelope';

/** A cached envelope row: the envelope plus when it was persisted (staleness display). */
export interface PersistedEnvelope {
  envelope: StateEnvelope;
  savedAtMs: number;
}

/**
 * The platform seam for durable positron state. Implementations must be safe
 * to call concurrently and must treat `save` as replace-by-(scope, kind).
 */
export interface StateStorageAdapter {
  /** All cached envelopes for a scope (one per kind), oldest-saved first. */
  load(scope: string): Promise<PersistedEnvelope[]>;
  /** Replace the cached envelope for (scope, envelope.kind). */
  save(scope: string, envelope: StateEnvelope): Promise<void>;
  /** Drop a scope's cache entirely (sign-out / citizen switch). */
  clear(scope: string): Promise<void>;
}

/** Row key: scope and kind joined by NUL — unambiguous, range-scannable. */
const rowKey = (scope: string, kind: string): string => `${scope}\u0000${kind}`;
const scopePrefix = (scope: string): string => `${scope}\u0000`;

/**
 * In-memory adapter — tests and ephemeral surfaces (an SSR pass, a headless
 * observer that wants hydrate semantics without persistence). Also the
 * conformance reference: the IndexedDB adapter must be behaviorally identical.
 */
export class MemoryStateStorage implements StateStorageAdapter {
  private readonly rows = new Map<string, PersistedEnvelope>();

  load(scope: string): Promise<PersistedEnvelope[]> {
    const prefix = scopePrefix(scope);
    const out: PersistedEnvelope[] = [];
    for (const [k, row] of this.rows) {
      if (k.startsWith(prefix)) out.push(row);
    }
    out.sort((a, b) => a.savedAtMs - b.savedAtMs);
    return Promise.resolve(out);
  }

  save(scope: string, envelope: StateEnvelope): Promise<void> {
    this.rows.set(rowKey(scope, envelope.kind), { envelope, savedAtMs: Date.now() });
    return Promise.resolve();
  }

  clear(scope: string): Promise<void> {
    const prefix = scopePrefix(scope);
    for (const k of [...this.rows.keys()]) {
      if (k.startsWith(prefix)) this.rows.delete(k);
    }
    return Promise.resolve();
  }
}

/**
 * Browser adapter over IndexedDB (async, structured-clone, sized for real
 * state — everything localStorage is not). One DB, one object store; the row
 * key is `rowKey(scope, kind)` so a scope's rows form a contiguous range.
 *
 * Failure posture: storage is an ACCELERANT, not a dependency — a browser with
 * IndexedDB unavailable (private-mode edge cases) degrades to live-only.
 * `load` resolves `[]` and `save` resolves silently after logging once; the
 * feed itself never breaks on cache trouble.
 */
export class IndexedDbStateStorage implements StateStorageAdapter {
  private db?: Promise<IDBDatabase | undefined>;
  private warned = false;

  constructor(
    private readonly dbName = 'positron-state',
    private readonly storeName = 'envelopes',
  ) {}

  private open(): Promise<IDBDatabase | undefined> {
    if (this.db) return this.db;
    this.db = new Promise((resolve) => {
      const idb = (globalThis as { indexedDB?: IDBFactory }).indexedDB;
      if (!idb) {
        this.warnOnce('IndexedDB unavailable — positron state cache disabled (live-only)');
        resolve(undefined);
        return;
      }
      const req = idb.open(this.dbName, 1);
      req.onupgradeneeded = () => {
        if (!req.result.objectStoreNames.contains(this.storeName)) {
          req.result.createObjectStore(this.storeName);
        }
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => {
        this.warnOnce(`IndexedDB open failed — positron state cache disabled: ${String(req.error)}`);
        resolve(undefined);
      };
    });
    return this.db;
  }

  private warnOnce(msg: string): void {
    if (!this.warned) {
      this.warned = true;
      console.warn(`StateStorage: ${msg}`);
    }
  }

  private keyRange(scope: string): IDBKeyRange {
    // Every row key is `scope\u0000kind`; `scope\u0001` is the tightest
    // exclusive upper bound over that prefix family (\u0000 sorts ABOVE the
    // bare scope string, so bare `scope` cannot be the upper bound).
    return IDBKeyRange.bound(`${scope}\u0000`, `${scope}\u0001`, false, true);
  }

  async load(scope: string): Promise<PersistedEnvelope[]> {
    const db = await this.open();
    if (!db) return [];
    return new Promise((resolve) => {
      const tx = db.transaction(this.storeName, 'readonly');
      const req = tx.objectStore(this.storeName).getAll(this.keyRange(scope));
      req.onsuccess = () => {
        const rows = (req.result as PersistedEnvelope[]).filter(
          (r) => r && typeof r === 'object' && 'envelope' in r,
        );
        rows.sort((a, b) => a.savedAtMs - b.savedAtMs);
        resolve(rows);
      };
      req.onerror = () => {
        this.warnOnce(`IndexedDB load failed: ${String(req.error)}`);
        resolve([]);
      };
    });
  }

  async save(scope: string, envelope: StateEnvelope): Promise<void> {
    const db = await this.open();
    if (!db) return;
    return new Promise((resolve) => {
      const tx = db.transaction(this.storeName, 'readwrite');
      const row: PersistedEnvelope = { envelope, savedAtMs: Date.now() };
      tx.objectStore(this.storeName).put(row, rowKey(scope, envelope.kind));
      tx.oncomplete = () => resolve();
      tx.onerror = () => {
        this.warnOnce(`IndexedDB save failed: ${String(tx.error)}`);
        resolve();
      };
    });
  }

  async clear(scope: string): Promise<void> {
    const db = await this.open();
    if (!db) return;
    return new Promise((resolve) => {
      const tx = db.transaction(this.storeName, 'readwrite');
      tx.objectStore(this.storeName).delete(this.keyRange(scope));
      tx.oncomplete = () => resolve();
      tx.onerror = () => {
        this.warnOnce(`IndexedDB clear failed: ${String(tx.error)}`);
        resolve();
      };
    });
  }
}
