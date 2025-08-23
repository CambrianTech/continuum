/**
 * Test JTAG Context Helper - Create contexts for testing
 */

import { v4 as uuidv4 } from 'uuid';
import type { JTAGContext, JTAGEnvironment } from '../../system/core/types/JTAGTypes';

export function createTestJTAGContext(environment: JTAGEnvironment = 'browser'): JTAGContext {
  return {
    uuid: `jtag_test_${uuidv4()}`,
    environment,
    sessionId: `session_${uuidv4()}`,
    timestamp: Date.now()
  };
}