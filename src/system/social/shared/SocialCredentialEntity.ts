/**
 * SocialCredentialEntity - Stores per-persona social media credentials
 *
 * Each persona can have credentials for multiple platforms.
 * Stored in the persona's longterm.db via ORM (DataCreate/DataList).
 *
 * Credential lifecycle:
 * 1. social/signup creates credential → stored here
 * 2. Commands load credential from here → authenticate provider
 * 3. lastActiveAt updated on each API call
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { BaseEntity } from '@system/data/entities/BaseEntity';
import {
  TextField,
  DateField,
  EnumField,
  JsonField,
  CompositeIndex,
  TEXT_LENGTH,
} from '@system/data/decorators/FieldDecorators';

export type ClaimStatus = 'pending' | 'claimed' | 'unknown';

@CompositeIndex({
  name: 'idx_social_creds_persona_platform',
  fields: ['personaId', 'platformId'],
  unique: true,
})
export class SocialCredentialEntity extends BaseEntity {
  static readonly collection = 'social_credentials';

  get collection(): string {
    return SocialCredentialEntity.collection;
  }

  /** Persona who owns this credential */
  @TextField({ index: true })
  personaId!: UUID;

  /** Platform identifier (e.g., 'moltbook') */
  @TextField({ index: true })
  platformId!: string;

  /** API key / bearer token for the platform */
  @TextField({ maxLength: TEXT_LENGTH.UNLIMITED })
  apiKey!: string;

  /** Username on the platform */
  @TextField({ index: true })
  agentName!: string;

  /** URL to the agent's profile on the platform */
  @TextField({ maxLength: TEXT_LENGTH.UNLIMITED, nullable: true })
  profileUrl?: string;

  /** URL to claim/verify the account (if applicable) */
  @TextField({ maxLength: TEXT_LENGTH.UNLIMITED, nullable: true })
  claimUrl?: string;

  /** Claim/verification status */
  @EnumField({ index: true })
  claimStatus!: ClaimStatus;

  /** When the account was registered */
  @DateField({ index: true })
  registeredAt!: Date;

  /** When the credential was last used for an API call */
  @DateField({ nullable: true })
  lastActiveAt?: Date;

  /** Additional platform-specific metadata */
  @JsonField({ nullable: true })
  metadata?: Record<string, unknown>;

  [key: string]: unknown;

  constructor() {
    super();
    this.personaId = '' as UUID;
    this.platformId = '';
    this.apiKey = '';
    this.agentName = '';
    this.claimStatus = 'pending';
    this.registeredAt = new Date();
  }

  validate(): { success: boolean; error?: string } {
    const errors: string[] = [];

    if (!this.personaId) errors.push('personaId is required');
    if (!this.platformId?.trim()) errors.push('platformId is required');
    if (!this.apiKey?.trim()) errors.push('apiKey is required');
    if (!this.agentName?.trim()) errors.push('agentName is required');

    const validStatuses: ClaimStatus[] = ['pending', 'claimed', 'unknown'];
    if (!validStatuses.includes(this.claimStatus)) {
      errors.push(`claimStatus must be one of: ${validStatuses.join(', ')}`);
    }

    if (errors.length > 0) {
      return { success: false, error: errors.join(', ') };
    }
    return { success: true };
  }

  static override getPaginationConfig() {
    return {
      defaultSortField: 'registeredAt',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 50,
      cursorField: 'registeredAt',
    };
  }
}
