/**
 * CollaborativeActivityWidget - Base class for real-time collaborative widgets
 *
 * Provides the infrastructure for any widget that:
 * - Loads operations from database on init
 * - Saves new operations when user acts
 * - Subscribes to real-time operation events
 * - Replays operations to build/rebuild state
 *
 * Subclasses implement domain-specific rendering and state management.
 *
 * @example
 * class ChessBoardWidget extends CollaborativeActivityWidget<ChessMoveEntity, ChessMoveData> {
 *   protected collectionName = 'chess_moves';
 *   protected domainName = 'chess';
 *
 *   protected applyOperation(op: ChessMoveEntity): void {
 *     // Move piece on board
 *     this.board.movePiece(op.data.from, op.data.to);
 *   }
 *
 *   protected buildStateFromOperations(ops: ChessMoveEntity[]): void {
 *     // Reset board and replay all moves
 *     this.board.reset();
 *     for (const op of ops) {
 *       if (!op.isUndone) this.applyOperation(op);
 *     }
 *   }
 *
 *   protected createOperationData(move: UserMove): Partial<ChessMoveData> {
 *     return { from: move.from, to: move.to, piece: move.piece };
 *   }
 * }
 */

import { BaseWidget, type WidgetConfig } from './BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { Commands } from '../../system/core/shared/Commands';
import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import {
  CollaborativeOperationEntity,
  createOperationEventNames,
  type OperationMeta
} from '../../system/data/entities/CollaborativeOperationEntity';

/**
 * Configuration for CollaborativeActivityWidget
 */
export interface CollaborativeActivityConfig extends WidgetConfig {
  /** Maximum operations to load on init */
  maxOperationsToLoad?: number;
  /** Whether to auto-subscribe to real-time events */
  enableRealTimeSync?: boolean;
}

/**
 * Result from user/get-me command
 */
interface UserGetMeResult extends CommandResult {
  success: boolean;
  user?: {
    id: UUID;
    displayName: string;
  };
}

/**
 * Generic list result for operations
 */
interface OperationListResult<T> extends CommandResult {
  success: boolean;
  operations?: T[];
  count?: number;
  error?: string;
}

/**
 * Generic add result for operations
 */
interface OperationAddResult extends CommandResult {
  success: boolean;
  operationId?: UUID;
  error?: string;
}

/**
 * Abstract base class for collaborative activity widgets
 *
 * STORAGE ARCHITECTURE:
 * - Operations (metadata) loaded from DB via list command
 * - Content loaded from per-activity storage via loadContent()
 * - Subclasses implement domain-specific content loading/rendering
 *
 * @typeParam TEntity - The operation entity type (extends CollaborativeOperationEntity)
 * @typeParam TMeta - The operation metadata type (small, in DB)
 */
export abstract class CollaborativeActivityWidget<
  TEntity extends CollaborativeOperationEntity<TMeta>,
  TMeta extends OperationMeta = OperationMeta
> extends BaseWidget {

  // ─────────────────────────────────────────────────────────────────
  // Abstract members - subclasses MUST implement
  // ─────────────────────────────────────────────────────────────────

  /** Database collection name (e.g., 'chess_moves', 'canvas_strokes') */
  protected abstract readonly collectionName: string;

  /** Domain name for events/commands (e.g., 'chess', 'canvas') */
  protected abstract readonly domainName: string;

  /** Apply a single operation to the widget's state/display */
  protected abstract applyOperation(op: TEntity): void;

  /** Rebuild entire state by replaying all operations */
  protected abstract buildStateFromOperations(ops: TEntity[]): void;

  // ─────────────────────────────────────────────────────────────────
  // Protected state
  // ─────────────────────────────────────────────────────────────────

  /** Current activity instance ID */
  protected _activityId: UUID | null = null;

  /** Loaded operations */
  protected operations: TEntity[] = [];

  /** Track loaded operation IDs to prevent duplicates */
  protected loadedOperationIds = new Set<string>();

  /** Current user ID for attribution */
  protected _userId: UUID | null = null;

  /** Current user display name */
  protected _userName: string = 'Unknown';

  /** Event unsubscribe function */
  private operationEventUnsubscribe: (() => void) | null = null;

  /** Configuration */
  protected readonly activityConfig: CollaborativeActivityConfig;

  // ─────────────────────────────────────────────────────────────────
  // Constructor
  // ─────────────────────────────────────────────────────────────────

  constructor(config: CollaborativeActivityConfig) {
    super({
      ...config,
      enableDatabase: true, // Always need database for operations
    });

    this.activityConfig = {
      maxOperationsToLoad: 1000,
      enableRealTimeSync: true,
      ...config,
    };
  }

  // ─────────────────────────────────────────────────────────────────
  // Public accessors
  // ─────────────────────────────────────────────────────────────────

  /** Get the current activity ID */
  get activityId(): UUID | null {
    // Check attribute first (set by recipe/content system)
    const attrId = this.getAttribute('activity-id') || this.getAttribute('entity-id');
    return attrId || this._activityId;
  }

  /** Set the activity ID */
  set activityId(id: UUID | null) {
    if (this._activityId !== id) {
      this._activityId = id;
      // Reload operations when activity changes
      if (id) {
        this.reloadOperations();
      }
    }
  }

  /** Get current user ID */
  get userId(): UUID | null {
    return this._userId;
  }

  /** Get current user name */
  get userName(): string {
    return this._userName;
  }

  /** Get count of loaded operations */
  get operationCount(): number {
    return this.operations.length;
  }

  // ─────────────────────────────────────────────────────────────────
  // Lifecycle
  // ─────────────────────────────────────────────────────────────────

  protected async onWidgetInitialize(): Promise<void> {
    this.verbose() && console.log(`🎯 ${this.constructor.name}: Initializing collaborative activity...`);

    // Load current user info
    await this.loadUserInfo();

    // Subscribe to real-time events if enabled
    if (this.activityConfig.enableRealTimeSync) {
      this.subscribeToOperationEvents();
    }

    this.verbose() && console.log(`✅ ${this.constructor.name}: Ready for collaboration`);
  }

  async disconnectedCallback(): Promise<void> {
    await super.disconnectedCallback();

    // Unsubscribe from events
    if (this.operationEventUnsubscribe) {
      this.operationEventUnsubscribe();
      this.operationEventUnsubscribe = null;
    }
  }

  // ─────────────────────────────────────────────────────────────────
  // User info
  // ─────────────────────────────────────────────────────────────────

  /** Load current user info for operation attribution */
  protected async loadUserInfo(): Promise<void> {
    try {
      const result = await Commands.execute<CommandParams, UserGetMeResult>('user/get-me', {});
      if (result.success && result.user) {
        this._userId = result.user.id;
        this._userName = result.user.displayName || 'Unknown';
        this.verbose() && console.log(`🎯 ${this.constructor.name}: User identified as ${this._userName}`);
      }
    } catch (err) {
      console.warn(`🎯 ${this.constructor.name}: Could not get user info`);
    }
  }

  // ─────────────────────────────────────────────────────────────────
  // Operation loading
  // ─────────────────────────────────────────────────────────────────

  /**
   * Load all operations for current activity
   * Called after widget is fully initialized (ctx ready, etc.)
   */
  protected async loadOperations(): Promise<void> {
    if (!this.activityId) {
      console.warn(`🎯 ${this.constructor.name}: No activityId, skipping operation load`);
      return;
    }

    try {
      // Use Record<string, unknown> to bypass strict param typing
      // Domain-specific params vary by implementation
      const params = {
        activityId: this.activityId,
        limit: this.activityConfig.maxOperationsToLoad,
      } as Record<string, unknown>;

      const result = await Commands.execute(
        `${this.domainName}/operation/list`,
        params
      ) as OperationListResult<TEntity>;

      if (result.success && result.operations) {
        this.verbose() && console.log(`🎯 ${this.constructor.name}: Loading ${result.operations.length} operations`);
        this.operations = result.operations;

        // Track loaded IDs
        for (const op of result.operations) {
          this.loadedOperationIds.add(op.id);
        }

        // Build state from operations
        this.buildStateFromOperations(result.operations);
      }
    } catch (error) {
      console.error(`❌ ${this.constructor.name}: Failed to load operations:`, error);
    }
  }

  /** Reload operations (e.g., when activity changes) */
  protected async reloadOperations(): Promise<void> {
    this.operations = [];
    this.loadedOperationIds.clear();
    await this.loadOperations();
  }

  // ─────────────────────────────────────────────────────────────────
  // Operation saving
  // ─────────────────────────────────────────────────────────────────

  /**
   * Save a new operation
   * Called by subclass when user performs an action
   *
   * @param opType - Operation type (e.g., 'stroke', 'move', 'delete')
   * @param meta - Operation metadata (small, goes in DB)
   * @param content - Optional large content (stored in activity storage)
   */
  protected async saveOperation(
    opType: string,
    meta: TMeta,
    content?: unknown
  ): Promise<UUID | null> {
    if (!this.activityId) {
      console.warn(`🎯 ${this.constructor.name}: No activityId, cannot save operation`);
      return null;
    }

    try {
      // Use Record<string, unknown> to bypass strict param typing
      // Domain-specific params vary by implementation
      const addParams = {
        activityId: this.activityId,
        opType,
        meta,
        content,  // Optional - command handles content storage
      } as Record<string, unknown>;

      const result = await Commands.execute(
        `${this.domainName}/operation/add`,
        addParams
      ) as OperationAddResult;

      if (result.success && result.operationId) {
        // Track as loaded to prevent duplicate rendering from events
        this.loadedOperationIds.add(result.operationId);
        return result.operationId;
      } else {
        console.error(`❌ ${this.constructor.name}: Failed to save operation:`, result.error);
        return null;
      }
    } catch (error) {
      console.error(`❌ ${this.constructor.name}: Error saving operation:`, error);
      return null;
    }
  }

  // ─────────────────────────────────────────────────────────────────
  // Real-time sync
  // ─────────────────────────────────────────────────────────────────

  /** Subscribe to operation events for real-time collaboration */
  protected subscribeToOperationEvents(): void {
    const eventNames = createOperationEventNames(this.domainName);

    this.operationEventUnsubscribe = Events.subscribe(
      eventNames.ADDED,
      (eventData: {
        activityId: UUID;
        operationId: UUID;
        operation: TEntity;
      }) => {
        // Only process operations for our activity
        if (eventData.activityId !== this.activityId) return;

        // Don't re-render operations we already have
        if (this.loadedOperationIds.has(eventData.operationId)) return;

        // Don't render our own operations (we applied them locally)
        if (eventData.operation.creatorId === this._userId) return;

        this.verbose() && console.log(`🎯 ${this.constructor.name}: Received operation from ${eventData.operation.creatorName}`);

        // Apply the remote operation
        this.operations.push(eventData.operation);
        this.loadedOperationIds.add(eventData.operationId);
        this.applyOperation(eventData.operation);
      }
    );
  }

  // ─────────────────────────────────────────────────────────────────
  // Undo/Redo support (optional, subclasses can use)
  // ─────────────────────────────────────────────────────────────────

  /**
   * Get operations that can be undone (most recent first)
   */
  protected getUndoableOperations(): TEntity[] {
    return this.operations
      .filter(op => !op.isUndone && op.creatorId === this._userId)
      .reverse();
  }

  /**
   * Get operations that can be redone (most recent first)
   */
  protected getRedoableOperations(): TEntity[] {
    return this.operations
      .filter(op => op.isUndone && op.creatorId === this._userId)
      .reverse();
  }
}
