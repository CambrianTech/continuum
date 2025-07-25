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

import { CommandParams, CommandResult, type JTAGContext } from '@shared/JTAGTypes';

/**
 * Generic base parameters for distributed chat operations
 */
export class ChatParams<T extends Record<string, any> = {}> extends CommandParams {
  roomId!: string;
  nodeId?: string;  // Which Continuum node hosts this room (for remote operations)

  constructor(data: Partial<ChatParams<T> & T> = {}) {
    super();
    this.roomId = data.roomId ?? '';
    this.nodeId = data.nodeId;
  }
}

/**
 * Generic base result for distributed chat operations  
 */
export class ChatResult<T extends Record<string, any> = {}> extends CommandResult {
  success!: boolean;
  roomId!: string;
  nodeId?: string;  // Which node actually processed this command
  environment!: JTAGContext['environment'];
  timestamp!: string;
  error?: string;

  constructor(data: Partial<ChatResult<T> & T> & { roomId: string }) {
    super();
    this.success = data.success ?? false;
    this.roomId = data.roomId;
    this.nodeId = data.nodeId;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
    this.error = data.error;
  }
}

/**
 * Distributed participant identification
 */
export interface ChatParticipant {
  id: string;
  type: 'human' | 'ai' | 'system';
  nodeId?: string;  // Which Continuum node this participant is on
}