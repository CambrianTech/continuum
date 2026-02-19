/**
 * User Daemon - Server Implementation
 *
 * Server-side user lifecycle management.
 * Manages PersonaUser client instances, ensures all users have UserState.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { UserDaemon } from '../shared/UserDaemon';
import { PersonaUser } from '../../../system/user/server/PersonaUser';
import { SQLiteStateBackend } from '../../../system/user/storage/server/SQLiteStateBackend';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import { UserStateEntity } from '../../../system/data/entities/UserStateEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { ORM } from '../../data-daemon/server/ORM';
import { Events } from '../../../system/core/shared/Events';
import { DATA_EVENTS, getDataEventName } from '../../../system/core/shared/EventConstants';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';
import { getDefaultPreferencesForType } from '../../../system/user/config/UserCapabilitiesDefaults';
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';
import { JTAGClientServer } from '../../../system/core/client/server/JTAGClientServer';
import { AIDecisionLogger } from '../../../system/ai/server/AIDecisionLogger';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
import { SystemPaths } from '../../../system/core/config/SystemPaths';
import { UserEntityCache } from '../../../system/user/server/UserEntityCache';

export class UserDaemonServer extends UserDaemon {
  private static instance: UserDaemonServer | null = null;
  protected log: ComponentLogger;

  /**
   * Get singleton instance (for genome commands to access PersonaUsers)
   */
  public static getInstance(): UserDaemonServer | null {
    return UserDaemonServer.instance;
  }

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);

    // Store singleton instance
    UserDaemonServer.instance = this;

    // NOTE: All async initialization moved to initialize()/initializeDeferred()
    // Constructor should only do synchronous setup
  }

  /**
   * PHASE 1: Core initialization (BLOCKING)
   * Sets up event subscriptions - fast, no I/O, no external dependencies.
   */
  protected async initialize(): Promise<void> {
    const coreStart = Date.now();
    this.log.info('🚀 UserDaemonServer: CORE init starting...');

    // Initialize AI decision logger (synchronous, in-memory only)
    AIDecisionLogger.initialize();

    // Setup event subscriptions (fast, just registers handlers)
    await this.setupEventSubscriptions();

    // Subscribe to system:ready for deferred persona initialization
    this.subscribeToSystemReady();

    const coreMs = Date.now() - coreStart;
    this.log.info(`✅ UserDaemonServer: CORE init complete (${coreMs}ms) - READY`);
    this.log.info(`   ToolRegistry + Persona clients will initialize via system:ready event`);
  }

  /**
   * PHASE 2: Deferred initialization (NON-BLOCKING)
   * ToolRegistry + Persona clients initialization runs here, after daemon is READY.
   */
  protected async initializeDeferred(): Promise<void> {
    this.log.info('🔄 UserDaemonServer: DEFERRED init starting (ToolRegistry + Personas)...');
    const deferredStart = Date.now();

    // Initialize ToolRegistry for dynamic tool discovery
    await this.initializeToolRegistry();

    // Initialize persona clients - ensures they're created even if system:ready event was missed
    // This handles race conditions where DataDaemon emits before UserDaemon subscribes
    await this.ensurePersonaClients().catch((error: Error) => {
      this.log.error('❌ UserDaemon: Failed to initialize persona clients in deferred:', error);
    });

    const deferredMs = Date.now() - deferredStart;
    this.log.info(`✅ UserDaemonServer: DEFERRED init complete (${deferredMs}ms)`);
  }

  /**
   * Subscribe to system ready event to initialize personas when DataDaemon is ready
   * NOTE: ToolRegistry is initialized in initializeDeferred(), not here
   */
  private subscribeToSystemReady(): void {
    const unsubReady = Events.subscribe('system:ready', async (payload: any) => {
      if (payload?.daemon === 'data') {
        this.log.info('📡 UserDaemon: Received system:ready from DataDaemon, initializing personas...');

        await this.ensurePersonaClients().catch((error: Error) => {
          this.log.error('❌ UserDaemon: Failed to initialize persona clients:', error);
        });
      }
    });
    this.registerSubscription(unsubReady);
  }

  /**
   * Initialize ToolRegistry for dynamic tool discovery
   */
  private async initializeToolRegistry(): Promise<void> {
    this.log.info('⚙️ UserDaemon: Initializing ToolRegistry...');
    const { ToolRegistry } = await import('../../../system/tools/server/ToolRegistry');
    try {
      await ToolRegistry.getInstance().initialize();
      this.log.info('✅ UserDaemon: ToolRegistry initialized');
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.log.error(`❌ UserDaemon: Failed to initialize ToolRegistry: ${errorMsg}`);
    }
  }

  /**
   * Subscribe to user lifecycle events using universal Events system
   * Note: Chat messages are handled by individual PersonaUser instances, not UserDaemon
   */
  private async setupEventSubscriptions(): Promise<void> {
    // Listen for user creation using EventConstants
    const unsubCreated = Events.subscribe<UserEntity>(DATA_EVENTS.USERS.CREATED, async (userData: UserEntity) => {
      await this.handleUserCreated(userData);
    });
    this.registerSubscription(unsubCreated);

    // Listen for user updates using EventConstants
    const unsubUpdated = Events.subscribe<UserEntity>(DATA_EVENTS.USERS.UPDATED, async (userData: UserEntity) => {
      await this.handleUserUpdated(userData);
    });
    this.registerSubscription(unsubUpdated);

    // Listen for user deletion using EventConstants
    const unsubDeleted = Events.subscribe<UserEntity>(DATA_EVENTS.USERS.DELETED, async (userData: UserEntity) => {
      await this.handleUserDeleted(userData);
    });
    this.registerSubscription(unsubDeleted);

    // Listen for voice utterances directed to personas
    const unsubVoice = Events.subscribe('voice:utterance-for-persona', async (payload: { personaId: UUID; message: any }) => {
      const personaClient = this.personaClients.get(payload.personaId);
      if (personaClient && personaClient instanceof PersonaUser) {
        await personaClient.inbox.enqueue(payload.message);
        this.log.info(`🎙️ Enqueued voice message to ${personaClient.displayName}'s inbox`);
      } else {
        this.log.warn(`⚠️  Voice message for ${payload.personaId} but no PersonaUser client found`);
      }
    });
    this.registerSubscription(unsubVoice);

  }

  /**
   * Handle user created event
   * Note: Room membership handled by RoomMembershipDaemon (Discord-style auto-join)
   */
  private async handleUserCreated(userEntity: UserEntity): Promise<void> {
    try {
      // Ensure user has UserState
      await this.ensureUserHasState(userEntity.id);

      // For PersonaUsers, create client instance
      if (userEntity.type === 'persona') {
        await this.createPersonaClient(userEntity);
      }

      // HumanUser and AgentUser managed by SessionDaemon

    } catch (error) {
      this.log.error(`❌ UserDaemon: Failed to handle user creation for ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Handle user updated event
   */
  private async handleUserUpdated(userEntity: UserEntity): Promise<void> {
    // For personas, might need to reload state or update client
    if (userEntity.type === 'persona' && this.personaClients.has(userEntity.id)) {
      // TODO: Notify persona client of update
    }
  }

  /**
   * Handle user deleted event
   */
  private async handleUserDeleted(userEntity: UserEntity): Promise<void> {
    try {
      // For personas, cleanup client instance
      if (userEntity.type === 'persona' && this.personaClients.has(userEntity.id)) {
        // TODO: Shutdown persona client properly
        this.personaClients.delete(userEntity.id);
      }

      // Delete UserState (cascade)
      await ORM.remove(COLLECTIONS.USER_STATES, userEntity.id);

    } catch (error) {
      this.log.error(`❌ UserDaemon: Failed to cleanup user ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Ensure all existing PersonaUsers have active client instances
   */
  protected async ensurePersonaClients(): Promise<void> {
    this.log.info('🔧 UserDaemon: ensurePersonaClients() starting...');
    try {
      // Query all PersonaUser entities from database
      this.log.info('🔧 UserDaemon: Querying personas from database...');
      const result = await ORM.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: { type: 'persona' }
      });

      if (!result.success || !result.data) {
        this.log.warn('⚠️ UserDaemon: No personas found or query failed');
        return;
      }

      // Extract UserEntity from DataRecord - merge id from record level into entity data
      const personas = result.data.map(r => ({
        ...r.data,
        id: r.id
      } as UserEntity));

      this.log.info(`🔧 UserDaemon: Found ${personas.length} personas to initialize`);

      // Batched parallel initialization — 6 concurrent to avoid thundering herd on DB/Rust
      const BATCH_SIZE = 6;
      for (let i = 0; i < personas.length; i += BATCH_SIZE) {
        const batch = personas.slice(i, i + BATCH_SIZE);
        this.log.info(`🔧 UserDaemon: Initializing batch ${Math.floor(i / BATCH_SIZE) + 1}/${Math.ceil(personas.length / BATCH_SIZE)} (${batch.map(p => p.displayName).join(', ')})`);
        await Promise.all(batch.map(p => this.ensurePersonaCorrectState(p)));
      }

      this.log.info(`✅ UserDaemon: ensurePersonaClients() complete - processed ${personas.length} personas`);

    } catch (error) {
      // During initialization, DataDaemon might not be ready yet - this is expected
      const errorMessage = error instanceof Error ? error.message : String(error);
      if (errorMessage.includes('DataDaemon not initialized')) {
        this.log.warn('⚠️  UserDaemon: Deferring persona client initialization (DataDaemon not ready yet)');
      } else {
        // Unexpected error - log as ERROR
        this.log.error(`❌ UserDaemon: Failed to ensure persona clients: ${errorMessage}`);
      }
    }
  }

  /**
   * Ensure persona has correct state and client instance
   */
  private async ensurePersonaCorrectState(userEntity: UserEntity): Promise<void> {
    try {
      // STEP 1: Ensure UserState exists
      const hasState = await this.ensureUserHasState(userEntity.id);
      if (!hasState) {
        this.log.warn(`⚠️ UserDaemon: Failed to create UserState for ${userEntity.displayName}`);
        return;
      }

      // STEP 2: Check if persona client already exists
      if (this.personaClients.has(userEntity.id)) {
        return; // Don't recreate - instance already exists and is subscribed
      }

      // STEP 3: Create PersonaUser client instance
      await this.createPersonaClient(userEntity);

    } catch (error) {
      this.log.error(`❌ UserDaemon: Failed to ensure state for ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Create PersonaUser client instance
   * Expects both UserEntity and UserStateEntity to exist in database (created via user/create)
   */
  private async createPersonaClient(userEntity: UserEntity): Promise<void> {
    try {
      // Load UserStateEntity (must exist - created by user/create command)
      const userState = await ORM.read<UserStateEntity>(COLLECTIONS.USER_STATES, userEntity.id);

      if (!userState) {
        throw new Error(`UserStateEntity not found for persona ${userEntity.displayName} (${userEntity.id}) - user must be created via user/create command`);
      }

      // Initialize SQLite storage backend — path via SystemPaths (single source of truth)
      const dbPath = SystemPaths.personas.state(userEntity.uniqueId);
      const storage = new SQLiteStateBackend(dbPath);

      // Create JTAGClientServer for this persona via static connect method
      const clientResult = await JTAGClientServer.connect({
        userId: userEntity.id,
        targetEnvironment: 'server'
      });

      // Register client in static registry so Events.emit() can use sharedInstance
      JTAGClient.registerClient(`persona-${userEntity.id}`, clientResult.client);

      // Create PersonaUser instance with client injected
      const personaUser = new PersonaUser(userEntity, userState, storage, clientResult.client);

      // Initialize persona (loads rooms, subscribes to events)
      await personaUser.initialize();

      // Register in our client registry
      this.personaClients.set(userEntity.id, personaUser);

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : '';
      this.log.error(`❌ UserDaemon: Failed to create persona client for ${userEntity.displayName}: ${errorMsg}`);
      if (errorStack) {
        this.log.error(`   Stack: ${errorStack.split('\n').slice(1, 4).join('\n   ')}`);
      }
    }
  }

  /**
   * Ensure user has UserState entity
   */
  protected async ensureUserHasState(userId: UUID): Promise<boolean> {
    try {
      // Check if UserState exists
      const existingState = await ORM.read<UserStateEntity>(COLLECTIONS.USER_STATES, userId);

      if (existingState) {
        return true; // UserState exists
      }

      // UserState doesn't exist - create it
      return await this.createUserState(userId);

    } catch (error) {
      this.log.error(`❌ UserDaemon: Failed to check/create UserState for ${userId}:`, error);
      return false;
    }
  }

  /**
   * Create UserState for user
   */
  private async createUserState(userId: UUID): Promise<boolean> {
    try {
      // Load user entity to get type
      const user = await ORM.read<UserEntity>(COLLECTIONS.USERS, userId);
      if (!user) {
        this.log.error(`❌ UserDaemon: User ${userId} not found`);
        return false;
      }

      // Create UserState with type-specific defaults
      const userState = new UserStateEntity();
      userState.id = userId;
      userState.userId = userId;
      userState.deviceId = user.type === 'agent' ? 'agent-device' : 'server-device';
      userState.preferences = getDefaultPreferencesForType(user.type as 'human' | 'agent' | 'persona');

      // Store UserState
      const storeResult = await ORM.store<UserStateEntity>(
        COLLECTIONS.USER_STATES,
        userState
      );

      return !!storeResult;

    } catch (error) {
      this.log.error(`❌ UserDaemon: Failed to create UserState:`, error);
      return false;
    }
  }


  /**
   * Start continuous monitoring loops (using base class interval management)
   */
  protected startMonitoringLoops(): boolean {
    // User monitoring loop - every 30 seconds (was 5s — caused 134+ redundant ORM reads)
    this.registerInterval('user-monitoring', async () => {
      await this.runUserMonitoringLoop();
    }, 30000);

    // State reconciliation loop - every 30 seconds
    this.registerInterval('state-reconciliation', async () => {
      await this.runStateReconciliationLoop();
    }, 30000);

    return true;
  }

  /**
   * Stop monitoring loops (now handled by base class cleanupIntervals)
   */
  protected stopMonitoringLoops(): boolean {
    // No-op: Base class cleanupIntervals() handles this automatically in shutdown()
    return true;
  }

  /**
   * User monitoring loop - ensures all users have correct state
   */
  private async runUserMonitoringLoop(): Promise<void> {
    try {
      // Query ALL users from database
      const result = await ORM.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: {} // ALL users
      });

      if (!result.success || !result.data) {
        return;
      }

      // Extract UserEntity from DataRecord - merge id from record level into entity data
      const users = result.data.map(r => ({
        ...r.data,
        id: r.id
      } as UserEntity));

      // Populate UserEntityCache — eliminates redundant ORM reads for CallerDetector et al.
      UserEntityCache.instance.setAll(users);

      // Check each user
      for (const user of users) {
        if (user.type === 'persona') {
          await this.ensurePersonaCorrectState(user);
        }
        // Human and Agent users managed by SessionDaemon
      }

    } catch (error) {
      this.log.error('❌ UserDaemon: Monitoring loop error:', error);
    }
  }

  /**
   * State reconciliation loop - fixes inconsistencies
   */
  private async runStateReconciliationLoop(): Promise<void> {
    try {
      // Find personas that should have clients but don't
      const result = await ORM.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: { type: 'persona' }
      });

      if (!result.success || !result.data) {
        return;
      }

      // Extract UserEntity from DataRecord - id is at record level, data at .data level
      const personas = result.data.map(r => ({
        ...r.data,
        id: r.id  // Add id from DataRecord to entity
      } as UserEntity));

      for (const persona of personas) {
        if (!persona || !persona.id) {
          this.log.error(`❌ UserDaemon: Invalid persona data:`, persona);
          continue;
        }

        if (!this.personaClients.has(persona.id)) {
          await this.createPersonaClient(persona);
        }
      }

      // TODO: Find zombie persona clients (client exists but User entity deleted)
      // TODO: Clean up orphaned UserStates

    } catch (error) {
      this.log.error('❌ UserDaemon: Reconciliation loop error:', error);
    }
  }

  /**
   * Cleanup on shutdown
   * Base class handles interval and subscription cleanup automatically
   */
  protected async cleanup(): Promise<void> {
    // Shutdown all persona clients
    for (const userId of this.personaClients.keys()) {
      // TODO: Add shutdown method to PersonaUser
    }
    this.personaClients.clear();
  }
}