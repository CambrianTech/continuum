/**
 * Content Open Command Types
 *
 * Opens content and adds it to user's openItems array.
 * Emits content:opened event for widgets to respond to.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ContentType } from '@system/data/entities/UserStateEntity';

export interface ContentOpenParams extends CommandParams {
  readonly userId: UUID;             // User ID (REQUIRED - infrastructure should inject from session)
  readonly contentType: ContentType;
  readonly entityId?: UUID;          // ID of the room/document/etc being opened (optional for singletons like settings)
  readonly title: string;             // Display title for the tab
  readonly subtitle?: string;         // Optional subtitle
  readonly priority?: 'low' | 'normal' | 'high' | 'urgent';
  readonly setAsCurrent?: boolean;    // Set as currentItemId (default: true)
  readonly metadata?: Record<string, unknown>;
}

export interface ContentOpenResult extends CommandResult {
  readonly success: boolean;
  readonly contentItemId: UUID;       // ID of the content item created
  readonly currentItemId?: UUID;      // Current item after operation
  readonly openItemsCount: number;    // Total open items after operation
  readonly error?: string;            // Error message if failed
}

export interface ContentOpenedEvent {
  readonly contentItemId: UUID;
  readonly contentType: ContentType;
  readonly entityId?: UUID;          // Optional for singletons like settings
  readonly title: string;
  readonly userId: UUID;
  readonly currentItemId?: UUID;
  readonly setAsCurrent?: boolean;   // Whether this content should be displayed immediately
}
