/**
 * RoomDataManager - Centralized Room Data Management
 * 
 * Provides complete room information to all widgets, eliminating the need for
 * string manipulation and ensuring all components have access to rich metadata
 */

export interface RoomData {
  id: string;
  name: string;
  type: string;
  description: string;
  autoCreated?: boolean;
  metadata?: {
    default?: boolean;
    category?: string;
    icon?: string;
    [key: string]: any;
  };
  participants?: RoomParticipant[];
  messageCount?: number;
  lastActivity?: Date;
}

export interface RoomParticipant {
  id: string;
  name: string;
  type: 'user' | 'ai' | 'system';
  avatar?: string;
  status: 'online' | 'away' | 'offline';
  role?: string;
}

export interface RoomChangeEvent {
  previousRoom: RoomData | null;
  currentRoom: RoomData;
  timestamp: Date;
}

/**
 * Centralized Room Data Manager
 * Single source of truth for all room information across widgets
 */
export class RoomDataManager extends EventTarget {
  private rooms: Map<string, RoomData> = new Map();
  private currentRoom: RoomData | null = null;
  private isInitialized: boolean = false;
  private roomTypeConfig: any = null; // Loaded from JSON
  
  private static instance: RoomDataManager | null = null;
  
  static getInstance(): RoomDataManager {
    if (!RoomDataManager.instance) {
      RoomDataManager.instance = new RoomDataManager();
    }
    return RoomDataManager.instance;
  }
  
  private constructor() {
    super();
    console.log('🏠 RoomDataManager: Initializing centralized room data system');
  }
  
  /**
   * Initialize with room data from server
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      return;
    }
    
    try {
      console.log('🏠 RoomDataManager: Loading room data from server...');
      
      // Load room type configuration first
      await this.loadRoomTypeConfig();
      
      // First try to load from server API
      if ((window as any).continuum && typeof (window as any).continuum.execute === 'function') {
        try {
          const response = await (window as any).continuum.execute('listrooms', {});
          if (response?.rooms) {
            this.loadRoomsFromAPI(response.rooms);
            this.isInitialized = true;
            console.log(`🏠 RoomDataManager: Loaded ${this.rooms.size} rooms from server API`);
            return;
          }
        } catch (error) {
          console.warn('🏠 RoomDataManager: Server API not available, falling back to static data');
        }
      }
      
      // Fallback: Load default rooms from static config
      await this.loadDefaultRooms();
      this.isInitialized = true;
      
      // Set default current room
      if (!this.currentRoom && this.rooms.has('general')) {
        this.setCurrentRoom('general');
      }
      
      console.log(`🏠 RoomDataManager: Initialized with ${this.rooms.size} rooms`);
      
    } catch (error) {
      console.error('🏠 RoomDataManager: Failed to initialize:', error);
      // Create minimal fallback room so widgets don't break
      this.createFallbackRoom();
    }
  }
  
  private async loadRoomTypeConfig(): Promise<void> {
    try {
      const response = await fetch('/src/ui/components/shared/room-type-config.json');
      if (!response.ok) {
        throw new Error(`Failed to load room type config: ${response.status}`);
      }
      
      this.roomTypeConfig = await response.json();
      console.log('🏠 RoomDataManager: Loaded room type configuration');
      
    } catch (error) {
      console.warn('🏠 RoomDataManager: Failed to load room type config, using defaults:', error);
      // Minimal fallback
      this.roomTypeConfig = {
        roomTypes: {
          chat: { displayName: 'Chat Room', icon: '💬' }
        },
        fallbacks: {
          unknownType: { displayName: 'Room', icon: '🏠' }
        }
      };
    }
  }
  
  private loadRoomsFromAPI(roomsData: any[]): void {
    for (const roomData of roomsData) {
      const room: RoomData = {
        id: roomData.id,
        name: roomData.name || this.generateFallbackRoomName(roomData.id, roomData.type),
        type: roomData.type || 'chat',
        description: roomData.description || `Chat room: ${roomData.name || this.generateFallbackRoomName(roomData.id, roomData.type)}`,
        autoCreated: roomData.autoCreated || false,
        metadata: roomData.metadata || {},
        participants: roomData.participants || [],
        messageCount: roomData.messageCount || 0,
        lastActivity: roomData.lastActivity ? new Date(roomData.lastActivity) : new Date()
      };
      
      this.rooms.set(room.id, room);
    }
  }
  
  private async loadDefaultRooms(): Promise<void> {
    try {
      // Load from the same config that ChatRoomDaemon uses
      const response = await fetch('/src/daemons/chatroom/config/default-rooms.json');
      if (!response.ok) {
        throw new Error(`Failed to load default rooms: ${response.status}`);
      }
      
      const config = await response.json();
      
      for (const roomConfig of config.defaultRooms) {
        const room: RoomData = {
          id: roomConfig.id,
          name: roomConfig.name,
          type: roomConfig.type,
          description: roomConfig.description,
          autoCreated: roomConfig.autoCreated || false,
          metadata: {
            ...roomConfig.metadata,
            icon: this.getDefaultRoomIcon(roomConfig.type, roomConfig.id)
          },
          participants: [],
          messageCount: 0,
          lastActivity: new Date()
        };
        
        this.rooms.set(room.id, room);
      }
      
      console.log('🏠 RoomDataManager: Loaded default rooms from config');
      
    } catch (error) {
      console.warn('🏠 RoomDataManager: Failed to load default rooms config:', error);
      this.createFallbackRoom();
    }
  }
  
  private createFallbackRoom(): void {
    const fallbackRoom: RoomData = {
      id: 'general',
      name: 'General Chat',
      type: 'chat',
      description: 'Main chat room for general conversation',
      autoCreated: true,
      metadata: { default: true, category: 'public', icon: '💬' },
      participants: [],
      messageCount: 0,
      lastActivity: new Date()
    };
    
    this.rooms.set('general', fallbackRoom);
    console.log('🏠 RoomDataManager: Created fallback room');
  }
  
  private getDefaultRoomIcon(type: string, id: string): string {
    if (!this.roomTypeConfig) {
      return '🏠'; // Fallback if config not loaded
    }
    
    // Check by type first
    if (this.roomTypeConfig.roomTypes[type]) {
      return this.roomTypeConfig.roomTypes[type].icon;
    }
    
    // Check by ID for legacy rooms
    if (this.roomTypeConfig.roomTypes[id]) {
      return this.roomTypeConfig.roomTypes[id].icon;
    }
    
    // Use unknown type fallback
    return this.roomTypeConfig.fallbacks?.unknownType?.icon || '🏠';
  }
  
  /**
   * Generate a fallback name for rooms that don't have proper names
   * Since room IDs are UUIDs, we can't just capitalize them
   */
  private generateFallbackRoomName(roomId: string, type?: string): string {
    // Check if it looks like a UUID
    if (roomId.match(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i)) {
      // For UUIDs, use JSON config for meaningful names
      if (this.roomTypeConfig?.roomTypes[type || 'chat']) {
        return this.roomTypeConfig.roomTypes[type || 'chat'].displayName;
      }
      
      // Fallback if config not available
      return this.roomTypeConfig?.fallbacks?.unknownType?.displayName || 'Room';
    }
    
    // For legacy string IDs, capitalize first letter
    return roomId.charAt(0).toUpperCase() + roomId.slice(1);
  }
  
  /**
   * Get complete room data by ID
   */
  getRoom(roomId: string): RoomData | null {
    return this.rooms.get(roomId) || null;
  }
  
  /**
   * Get all available rooms
   */
  getAllRooms(): RoomData[] {
    return Array.from(this.rooms.values());
  }
  
  /**
   * Get current room data
   */
  getCurrentRoom(): RoomData | null {
    return this.currentRoom;
  }
  
  /**
   * Set current room and notify all listeners
   */
  setCurrentRoom(roomId: string): boolean {
    const newRoom = this.rooms.get(roomId);
    if (!newRoom) {
      console.warn(`🏠 RoomDataManager: Room '${roomId}' not found`);
      return false;
    }
    
    const previousRoom = this.currentRoom;
    this.currentRoom = newRoom;
    
    const changeEvent: RoomChangeEvent = {
      previousRoom,
      currentRoom: newRoom,
      timestamp: new Date()
    };
    
    console.log(`🏠 RoomDataManager: Room changed: ${previousRoom?.name || 'none'} → ${newRoom.name}`);
    
    // Dispatch both specific room change event and generic change event
    this.dispatchEvent(new CustomEvent('room-changed', { detail: changeEvent }));
    this.dispatchEvent(new CustomEvent('current-room-updated', { detail: newRoom }));
    
    return true;
  }
  
  /**
   * Update room data (e.g., participant count, last activity)
   */
  updateRoom(roomId: string, updates: Partial<RoomData>): void {
    const room = this.rooms.get(roomId);
    if (!room) {
      console.warn(`🏠 RoomDataManager: Cannot update room '${roomId}' - not found`);
      return;
    }
    
    // Merge updates into existing room data
    const updatedRoom = { ...room, ...updates };
    this.rooms.set(roomId, updatedRoom);
    
    // If this is the current room, update current room reference
    if (this.currentRoom?.id === roomId) {
      this.currentRoom = updatedRoom;
      this.dispatchEvent(new CustomEvent('current-room-updated', { detail: updatedRoom }));
    }
    
    console.log(`🏠 RoomDataManager: Updated room '${roomId}'`, updates);
  }
  
  /**
   * Add or update a participant in a room
   */
  updateRoomParticipant(roomId: string, participant: RoomParticipant): void {
    const room = this.rooms.get(roomId);
    if (!room) return;
    
    const participants = room.participants || [];
    const existingIndex = participants.findIndex(p => p.id === participant.id);
    
    if (existingIndex >= 0) {
      participants[existingIndex] = participant;
    } else {
      participants.push(participant);
    }
    
    this.updateRoom(roomId, { participants });
  }
  
  /**
   * Remove a participant from a room
   */
  removeRoomParticipant(roomId: string, participantId: string): void {
    const room = this.rooms.get(roomId);
    if (!room) return;
    
    const participants = (room.participants || []).filter(p => p.id !== participantId);
    this.updateRoom(roomId, { participants });
  }
  
  /**
   * Get welcome message for a room type from JSON config
   */
  getWelcomeMessage(roomType: string): string {
    if (!this.roomTypeConfig) {
      return 'Welcome! Start a conversation.';
    }
    
    const typeConfig = this.roomTypeConfig.roomTypes[roomType];
    if (typeConfig?.defaultWelcomeMessage) {
      return typeConfig.defaultWelcomeMessage;
    }
    
    return this.roomTypeConfig.fallbacks?.unknownType?.defaultWelcomeMessage || 'Welcome! Start a conversation.';
  }
  
  /**
   * Check if room data manager is ready for use
   */
  isReady(): boolean {
    return this.isInitialized && this.rooms.size > 0;
  }
  
  /**
   * Force refresh room data from server
   */
  async refresh(): Promise<void> {
    this.isInitialized = false;
    this.rooms.clear();
    await this.initialize();
  }
}

// Global singleton instance
export const roomDataManager = RoomDataManager.getInstance();