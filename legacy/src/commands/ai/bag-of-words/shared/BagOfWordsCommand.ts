/**
 * Bag of Words Command - Shared Base Class
 *
 * ARCHITECTURE-RULES.MD compliance:
 * - Shared code is environment-agnostic
 * - No server imports (DataDaemon, etc.)
 * - Server implementation handles actual orchestration
 * - Browser implementation routes to server
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  BagOfWordsParams,
  BagOfWordsResult
} from './BagOfWordsTypes';

/**
 * Abstract base class for bag-of-words command
 * Server implementation provides actual conversation orchestration logic
 */
export abstract class BagOfWordsCommand extends CommandBase<BagOfWordsParams, BagOfWordsResult> {
  public readonly subpath: string = 'commands/ai/bag-of-words';
}
