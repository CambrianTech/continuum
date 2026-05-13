/**
 * PersonaLifecycleManager — runtime persona creation/removal based on API key changes.
 *
 * Subscribes to:
 * - system:config:key-added  → calls persona/allocate IPC, creates new personas
 * - system:config:key-removed → gracefully shuts down that provider's personas
 *
 * This enables the adaptive self-installing system: add an API key in Settings,
 * and the persona appears in chat within seconds — no restart needed.
 */

import { Events } from '../../core/shared/Events';
import { Commands } from '../../core/shared/Commands';
import type { CommandParams } from '../../core/types/JTAGTypes';
import { SecretManager } from '../../secrets/SecretManager';

interface KeyChangeEvent {
  provider: string;
  timestamp: number;
}

interface PersonaAllocation {
  uniqueId: string;
  displayName: string;
  provider: string;
  personaType: string;
  voiceId?: string;
  modelId?: string;
  isAudioNative: boolean;
  apiKeyEnv?: string;
  vramBudgetGb: number;
  resolvedModel?: string;
  reason: string;
  bio?: string;
  speciality?: string;
  accentColor?: string;
}

interface AllocationResult {
  allocations: PersonaAllocation[];
  skipped: PersonaAllocation[];
  summary: string[];
  gpuName: string;
  totalVramGb: number;
  gpuType: string;
  localModel: string;
}

export class PersonaLifecycleManager {
  private static _instance: PersonaLifecycleManager | null = null;
  private _subscribed = false;

  static get instance(): PersonaLifecycleManager {
    if (!this._instance) {
      this._instance = new PersonaLifecycleManager();
    }
    return this._instance;
  }

  /**
   * Start listening for key change events.
   * Call once during server startup (after commands are registered).
   */
  subscribe(): void {
    if (this._subscribed) return;
    this._subscribed = true;

    Events.subscribe('system:config:key-added', (event: KeyChangeEvent) => {
      this.handleKeyAdded(event).catch(err => {
        console.error('❌ PersonaLifecycleManager: Failed to handle key-added:', err);
      });
    });

    Events.subscribe('system:config:key-removed', (event: KeyChangeEvent) => {
      this.handleKeyRemoved(event).catch(err => {
        console.error('❌ PersonaLifecycleManager: Failed to handle key-removed:', err);
      });
    });

    console.log('🔄 PersonaLifecycleManager: Subscribed to config change events');

    // Run initial allocation on startup — config.env keys are already loaded
    // by SecretManager but no key-added event fires for pre-existing keys.
    setTimeout(() => this.runInitialAllocation().catch(err => {
      console.error('❌ PersonaLifecycleManager: Initial allocation failed:', err);
    }), 2000);
  }

  /**
   * Run allocation on startup with all currently available API keys.
   * Creates any personas that should exist based on the current hardware + keys.
   */
  private async runInitialAllocation(): Promise<void> {
    const availableApiKeys = this.collectAvailableApiKeys();
    console.log(`🎭 PersonaLifecycleManager: Initial allocation with ${availableApiKeys.length} API keys: [${availableApiKeys.join(', ')}]`);

    const allocation = await Commands.execute(
      'persona/allocate',
      { availableApiKeys } as Partial<CommandParams>
    ) as unknown as AllocationResult;

    if (!allocation?.allocations?.length) {
      console.warn('⚠️ PersonaLifecycleManager: No allocations from initial run');
      return;
    }

    console.log(`🎭 PersonaLifecycleManager: Allocator returned ${allocation.allocations.length} persona(s)`);

    let created = 0;
    for (const persona of allocation.allocations) {
      await this.createPersona(persona);
      created++;
    }

    console.log(`✅ PersonaLifecycleManager: ${created} persona(s) activated on startup`);

    // Local model prewarm allocates the full model/KV context. Doing that at
    // boot competes with seed, browser reconnect, and first room hydration, and
    // on unified-memory Macs can push continuum-core into OS pressure before
    // the system is actually ready. Keep it as an explicit performance knob,
    // not default startup behavior.
    if (process.env.CONTINUUM_PREWARM_PERSONAS === '1' || process.env.CONTINUUM_PREWARM_PERSONAS === 'true') {
      void this.prewarmAllPersonas(allocation.allocations);
    } else {
      console.log('⏭️ PersonaLifecycleManager: local model prewarm skipped (set CONTINUUM_PREWARM_PERSONAS=1 to enable)');
    }
  }

  /**
   * Fire prewarm requests in parallel for local personas. Each is bounded
   * by short timeouts so a stuck DMR can never hang boot.
   */
  private async prewarmAllPersonas(allocations: PersonaAllocation[]): Promise<void> {
    const local = allocations.filter(a => this.isLocalProvider(a.provider));
    if (local.length === 0) return;

    // Probe DMR availability ONCE before firing all prewarms — saves N
    // failed connection attempts when DMR isn't up yet (Docker still booting).
    const dmrUp = await this.checkDmrAvailable();
    if (!dmrUp) {
      console.log(`⏭️ PersonaLifecycleManager: DMR not reachable yet — skipping prewarm for ${local.length} local persona(s)`);
      return;
    }

    console.log(`🔥 PersonaLifecycleManager: Prewarming ${local.length} local persona(s)...`);
    const startedAt = Date.now();
    await Promise.allSettled(local.map(p => this.prewarmPersona(p)));
    console.log(`🔥 PersonaLifecycleManager: Prewarm batch finished in ${Date.now() - startedAt}ms`);
  }

  /**
   * Quick DMR availability probe with a hard 2s timeout. Returns false on
   * any failure (network, timeout, non-200) — never throws. Docker concern:
   * DMR runs in Docker Desktop's container; on cold Docker start it may
   * take a few seconds beyond our system boot to be reachable. We'd rather
   * skip prewarm than hang.
   */
  private async checkDmrAvailable(): Promise<boolean> {
    try {
      const ctrl = new AbortController();
      const timer = setTimeout(() => ctrl.abort(), 2000);
      const res = await fetch('http://localhost:12434/engines/v1/models', { signal: ctrl.signal });
      clearTimeout(timer);
      return res.ok;
    } catch {
      return false;
    }
  }

  /**
   * Fire a single tiny generation to warm the model + DMR slot for one persona.
   * max_tokens=1 keeps it nearly free; the cost we want is the model load,
   * not the generation. Errors are swallowed — prewarm failure is non-fatal.
   */
  private async prewarmPersona(allocation: PersonaAllocation): Promise<void> {
    const model = allocation.resolvedModel || allocation.modelId;
    if (!model) return;
    try {
      await Commands.execute('ai/generate', {
        provider: allocation.provider,
        model,
        messages: [{ role: 'user', content: 'ready' }],
        max_tokens: 1,
        temperature: 0.0,
      } as Partial<CommandParams>);
      console.log(`  🔥 Prewarmed: ${allocation.displayName} (${model})`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.log(`  ⏭️ Prewarm failed (non-fatal) for ${allocation.displayName}: ${msg}`);
    }
  }

  /**
   * Provider classes that route to the local DMR/llama-server pool — these
   * benefit from prewarm because they pay model-load cold start. Cloud
   * providers maintain their own warm state via API connection pooling.
   */
  private isLocalProvider(provider: string): boolean {
    return provider === 'local' || provider === 'sentinel';
  }

  /**
   * When an API key is added, re-run allocation and create any new personas.
   */
  private async handleKeyAdded(event: KeyChangeEvent): Promise<void> {
    console.log(`🔑 PersonaLifecycleManager: Key added — ${event.provider}`);

    // Collect all currently set API keys from process.env
    const availableApiKeys = this.collectAvailableApiKeys();

    // Call Rust allocator for optimal persona assignments
    const allocation = await Commands.execute(
      'persona/allocate',
      { availableApiKeys } as Partial<CommandParams>
    ) as unknown as AllocationResult;

    if (!allocation?.allocations) {
      console.warn('⚠️ PersonaLifecycleManager: No allocations returned from persona/allocate');
      return;
    }

    // Find personas that need this specific API key
    const newPersonas = allocation.allocations.filter(
      a => a.apiKeyEnv === event.provider
    );

    if (newPersonas.length === 0) {
      console.log(`ℹ️ PersonaLifecycleManager: No personas configured for ${event.provider}`);
      return;
    }

    // Create each new persona
    for (const persona of newPersonas) {
      await this.createPersona(persona);
    }

    console.log(`✅ PersonaLifecycleManager: Created ${newPersonas.length} persona(s) for ${event.provider}`);
  }

  /**
   * When an API key is removed, deactivate that provider's personas.
   */
  private async handleKeyRemoved(event: KeyChangeEvent): Promise<void> {
    console.log(`🔑 PersonaLifecycleManager: Key removed — ${event.provider}`);

    // Emit a deactivation event that PersonaUser instances can listen for
    await Events.emit('persona:provider-deactivated', {
      provider: event.provider,
      timestamp: Date.now(),
    });

    console.log(`⚠️ PersonaLifecycleManager: Deactivation event emitted for ${event.provider} personas`);
  }

  /**
   * Create a persona user via the user/create command.
   * The command already handles duplicate checking (idempotent).
   */
  private async createPersona(allocation: PersonaAllocation): Promise<void> {
    try {
      const result = await Commands.execute('user/create', {
        type: allocation.personaType,
        displayName: allocation.displayName,
        uniqueId: allocation.uniqueId,
        provider: allocation.provider,
      } as Partial<CommandParams>) as unknown as { success: boolean; error?: string };

      if (result?.success) {
        console.log(`  ✅ Created persona: ${allocation.displayName} (${allocation.uniqueId})`);
      } else {
        console.warn(`  ⚠️ Persona creation returned: ${JSON.stringify(result)}`);
      }
    } catch (error) {
      console.error(`  ❌ Failed to create persona ${allocation.displayName}:`, error);
    }
  }

  /**
   * Collect all API key env vars that are currently set in process.env.
   * These are the keys the Rust allocator needs to make decisions.
   */
  private collectAvailableApiKeys(): string[] {
    const knownKeyVars = [
      'ANTHROPIC_API_KEY',
      'OPENAI_API_KEY',
      'DEEPSEEK_API_KEY',
      'GROQ_API_KEY',
      'XAI_API_KEY',
      'TOGETHER_API_KEY',
      'FIREWORKS_API_KEY',
      'DASHSCOPE_API_KEY',
      'GOOGLE_API_KEY',
      'HF_TOKEN',
      'SENTINEL_PATH',
    ];

    const secrets = SecretManager.getInstance();
    return knownKeyVars.filter(key => Boolean(secrets.get(key, 'PersonaLifecycleManager.collectAvailableApiKeys')));
  }
}
