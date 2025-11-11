/**
 * Activity User Presence Server Command
 *
 * Phase 3bis: Browser tab visibility integration
 * Updates ChatCoordinationStream temperature when user focus changes
 */

import type { ActivityUserPresentParams, ActivityUserPresentResult } from '../shared/ActivityUserPresentTypes';
import { getChatCoordinator } from '../../../../system/coordination/server/ChatCoordinationStream';

export async function execute(params: ActivityUserPresentParams): Promise<ActivityUserPresentResult> {
  const coordinator = getChatCoordinator();

  // Update presence and temperature
  coordinator.onUserPresent(params.activityId, params.present);

  // Get new temperature
  const temperature = coordinator.getTemperature(params.activityId);

  return {
    activityId: params.activityId,
    present: params.present,
    temperature,
    timestamp: Date.now(),
    context: params.context!,
    sessionId: params.sessionId
  };
}
