/**
 * Chat Service - Business Logic for Chat Operations
 * 
 * Uses clean API types and existing router/transport system.
 * Provides high-level business operations for chat functionality.
 * 
 * NO hardcoded daemon connections - uses transport abstraction.
 * NO magic constants - uses typed API parameters.
 */

import { ServiceBase } from '../shared/ServiceBase';
import type { IServiceTransport } from '../shared/ServiceBase';
import type { JTAGContext } from '../../system/core/types/JTAGTypes';
import type { BaseUser } from '../../api/types/User';
import type { 
  SendMessageParams,
  SendMessageResult,
  CreateRoomParams,
  CreateRoomResult,
  JoinRoomParams,
  JoinRoomResult,
  ListRoomsParams,
  ListRoomsResult
} from '../../api/commands/chat/ChatCommands';

export interface IChatService {
  sendMessage(params: SendMessageParams): Promise<SendMessageResult>;
  createRoom(params: CreateRoomParams): Promise<CreateRoomResult>;
  joinRoom(params: JoinRoomParams): Promise<JoinRoomResult>;
  leaveRoom(roomId: string, user: BaseUser): Promise<{ success: boolean }>;
  listRooms(params?: ListRoomsParams): Promise<ListRoomsResult>;
}

export class ChatService extends ServiceBase implements IChatService {
  constructor(transport: IServiceTransport, context: JTAGContext) {
    super('ChatService', transport, context);
  }

  async sendMessage(params: SendMessageParams): Promise<SendMessageResult> {
    // Validate required parameters  
    if (!params.content?.text?.trim()) {
      throw new Error('Message content is required');
    }
    if (!params.roomId) {
      throw new Error('Room ID is required');
    }
    if (!params.sender) {
      throw new Error('Sender information is required');
    }

    // Use existing chat/send command through transport
    return await this.executeCommand('collaboration/chat/send', params);
  }

  async createRoom(params: CreateRoomParams): Promise<CreateRoomResult> {
    if (!params.name?.trim()) {
      throw new Error('Room name is required');
    }
    if (!params.creator) {
      throw new Error('Room creator is required');
    }

    // Use existing daemon system through transport
    return await this.executeCommand('collaboration/chat/create-room', params);
  }

  async joinRoom(params: JoinRoomParams): Promise<JoinRoomResult> {
    if (!params.roomId) {
      throw new Error('Room ID is required');
    }
    if (!params.user) {
      throw new Error('User is required');
    }

    return await this.executeCommand('collaboration/chat/join-room', params);
  }

  async leaveRoom(roomId: string, user: BaseUser): Promise<{ success: boolean }> {
    const params = { roomId, user };
    const result = await this.executeCommand('collaboration/chat/leave-room', params);
    return { success: result.success };
  }

  async listRooms(params: ListRoomsParams = {}): Promise<ListRoomsResult> {
    return await this.executeCommand('collaboration/chat/list-rooms', params);
  }
}