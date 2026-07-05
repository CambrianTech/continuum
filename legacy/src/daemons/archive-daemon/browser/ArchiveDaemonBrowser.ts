/**
 * ArchiveDaemon Browser - Browser-specific implementation
 *
 * GENERATED FILE - DO NOT EDIT MANUALLY
 */

import { ArchiveDaemon } from '../shared/ArchiveDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class ArchiveDaemonBrowser extends ArchiveDaemon {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  // Browser-specific overrides go here
}
