/**
 * Index Create Command - Abstract Base
 *
 * Low-level primitive for storing a single code entry with embeddings
 */

import { CommandBase } from '../../../../../../daemons/command-daemon/shared/CommandBase';
import type { IndexCreateParams, IndexCreateResult } from './IndexCreateTypes';

export abstract class IndexCreateCommand extends CommandBase<IndexCreateParams, IndexCreateResult> {
  getDescription(): string {
    return 'Store a single code index entry with embeddings in the database';
  }
}
