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
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { Events } from '../../../system/core/shared/Events';
import { DATA_EVENTS, getDataEventName } from '../../../system/core/shared/EventConstants';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';
import { getDefaultPreferencesForType } from '../../../system/user/config/UserCapabilitiesDefaults';
import { JTAGClientServer } from '../../../system/core/client/server/JTAGClientServer';
import { AIDecisionLogger } from '../../../system/ai/server/AIDecisionLogger';

export class UserDaemonServer extends UserDaemon {
  private monitoringInterval?: ReturnType<typeof setInterval>;
  private reconciliationInterval?: ReturnType<typeof setInterval>;
  private unsubscribeFunctions: (() => void)[] = [];

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize AI decision logger for persona decision-making
    AIDecisionLogger.initialize('00000000-0000-0000-0000-000000000000'); // System session ID

    this.setupEventSubscriptions().catch((error: Error) => {
      console.error('❌ UserDaemon: Failed to setup event subscriptions:', error);
    });
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
    this.unsubscribeFunctions.push(unsubCreated);

    // Listen for user updates using EventConstants
    const unsubUpdated = Events.subscribe<UserEntity>(DATA_EVENTS.USERS.UPDATED, async (userData: UserEntity) => {
      await this.handleUserUpdated(userData);
    });
    this.unsubscribeFunctions.push(unsubUpdated);

    // Listen for user deletion using EventConstants
    const unsubDeleted = Events.subscribe<UserEntity>(DATA_EVENTS.USERS.DELETED, async (userData: UserEntity) => {
      await this.handleUserDeleted(userData);
    });
    this.unsubscribeFunctions.push(unsubDeleted);

  }

  /**
   * Handle user created event
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
      console.error(`❌ UserDaemon: Failed to handle user creation for ${userEntity.displayName}:`, error);
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
      await DataDaemon.remove(COLLECTIONS.USER_STATES, userEntity.id);

    } catch (error) {
      console.error(`❌ UserDaemon: Failed to cleanup user ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Ensure all existing PersonaUsers have active client instances
   */
  protected async ensurePersonaClients(): Promise<void> {
    try {
      // Query all PersonaUser entities from database
      const result = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filters: { type: 'persona' }
      });

      if (!result.success || !result.data) {
        console.warn('⚠️ UserDaemon: No personas found or query failed');
        return;
      }

      // Extract UserEntity from DataRecord - merge id from record level into entity data
      const personas = result.data.map(r => ({
        ...r.data,
        id: r.id
      } as UserEntity));

      // Ensure each persona has correct state
      for (const persona of personas) {
        await this.ensurePersonaCorrectState(persona);
      }

    } catch (error) {
      console.error('❌ UserDaemon: Failed to ensure persona clients:', error);
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
        console.warn(`⚠️ UserDaemon: Failed to create UserState for ${userEntity.displayName}`);
        return;
      }

      // STEP 2: Check if persona client already exists
      if (this.personaClients.has(userEntity.id)) {
        return; // Don't recreate - instance already exists and is subscribed
      }

      // STEP 3: Create PersonaUser client instance
      await this.createPersonaClient(userEntity);

    } catch (error) {
      console.error(`❌ UserDaemon: Failed to ensure state for ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Create PersonaUser client instance
   * Expects both UserEntity and UserStateEntity to exist in database (created via user/create)
   */
  private async createPersonaClient(userEntity: UserEntity): Promise<void> {
    try {
      // Load UserStateEntity (must exist - created by user/create command)
      const userStateResult = await DataDaemon.read<UserStateEntity>(COLLECTIONS.USER_STATES, userEntity.id);

      if (!userStateResult.success || !userStateResult.data) {
        throw new Error(`UserStateEntity not found for persona ${userEntity.displayName} (${userEntity.id}) - user must be created via user/create command`);
      }

      const userState: UserStateEntity = userStateResult.data.data;

      // Initialize SQLite storage backend
      const dbPath = `.continuum/personas/${userEntity.id}/state.sqlite`;
      const storage = new SQLiteStateBackend(dbPath);

      // Create JTAGClientServer for this persona via static connect method
      const clientResult = await JTAGClientServer.connect({
        userId: userEntity.id,
        targetEnvironment: 'server'
      });

      // Create PersonaUser instance with client injected
      const personaUser = new PersonaUser(userEntity, userState, storage, clientResult.client);

      // Initialize persona (loads rooms, subscribes to events)
      await personaUser.initialize();

      // Register in our client registry
      this.personaClients.set(userEntity.id, personaUser);

    } catch (error) {
      console.error(`❌ UserDaemon: Failed to create persona client for ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Ensure user has UserState entity
   */
  protected async ensureUserHasState(userId: UUID): Promise<boolean> {
    try {
      // Check if UserState exists
      const result = await DataDaemon.read<UserStateEntity>(COLLECTIONS.USER_STATES, userId);

      if (result.success && result.data) {
        return true; // UserState exists
      }

      // UserState doesn't exist - create it
      return await this.createUserState(userId);

    } catch (error) {
      console.error(`❌ UserDaemon: Failed to check/create UserState for ${userId}:`, error);
      return false;
    }
  }

  /**
   * Create UserState for user
   */
  private async createUserState(userId: UUID): Promise<boolean> {
    try {
      // Load user entity to get type
      const userResult = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);
      if (!userResult.success || !userResult.data) {
        console.error(`❌ UserDaemon: User ${userId} not found`);
        return false;
      }

      const user: UserEntity = userResult.data.data;

      // Create UserState with type-specific defaults
      const userState = new UserStateEntity();
      userState.id = userId;
      userState.userId = userId;
      userState.deviceId = user.type === 'agent' ? 'agent-device' : 'server-device';
      userState.preferences = getDefaultPreferencesForType(user.type as 'human' | 'agent' | 'persona');

      // Store UserState
      const storeResult = await DataDaemon.store<UserStateEntity>(
        COLLECTIONS.USER_STATES,
        userState
      );

      return !!storeResult;

    } catch (error) {
      console.error(`❌ UserDaemon: Failed to create UserState:`, error);
      return false;
    }
  }


  /**
   * Start continuous monitoring loops
   */
  protected startMonitoringLoops(): boolean {
    // User monitoring loop - every 5 seconds
    this.monitoringInterval = setInterval(() => {
      this.runUserMonitoringLoop().catch((error: Error) => {
        console.error('❌ UserDaemon: Monitoring loop error:', error);
      });
    }, 5000);

    // State reconciliation loop - every 30 seconds
    this.reconciliationInterval = setInterval(() => {
      this.runStateReconciliationLoop().catch((error: Error) => {
        console.error('❌ UserDaemon: Reconciliation loop error:', error);
      });
    }, 30000);

    return true;
  }

  /**
   * User monitoring loop - ensures all users have correct state
   */
  private async runUserMonitoringLoop(): Promise<void> {
    try {
      // Query ALL users from database
      const result = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filters: {} // ALL users
      });

      if (!result.success || !result.data) {
        return;
      }

      // Extract UserEntity from DataRecord - merge id from record level into entity data
      const users = result.data.map(r => ({
        ...r.data,
        id: r.id
      } as UserEntity));

      // Check each user
      for (const user of users) {
        if (user.type === 'persona') {
          await this.ensurePersonaCorrectState(user);
        }
        // Human and Agent users managed by SessionDaemon
      }

    } catch (error) {
      console.error('❌ UserDaemon: Monitoring loop error:', error);
    }
  }

  /**
   * State reconciliation loop - fixes inconsistencies
   */
  private async runStateReconciliationLoop(): Promise<void> {
    try {
      // Find personas that should have clients but don't
      const result = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filters: { type: 'persona' }
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
          console.error(`❌ UserDaemon: Invalid persona data:`, persona);
          continue;
        }

        if (!this.personaClients.has(persona.id)) {
          await this.createPersonaClient(persona);
        }
      }

      // TODO: Find zombie persona clients (client exists but User entity deleted)
      // TODO: Clean up orphaned UserStates

    } catch (error) {
      console.error('❌ UserDaemon: Reconciliation loop error:', error);
    }
  }

  /**
   * Stop continuous monitoring loops
   */
  protected stopMonitoringLoops(): boolean {
    let stopped = false;

    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = undefined;
      stopped = true;
    }

    if (this.reconciliationInterval) {
      clearInterval(this.reconciliationInterval);
      this.reconciliationInterval = undefined;
      stopped = true;
    }

    return stopped;
  }

  /**
   * Cleanup on shutdown
   */
  async shutdown(): Promise<void> {
    await super.shutdown();

    // Unsubscribe from all events
    for (const unsubscribe of this.unsubscribeFunctions) {
      unsubscribe();
    }
    this.unsubscribeFunctions = [];

    // Shutdown all persona clients
    for (const userId of this.personaClients.keys()) {
      // TODO: Add shutdown method to PersonaUser
    }

    this.personaClients.clear();
  }
}