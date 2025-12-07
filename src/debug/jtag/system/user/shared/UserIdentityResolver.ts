/**
 * User Identity Resolver
 *
 * Like navigator.userAgent → user identity mapping for JTAG citizens.
 * Uses AgentDetector to identify who/what is connecting, then resolves
 * to appropriate user identity (lookup existing or create new).
 *
 * CRITICAL: Always lookup BEFORE creating to prevent ghost users.
 */

import { AgentDetector, type AgentInfo } from '../../core/detection/AgentDetector';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import type { UserEntity } from '../../data/entities/UserEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../core/types/CrossPlatformUUID';

/**
 * Resolved user identity with well-typed fields
 */
export interface ResolvedUserIdentity {
  /** Unique identifier for user (stable across sessions) */
  uniqueId: string;

  /** User ID if found in database, undefined if needs creation */
  userId?: UUID;

  /** Display name for chat/UI */
  displayName: string;

  /** Short name for compact display */
  shortName: string;

  /** JTAG citizen type */
  type: 'human' | 'agent' | 'persona';

  /** Whether user exists in database */
  exists: boolean;

  /** Agent detection info that led to this resolution */
  agentInfo: AgentInfo;

  /** Bio/description for new user creation */
  bio?: string;

  /** Avatar for new user creation */
  avatar?: string;
}

/**
 * User Identity Resolver - Maps AgentDetector → User Identity
 */
export class UserIdentityResolver {

  /**
   * Resolve current connection to user identity
   * Uses AgentDetector to identify agent, then looks up existing user
   *
   * @param overrides - Optional explicit agent info for testing/impersonation
   * @returns Resolved user identity with lookup result
   */
  static async resolve(overrides?: Partial<AgentInfo>): Promise<ResolvedUserIdentity> {
    // STEP 1: Detect agent (or use overrides)
    const detectedAgent = AgentDetector.detect();
    const agentInfo = overrides ? { ...detectedAgent, ...overrides } : detectedAgent;

    // STEP 2: Generate stable uniqueId based on agent type
    const uniqueId = this.generateUniqueId(agentInfo);

    // STEP 3: Lookup existing user by uniqueId
    const existingUser = await this.lookupUserByUniqueId(uniqueId);

    // STEP 4: Build resolved identity
    if (existingUser) {
      const resolved: ResolvedUserIdentity = {
        uniqueId,
        userId: existingUser.id,
        displayName: existingUser.displayName,
        shortName: (typeof existingUser.shortName === 'string' ? existingUser.shortName : this.generateShortName(agentInfo)),
        type: this.mapAgentTypeToUserType(agentInfo.type),
        exists: true,
        agentInfo
      };

      // Only add bio/avatar if they're actual strings
      if (typeof existingUser.bio === 'string') {
        resolved.bio = existingUser.bio;
      }
      if (typeof existingUser.avatar === 'string') {
        resolved.avatar = existingUser.avatar;
      }

      return resolved;
    }

    // User doesn't exist - return identity for creation
    return {
      uniqueId,
      userId: undefined,
      displayName: this.generateDisplayName(agentInfo),
      shortName: this.generateShortName(agentInfo),
      type: this.mapAgentTypeToUserType(agentInfo.type),
      exists: false,
      agentInfo,
      bio: this.generateBio(agentInfo),
      avatar: this.generateAvatar(agentInfo)
    };
  }

  /**
   * Generate stable uniqueId based on agent detection
   *
   * CRITICAL: uniqueId must be stable across sessions for same agent
   * - Claude Code: "claude-code" (constant)
   * - Human: "primary-human" (default) or "human-{username}"
   * - CI: "ci-{platform}"
   * - Generic automation: "automation-{hash}"
   */
  private static generateUniqueId(agentInfo: AgentInfo): string {
    switch (agentInfo.type) {
      case 'ai':
        // Match seed script uniqueId format: generateUniqueId('Claude') → 'claude'
        // Strip common suffixes like "Code", "CLI", "Assistant" for cleaner IDs
        if (agentInfo.name === 'Claude Code') {
          return 'claude'; // Matches seed: generateUniqueId('Claude')
        }
        if (agentInfo.name === 'ChatGPT CLI') {
          return 'chatgpt'; // Matches seed: generateUniqueId('ChatGPT')
        }
        if (agentInfo.name === 'GitHub Copilot') {
          return 'copilot'; // Matches seed: generateUniqueId('Copilot')
        }
        // Generic AI agents: extract first word before space/dash
        // "Claude Code" → "claude", "GPT-4" → "gpt", "Gemini Pro" → "gemini"
        const firstWord = agentInfo.name.split(/[\s-]/)[0];
        return this.sanitizeForId(firstWord);

      case 'human': {
        // Use environment username if available
        const username = process.env.USER ?? process.env.USERNAME;
        if (username) {
          return this.sanitizeForId(username); // Matches seed: generateUniqueId('Joel')
        }
        // Fallback to primary human
        return 'joel'; // Matches seed: generateUniqueId('Joel')
      }

      case 'ci':
        return `ci-${this.sanitizeForId(agentInfo.name)}`;

      case 'automation':
        return `automation-${this.sanitizeForId(agentInfo.name)}`;

      default:
        return `unknown-${generateUUID()}`;
    }
  }

  /**
   * Lookup existing user by uniqueId
   */
  private static async lookupUserByUniqueId(uniqueId: string): Promise<UserEntity | null> {
    try {
      const result = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: { uniqueId },
        limit: 1
      });

      if (result.success && result.data && result.data.length > 0) {
        // Extract UserEntity from DataRecord
        return {
          ...result.data[0].data,
          id: result.data[0].id
        } as UserEntity;
      }

      return null;
    } catch (error) {
      console.warn(`⚠️ UserIdentityResolver: Failed to lookup user by uniqueId ${uniqueId}:`, error);
      return null;
    }
  }

  /**
   * Generate display name from agent info
   */
  private static generateDisplayName(agentInfo: AgentInfo): string {
    switch (agentInfo.type) {
      case 'ai':
        return agentInfo.name; // "Claude Code", "ChatGPT CLI", etc.

      case 'human': {
        const username = process.env.USER ?? process.env.USERNAME;
        return username ? this.capitalize(username) : 'Developer';
      }

      case 'ci':
        return agentInfo.name; // "GitHub Actions", "GitLab CI", etc.

      case 'automation':
        return 'Automation Script';

      default:
        return 'Unknown User';
    }
  }

  /**
   * Generate short name for compact display
   */
  private static generateShortName(agentInfo: AgentInfo): string {
    switch (agentInfo.type) {
      case 'ai':
        if (agentInfo.name === 'Claude Code') return 'Claude';
        if (agentInfo.name === 'ChatGPT CLI') return 'GPT';
        if (agentInfo.name === 'GitHub Copilot') return 'Copilot';
        return agentInfo.name.split(' ')[0]; // First word

      case 'human': {
        const username = process.env.USER ?? process.env.USERNAME;
        return username ? username.substring(0, 8) : 'Dev';
      }

      case 'ci':
        return 'CI';

      case 'automation':
        return 'Bot';

      default:
        return '?';
    }
  }

  /**
   * Map AgentInfo type to UserEntity type
   */
  private static mapAgentTypeToUserType(agentType: AgentInfo['type']): 'human' | 'agent' | 'persona' {
    switch (agentType) {
      case 'human':
        return 'human';

      case 'ai':
      case 'ci':
      case 'automation':
        return 'agent';

      default:
        return 'human'; // Fallback to human for unknown
    }
  }

  /**
   * Generate bio description
   */
  private static generateBio(agentInfo: AgentInfo): string {
    switch (agentInfo.type) {
      case 'ai':
        if (agentInfo.name === 'Claude Code') {
          return 'AI coding assistant powered by Anthropic Claude';
        }
        return `AI assistant (${agentInfo.name})`;

      case 'human':
        return 'Software developer';

      case 'ci':
        return `CI/CD system (${agentInfo.name})`;

      case 'automation':
        return 'Automation script';

      default:
        return 'Unknown user type';
    }
  }

  /**
   * Generate avatar emoji
   */
  private static generateAvatar(agentInfo: AgentInfo): string {
    switch (agentInfo.type) {
      case 'ai':
        return '🤖';

      case 'human':
        return '👤';

      case 'ci':
        return '🔄';

      case 'automation':
        return '⚙️';

      default:
        return '❓';
    }
  }

  /**
   * Sanitize string for use in IDs
   */
  private static sanitizeForId(str: string): string {
    return str
      .toLowerCase()
      .replace(/[^a-z0-9-]/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '');
  }

  /**
   * Capitalize first letter
   */
  private static capitalize(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
  }

  /**
   * Resolve for impersonation (AI pretending to be human)
   */
  static async resolveAsHuman(): Promise<ResolvedUserIdentity> {
    return this.resolve({
      name: 'Human Developer',
      type: 'human',
      capabilities: {
        supportsColors: true,
        prefersStructuredData: false,
        supportsInteractivity: true
      },
      outputFormat: 'human',
      confidence: 1.0
    });
  }

  /**
   * Resolve for testing with explicit identity
   */
  static async resolveWithIdentity(
    uniqueId: string,
    displayName: string,
    type: 'human' | 'agent' | 'persona'
  ): Promise<ResolvedUserIdentity> {
    const existingUser = await this.lookupUserByUniqueId(uniqueId);

    if (existingUser) {
      const resolved: ResolvedUserIdentity = {
        uniqueId,
        userId: existingUser.id,
        displayName: existingUser.displayName,
        shortName: (typeof existingUser.shortName === 'string' ? existingUser.shortName : displayName.split(' ')[0]),
        type,
        exists: true,
        agentInfo: AgentDetector.detect()
      };

      // Only add bio/avatar if they're actual strings
      if (typeof existingUser.bio === 'string') {
        resolved.bio = existingUser.bio;
      }
      if (typeof existingUser.avatar === 'string') {
        resolved.avatar = existingUser.avatar;
      }

      return resolved;
    }

    return {
      uniqueId,
      userId: undefined,
      displayName,
      shortName: displayName.split(' ')[0],
      type,
      exists: false,
      agentInfo: AgentDetector.detect()
    };
  }
}
