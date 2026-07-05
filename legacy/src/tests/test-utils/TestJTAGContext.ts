/**
 * Test JTAG Context Helper - Create contexts for testing
 */

import { v4 as uuidv4 } from 'uuid';
import type { JTAGContext, JTAGEnvironment } from '../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '../../system/core/types/SystemScopes';

export function createTestJTAGContext(environment: JTAGEnvironment = 'browser'): JTAGContext {
  return {
    uuid: `jtag_test_${uuidv4()}`,
    environment,
    sessionId: SYSTEM_SCOPES.UNKNOWN_SESSION, // Let SessionDaemon assign shared session
    timestamp: Date.now()
  };
}