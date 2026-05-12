import { Events } from '../../core/shared/Events';
import { Commands } from '../../core/shared/Commands';
import type { CommandParams } from '../../core/types/JTAGTypes';
import { SecretManager } from '../../secrets/SecretManager';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import type { UserEntity } from '../../data/entities/UserEntity';

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

interface UserListResult { success: boolean; items?: readonly UserEntity[]; error?: string; }
interface UserCreateResult { success: boolean; user?: UserEntity; error?: string; }

export class PersonaLifecycleManager {
  private static _instance: PersonaLifecycleManager | null = null;
  private _subscribed = false;
  private runtimeActivator?: (user: UserEntity, reason: string) => Promise<void>;

  static get instance(): PersonaLifecycleManager {
    if (!this._instance) {
      this._instance = new PersonaLifecycleManager();
    }
    return this._instance;
  }

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

    setTimeout(() => this.runInitialAllocation().catch(err => {
      console.error('❌ PersonaLifecycleManager: Initial allocation failed:', err);
    }), 2000);
  }

  setRuntimeActivator(activate: (user: UserEntity, reason: string) => Promise<void>): void {
    this.runtimeActivator = activate;
  }

  private async runInitialAllocation(): Promise<void> {
    const availableApiKeys = this.collectAvailableApiKeys();
    console.log(`🎭 PersonaLifecycleManager: Initial allocation with ${availableApiKeys.length} API keys: [${availableApiKeys.join(', ')}]`);

    const allocation = await Commands.execute(
      'persona/allocate',
      { availableApiKeys } as Partial<CommandParams>
    ) as unknown as AllocationResult;

    if (!allocation?.allocations?.length) {
      const activated = await this.activatePersistedLocalPersonas(allocation);
      if (activated > 0) {
        console.log(`✅ PersonaLifecycleManager: ${activated} persisted persona(s) activated on startup`);
        return;
      }

      const summary = allocation?.summary?.length ? allocation.summary.join('; ') : 'no allocator summary';
      const skipped = allocation?.skipped?.length ? ` skipped=${allocation.skipped.length}` : '';
      throw new Error(`persona/allocate returned zero startup allocations and no persisted local personas were available;${skipped} summary=${summary}`);
    }

    console.log(`🎭 PersonaLifecycleManager: Allocator returned ${allocation.allocations.length} persona(s)`);

    let created = 0;
    for (const persona of allocation.allocations) {
      await this.createPersona(persona);
      created++;
    }

    console.log(`✅ PersonaLifecycleManager: ${created} persona(s) activated on startup`);

    if (process.env.CONTINUUM_PREWARM_PERSONAS === '1' || process.env.CONTINUUM_PREWARM_PERSONAS === 'true') {
      void this.prewarmAllPersonas(allocation.allocations);
    } else {
      console.log('⏭️ PersonaLifecycleManager: local model prewarm skipped (set CONTINUUM_PREWARM_PERSONAS=1 to enable)');
    }
  }

  private async prewarmAllPersonas(allocations: PersonaAllocation[]): Promise<void> {
    const local = allocations.filter(a => this.isLocalProvider(a.provider));
    if (local.length === 0) return;

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

  private isLocalProvider(provider: string): boolean {
    return provider === 'local' || provider === 'sentinel';
  }

  private async handleKeyAdded(event: KeyChangeEvent): Promise<void> {
    console.log(`🔑 PersonaLifecycleManager: Key added — ${event.provider}`);

    const availableApiKeys = this.collectAvailableApiKeys();

    const allocation = await Commands.execute(
      'persona/allocate',
      { availableApiKeys } as Partial<CommandParams>
    ) as unknown as AllocationResult;

    if (!allocation?.allocations) {
      console.warn('⚠️ PersonaLifecycleManager: No allocations returned from persona/allocate');
      return;
    }

    const newPersonas = allocation.allocations.filter(
      a => a.apiKeyEnv === event.provider
    );

    if (newPersonas.length === 0) {
      console.log(`ℹ️ PersonaLifecycleManager: No personas configured for ${event.provider}`);
      return;
    }

    for (const persona of newPersonas) {
      await this.createPersona(persona);
    }

    console.log(`✅ PersonaLifecycleManager: Created ${newPersonas.length} persona(s) for ${event.provider}`);
  }

  private async handleKeyRemoved(event: KeyChangeEvent): Promise<void> {
    console.log(`🔑 PersonaLifecycleManager: Key removed — ${event.provider}`);

    await Events.emit('persona:provider-deactivated', {
      provider: event.provider,
      timestamp: Date.now(),
    });

    console.log(`⚠️ PersonaLifecycleManager: Deactivation event emitted for ${event.provider} personas`);
  }

  private async createPersona(allocation: PersonaAllocation): Promise<void> {
    const result = await Commands.execute('user/create', {
      type: allocation.personaType,
      displayName: allocation.displayName,
      uniqueId: allocation.uniqueId,
      provider: allocation.provider,
    } as Partial<CommandParams>) as unknown as UserCreateResult;

    if (!result?.success || !result.user) {
      throw new Error(`user/create failed for persona ${allocation.displayName} (${allocation.uniqueId}): ${result?.error ?? 'missing user in result'}`);
    }

    await this.ensurePersonaRuntimeClient(result.user, 'allocator');
    console.log(`  ✅ Activated persona: ${allocation.displayName} (${allocation.uniqueId})`);
  }

  private async activatePersistedLocalPersonas(allocation?: AllocationResult): Promise<number> {
    const result = await Commands.execute('data/list', {
      dbHandle: 'default',
      collection: COLLECTIONS.USERS,
      filter: { type: 'persona' },
      limit: 100,
      skipCount: true,
    } as Partial<CommandParams>) as unknown as UserListResult;

    if (!result?.success) {
      throw new Error(`data/list failed while checking persisted personas: ${result?.error ?? 'unknown error'}`);
    }

    const personas = result.items ?? [];
    if (personas.length === 0) {
      return 0;
    }

    console.error(
      `❌ PersonaLifecycleManager: persona/allocate returned zero allocations with ${personas.length} persisted persona(s); activating persisted local personas and preserving the allocator defect for CI.`
    );
    if (allocation?.summary?.length) {
      console.error(`❌ PersonaLifecycleManager: allocator summary: ${allocation.summary.join('; ')}`);
    }

    for (const persona of personas) {
      await this.ensurePersonaRuntimeClient(persona, 'persisted-local');
    }
    return personas.length;
  }

  private async ensurePersonaRuntimeClient(user: UserEntity, reason: string): Promise<void> {
    if (user.type !== 'persona') {
      throw new Error(`Refusing to activate non-persona user ${user.displayName} (${user.id}) from ${reason}`);
    }

    if (!this.runtimeActivator) {
      throw new Error(`Persona runtime activator is not registered; cannot activate persona ${user.displayName} (${user.id}) from ${reason}`);
    }
    await this.runtimeActivator(user, reason);
  }

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
