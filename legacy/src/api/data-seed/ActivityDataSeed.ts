/**
 * Activity Data Seeding - Centralized Collaborative Activity Creation
 *
 * Creates initial activities using proper ActivityEntity structure.
 * Uses JTAG data commands and stable uniqueId constants.
 *
 * Activities are distinct from Rooms:
 * - Rooms = chat channels (RoomEntity)
 * - Activities = collaborative content (ActivityEntity)
 *   - Canvas drawing sessions
 *   - Browser co-browsing
 *   - Game sessions
 *   - etc.
 */

import { ActivityEntity, type ActivityParticipant, type ActivityState, type ActivityConfig } from '../../system/data/entities/ActivityEntity';
import { ACTIVITY_UNIQUE_IDS } from '../../system/data/constants/ActivityConstants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { COLLECTIONS } from '../../system/data/config/DatabaseConfig';

export interface ActivitySeedData {
  readonly activities: readonly ActivityEntity[];
  readonly totalCount: number;
  readonly createdAt: string;
}

export class ActivityDataSeed {
  private static readonly COLLECTION = COLLECTIONS.ACTIVITIES;

  /**
   * Generate seed activities using ActivityEntity structure with stable uniqueIds
   * @param humanUserId - The userId of the system owner (from SystemIdentity)
   */
  public static generateSeedActivities(humanUserId: UUID): ActivitySeedData {
    const now = new Date();
    const activities: ActivityEntity[] = [];

    // Main canvas - the default collaborative drawing canvas
    const canvasMain = new ActivityEntity();
    canvasMain.uniqueId = ACTIVITY_UNIQUE_IDS.CANVAS_MAIN;
    canvasMain.displayName = 'Main Canvas';
    canvasMain.description = 'The primary collaborative drawing canvas where humans and AIs can draw together';
    canvasMain.recipeId = 'canvas'; // Canvas recipe with vision AI pipeline
    canvasMain.status = 'active';
    canvasMain.ownerId = humanUserId;
    canvasMain.startedAt = now;
    canvasMain.lastActivityAt = now;
    canvasMain.participants = [
      {
        userId: humanUserId,
        role: 'owner',
        joinedAt: now,
        isActive: true
      }
    ];
    canvasMain.state = {
      phase: 'active',
      progress: 0,
      variables: {
        canvasWidth: 800,
        canvasHeight: 600,
        backgroundColor: '#1a1a2e',
        zoomLevel: 1,
        strokeCount: 0
      },
      updatedAt: now
    };
    canvasMain.config = {
      settings: {
        allowAnonymous: false,
        autoSave: true,
        saveIntervalMs: 30000
      }
    };
    canvasMain.tags = ['canvas', 'drawing', 'collaborative'];
    activities.push(canvasMain);

    // Main browser - the default co-browsing session
    const browserMain = new ActivityEntity();
    browserMain.uniqueId = ACTIVITY_UNIQUE_IDS.BROWSER_MAIN;
    browserMain.displayName = 'Co-Browser';
    browserMain.description = 'Collaborative web browsing session where AIs can see what you browse';
    browserMain.recipeId = 'browser'; // Browser recipe (if exists)
    browserMain.status = 'active';
    browserMain.ownerId = humanUserId;
    browserMain.startedAt = now;
    browserMain.lastActivityAt = now;
    browserMain.participants = [
      {
        userId: humanUserId,
        role: 'owner',
        joinedAt: now,
        isActive: true
      }
    ];
    browserMain.state = {
      phase: 'active',
      progress: 0,
      variables: {
        currentUrl: '',
        urlHistory: []
      },
      updatedAt: now
    };
    browserMain.config = {
      settings: {
        allowNavigation: true,
        captureScreenshots: true
      }
    };
    browserMain.tags = ['browser', 'co-browsing', 'collaborative'];
    activities.push(browserMain);

    return {
      activities: activities as readonly ActivityEntity[],
      totalCount: activities.length,
      createdAt: now.toISOString()
    };
  }

  /**
   * Create JTAG data/store command for activity (uses entity validation)
   */
  public static createActivityStoreData(activity: ActivityEntity): ActivityEntity {
    const validation = activity.validate();
    if (!validation.success) {
      throw new Error(`Activity validation failed: ${validation.error}`);
    }
    return activity;
  }
}

export default ActivityDataSeed;
