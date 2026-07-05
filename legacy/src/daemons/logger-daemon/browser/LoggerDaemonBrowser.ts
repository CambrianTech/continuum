/**
 * LoggerDaemon Browser - Browser-specific implementation
 *
 * GENERATED FILE - DO NOT EDIT MANUALLY
 */

import { LoggerDaemon } from '../shared/LoggerDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class LoggerDaemonBrowser extends LoggerDaemon {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  // Browser-specific overrides go here
}
