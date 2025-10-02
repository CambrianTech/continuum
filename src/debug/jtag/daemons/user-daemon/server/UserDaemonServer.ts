/**
 * User Daemon - Server Implementation
 *
 * Server-side user lifecycle management.
 * Manages PersonaUser client instances, ensures all users have UserState.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { UserDaemon } from '../shared/UserDaemon';
import { PersonaUser } from '../../../system/user/shared/PersonaUser';
import { SQLiteStateBackend } from '../../../system/user/storage/server/SQLiteStateBackend';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import { UserStateEntity } from '../../../system/data/entities/UserStateEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';
import { getDefaultPreferencesForType } from '../../../system/user/config/UserCapabilitiesDefaults';

export class UserDaemonServer extends UserDaemon {
  private eventManager: EventManager;
  private monitoringInterval?: ReturnType<typeof setInterval>;
  private reconciliationInterval?: ReturnType<typeof setInterval>;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
    this.eventManager = new EventManager();
    void this.setupEventSubscriptions();
  }

  /**
   * Subscribe to user lifecycle events
   */
  private async setupEventSubscriptions(): Promise<void> {
    // Listen for user creation
    this.eventManager.events.on('data:User:created', async (userData) => {
      await this.handleUserCreated(userData as UserEntity);
    });

    // Listen for user updates
    this.eventManager.events.on('data:User:updated', async (userData) => {
      await this.handleUserUpdated(userData as UserEntity);
    });

    // Listen for user deletion
    this.eventManager.events.on('data:User:deleted', async (userData) => {
      await this.handleUserDeleted(userData as UserEntity);
    });

    console.log('📡 UserDaemon: Subscribed to user lifecycle events');
  }

  /**
   * Handle user created event
   */
  private async handleUserCreated(userEntity: UserEntity): Promise<void> {
    console.log(`🆕 UserDaemon: User created - ${userEntity.type} ${userEntity.displayName}`);

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
    console.log(`🔄 UserDaemon: User updated - ${userEntity.type} ${userEntity.displayName}`);

    // For personas, might need to reload state or update client
    if (userEntity.type === 'persona' && this.personaClients.has(userEntity.id)) {
      // TODO: Notify persona client of update
    }
  }

  /**
   * Handle user deleted event
   */
  private async handleUserDeleted(userEntity: UserEntity): Promise<void> {
    console.log(`🗑️ UserDaemon: User deleted - ${userEntity.type} ${userEntity.displayName}`);

    try {
      // For personas, cleanup client instance
      if (userEntity.type === 'persona' && this.personaClients.has(userEntity.id)) {
        const personaClient = this.personaClients.get(userEntity.id);
        // TODO: Shutdown persona client
        this.personaClients.delete(userEntity.id);
        console.log(`✅ UserDaemon: Removed persona client for ${userEntity.displayName}`);
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

      const personas = result.data.map(r => r.data);
      console.log(`🔍 UserDaemon: Found ${personas.length} PersonaUsers`);

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
        console.log(`✅ UserDaemon: Persona ${userEntity.displayName} already has client`);
        return;
      }

      // STEP 3: Create PersonaUser client instance
      await this.createPersonaClient(userEntity);

    } catch (error) {
      console.error(`❌ UserDaemon: Failed to ensure state for ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Create PersonaUser client instance
   */
  private async createPersonaClient(userEntity: UserEntity): Promise<void> {
    try {
      // Load or create UserStateEntity
      const userStateResult = await DataDaemon.read<UserStateEntity>(COLLECTIONS.USER_STATES, userEntity.id);
      let userState: UserStateEntity;

      if (!userStateResult.success || !userStateResult.data) {
        // Create default state
        const newState = new UserStateEntity();
        newState.id = userEntity.id;
        newState.userId = userEntity.id;
        newState.deviceId = 'server-device';
        userState = await DataDaemon.store<UserStateEntity>(COLLECTIONS.USER_STATES, newState);
      } else {
        userState = userStateResult.data as unknown as UserStateEntity;
      }

      // Initialize SQLite storage backend
      const dbPath = `.continuum/personas/${userEntity.id}/state.sqlite`;
      const storage = new SQLiteStateBackend(dbPath);

      // Create PersonaUser instance
      const personaUser = new PersonaUser(userEntity, userState, storage);

      // Register in our client registry
      this.personaClients.set(userEntity.id, personaUser);

      console.log(`✅ UserDaemon: Created PersonaUser client for ${userEntity.displayName}`);

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
      console.log(`💾 UserDaemon: Creating UserState for user ${userId}`);
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

      const user = userResult.data as unknown as UserEntity;

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

      if (storeResult) {
        console.log(`✅ UserDaemon: Created UserState for ${user.type} ${user.displayName}`);
        return true;
      }

      return false;

    } catch (error) {
      console.error(`❌ UserDaemon: Failed to create UserState:`, error);
      return false;
    }
  }


  /**
   * Start continuous monitoring loops
   */
  protected startMonitoringLoops(): void {
    // User monitoring loop - every 5 seconds
    this.monitoringInterval = setInterval(async () => {
      await this.runUserMonitoringLoop();
    }, 5000);

    // State reconciliation loop - every 30 seconds
    this.reconciliationInterval = setInterval(async () => {
      await this.runStateReconciliationLoop();
    }, 30000);

    console.log('🔄 UserDaemon: Started monitoring loops');
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

      const users = result.data.map(r => r.data);

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

      const personas = result.data.map(r => r.data);

      for (const persona of personas) {
        if (!this.personaClients.has(persona.id)) {
          console.warn(`🚀 UserDaemon: Starting missing persona client: ${persona.displayName}`);
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
  protected stopMonitoringLoops(): void {
    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = undefined;
    }

    if (this.reconciliationInterval) {
      clearInterval(this.reconciliationInterval);
      this.reconciliationInterval = undefined;
    }

    console.log('⏸️ UserDaemon: Stopped monitoring loops');
  }

  /**
   * Cleanup on shutdown
   */
  async shutdown(): Promise<void> {
    await super.shutdown();

    // Shutdown all persona clients
    for (const [userId, personaClient] of this.personaClients.entries()) {
      console.log(`👋 UserDaemon: Shutting down persona client ${userId}`);
      // TODO: Add shutdown method to PersonaUser
    }

    this.personaClients.clear();
  }
}