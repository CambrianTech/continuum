/**
 * CRUD Sync Debug Command Types
 *
 * Verifies database/UI synchronization across all three main widgets:
 * - room-list-widget: Rooms data
 * - chat-widget: Messages data
 * - user-list-widget: Users data
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface CrudSyncDebugParams extends CommandParams {
  collections?: string[]; // ['Room', 'ChatMessage', 'User'] by default
  includeDatabase?: boolean; // Compare with actual DB data
  maxItems?: number; // Limit items per widget for readability
}

export interface WidgetSyncData {
  widgetName: string;
  found: boolean;
  path: string;
  itemCount: number;
  items: Array<{
    id: string;
    displayText: string;
    dataAttributes: Record<string, string>;
    index: number;
  }>;
}

export interface DatabaseSyncData {
  collection: string;
  count: number;
  items: Array<{
    id: string;
    name?: string;
    content?: any;
    createdAt?: string;
  }>;
}

export interface CrudSyncDebugResult extends CommandResult {
  success: boolean;

  // Widget data extraction
  widgets: {
    roomList: WidgetSyncData;
    chatWidget: WidgetSyncData;
    userList: WidgetSyncData;
  };

  // Database comparison (if requested)
  database?: {
    rooms: DatabaseSyncData;
    messages: DatabaseSyncData;
    users: DatabaseSyncData;
  };

  // Sync analysis
  syncStatus: {
    roomsMatch: boolean;
    messagesMatch: boolean;
    usersMatch: boolean;
    overallSync: boolean;
  };

  debugging: {
    logs: string[];
    warnings: string[];
    errors: string[];
  };

  error?: string;
}

export const createCrudSyncDebugResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<CrudSyncDebugResult>, 'context' | 'sessionId'>
): CrudSyncDebugResult => createPayload(context, sessionId, {
  success: false,
  widgets: {
    roomList: { widgetName: 'room-list-widget', found: false, path: '', itemCount: 0, items: [] },
    chatWidget: { widgetName: 'chat-widget', found: false, path: '', itemCount: 0, items: [] },
    userList: { widgetName: 'user-list-widget', found: false, path: '', itemCount: 0, items: [] }
  },
  syncStatus: {
    roomsMatch: false,
    messagesMatch: false,
    usersMatch: false,
    overallSync: false
  },
  debugging: {
    logs: [],
    warnings: [],
    errors: []
  },
  ...data
});
/**
 * CrudSyncDebug — Type-safe command executor
 *
 * Usage:
 *   import { CrudSyncDebug } from '...shared/CrudSyncDebugTypes';
 *   const result = await CrudSyncDebug.execute({ ... });
 */
export const CrudSyncDebug = {
  execute(params: CommandInput<CrudSyncDebugParams>): Promise<CrudSyncDebugResult> {
    return Commands.execute<CrudSyncDebugParams, CrudSyncDebugResult>('development/debug/crud-sync', params as Partial<CrudSyncDebugParams>);
  },
  commandName: 'development/debug/crud-sync' as const,
} as const;
