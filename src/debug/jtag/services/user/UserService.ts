/**
 * User Service - Business Logic for User Management
 * 
 * Uses clean API types (BaseUser, HumanUser, PersonaUser, AgentUser) and 
 * existing router/transport system for all operations.
 * 
 * Provides high-level user operations without widgets needing to know
 * about daemon connections or transport details.
 */

import { ServiceBase } from '../shared/ServiceBase';
import type { IServiceTransport } from '../shared/ServiceBase';
import type { JTAGContext } from '../../system/core/types/JTAGTypes';
import type { 
  BaseUser, 
  HumanUser, 
  PersonaUser, 
  AgentUser,
  UserType,
  createHumanUser,
  isHumanUser,
  isPersonaUser, 
  isAgentUser
} from '../../api/types/User';

export interface IUserService {
  // Current user management
  getCurrentUser(): Promise<BaseUser | null>;
  authenticateUser(credentials: { email: string; password: string }): Promise<HumanUser | null>;
  switchUser(userId: string): Promise<BaseUser | null>;
  
  // User creation and management
  createHumanUser(config: { name: string; email: string; avatar?: string }): Promise<HumanUser>;
  createPersonaUser(config: any): Promise<PersonaUser>;  // PersonaConfig from API
  createAgentUser(config: any): Promise<AgentUser>;      // AgentConfig from API
  
  // User queries
  listUsers(userType?: UserType): Promise<BaseUser[]>;
  getUserById(userId: string): Promise<BaseUser | null>;
  searchUsers(query: string): Promise<BaseUser[]>;
  
  // User capabilities and permissions
  getUserCapabilities(user: BaseUser): string[];
  checkUserPermission(user: BaseUser, action: string, resource: string): boolean;
  
  // User state management  
  updateUserStatus(userId: string, status: string): Promise<boolean>;
  updateUserPreferences(userId: string, preferences: Record<string, any>): Promise<boolean>;
}

export class UserService extends ServiceBase implements IUserService {
  private currentUser: BaseUser | null = null;
  private userCache = new Map<string, BaseUser>();

  constructor(transport: IServiceTransport, context: JTAGContext) {
    super('UserService', transport, context);
  }

  async getCurrentUser(): Promise<BaseUser | null> {
    if (this.currentUser) {
      return this.currentUser;
    }

    // Load current user through transport system
    try {
      const result = await this.executeCommand('user/get-current', {});
      if (result.success && result.user) {
        this.currentUser = result.user;
        this.userCache.set(result.user.id, result.user);
        return result.user;
      }
    } catch (error) {
      console.warn(`UserService: Could not load current user:`, error);
    }
    
    return null;
  }

  async authenticateUser(credentials: { email: string; password: string }): Promise<HumanUser | null> {
    const result = await this.executeCommand('user/authenticate', credentials);
    
    if (result.success && result.user && isHumanUser(result.user)) {
      this.currentUser = result.user;
      this.userCache.set(result.user.id, result.user);
      return result.user;
    }
    
    return null;
  }

  async switchUser(userId: string): Promise<BaseUser | null> {
    const result = await this.executeCommand('user/switch', { userId });
    
    if (result.success && result.user) {
      this.currentUser = result.user;
      this.userCache.set(result.user.id, result.user);
      return result.user;
    }
    
    return null;
  }

  async createHumanUser(config: { name: string; email: string; avatar?: string }): Promise<HumanUser> {
    // Use API factory function for consistent user creation
    const user = createHumanUser(config);
    
    // Persist through transport system
    const result = await this.executeCommand('user/create', { user });
    
    if (!result.success) {
      throw new Error(`Failed to create user: ${result.error}`);
    }
    
    this.userCache.set(user.id, user);
    return user;
  }

  async createPersonaUser(config: any): Promise<PersonaUser> {
    // TODO: Import PersonaUser constructor from API types
    throw new Error('PersonaUser creation not implemented yet');
  }

  async createAgentUser(config: any): Promise<AgentUser> {
    // TODO: Import AgentUser constructor from API types  
    throw new Error('AgentUser creation not implemented yet');
  }

  async listUsers(userType?: UserType): Promise<BaseUser[]> {
    const result = await this.executeCommand('user/list', { userType });
    
    if (result.success && result.users) {
      // Cache all users
      result.users.forEach((user: BaseUser) => {
        this.userCache.set(user.id, user);
      });
      return result.users;
    }
    
    return [];
  }

  async getUserById(userId: string): Promise<BaseUser | null> {
    // Check cache first
    if (this.userCache.has(userId)) {
      return this.userCache.get(userId)!;
    }

    // Load from transport
    const result = await this.executeCommand('user/get', { userId });
    
    if (result.success && result.user) {
      this.userCache.set(userId, result.user);
      return result.user;
    }
    
    return null;
  }

  async searchUsers(query: string): Promise<BaseUser[]> {
    const result = await this.executeCommand('user/search', { query });
    
    if (result.success && result.users) {
      return result.users;
    }
    
    return [];
  }

  getUserCapabilities(user: BaseUser): string[] {
    return user.capabilities.filter(cap => cap.enabled).map(cap => cap.name);
  }

  checkUserPermission(user: BaseUser, action: string, resource: string): boolean {
    return user.permissions.some(permission => 
      permission.action === action && 
      (permission.resource === resource || permission.resource === '*') &&
      permission.granted
    );
  }

  async updateUserStatus(userId: string, status: string): Promise<boolean> {
    const result = await this.executeCommand('user/update-status', { userId, status });
    return result.success;
  }

  async updateUserPreferences(userId: string, preferences: Record<string, any>): Promise<boolean> {
    const result = await this.executeCommand('user/update-preferences', { userId, preferences });
    return result.success;
  }
}