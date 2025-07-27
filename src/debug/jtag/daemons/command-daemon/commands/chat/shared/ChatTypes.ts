// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Chat Command Base Types - Distributed P2P Room Architecture
 * 
 * Designed for distributed rooms across Continuum nodes, not centralized chat servers.
 * Rooms exist on individual Continuum instances with P2P coordination.
 * 
 * DISTRIBUTED ARCHITECTURE:
 * - roomId identifies a room on a specific Continuum node
 * - nodeId identifies which Continuum instance hosts the room
 * - Remote routing: /remote/{nodeId}/chat/{command} for cross-node operations
 * - UDP multicast for node discovery and room announcements
 * 
 * CORE PATTERNS:
 * ✅ roomId + nodeId for distributed room identification
 * ✅ Remote-aware parameter and result structures
 * ✅ Location-transparent command execution
 */

import { CommandParams, CommandResult, type JTAGContext, createPayload } from '@shared/JTAGTypes';
import { UUID } from 'crypto';

/**
 * Generic base parameters for distributed chat operations
 */
export interface ChatParams<T extends Record<string, any> = {}> extends CommandParams {
  roomId: string;
  nodeId?: string;  // Which Continuum node hosts this room (for remote operations)
}

export const createChatParams = <T extends Record<string, any> = {}>(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatParams<T> & T>, 'context' | 'sessionId'>
): ChatParams<T> => createPayload(context, sessionId, {
  roomId: data.roomId ?? '',
  nodeId: data.nodeId,
  ...data
});

/**
 * Generic base result for distributed chat operations  
 */
export interface ChatResult<T extends Record<string, any> = {}> extends CommandResult {
  success: boolean;
  roomId: string;
  nodeId?: string;  // Which node actually processed this command
  timestamp: string;
  error?: string;
}

export const createChatResult = <T extends Record<string, any> = {}>(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatResult<T> & T>, 'context' | 'sessionId'> & { roomId: string }
): ChatResult<T> => createPayload(context, sessionId, {
  success: data.success ?? false,
  roomId: data.roomId,
  nodeId: data.nodeId,
  timestamp: data.timestamp ?? new Date().toISOString(),
  error: data.error,
  ...data
});

/**
 * Distributed participant identification
 */
export interface ChatParticipant {
  id: string;
  type: 'human' | 'ai' | 'system';
  nodeId?: string;  // Which Continuum node this participant is on
}