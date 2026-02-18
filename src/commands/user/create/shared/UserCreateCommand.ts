/**
 * User Create Command - Shared Base Class
 *
 * ARCHITECTURE-RULES.MD compliance:
 * - Shared code is environment-agnostic
 * - No server imports (UserFactory, DataDaemon, etc.)
 * - Server implementation handles actual creation
 * - Browser implementation routes to server
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  UserCreateParams,
  UserCreateResult
} from './UserCreateTypes';

/**
 * Abstract base class for user creation command
 * Server and browser implementations provide actual logic
 */
export abstract class UserCreateCommand extends CommandBase<UserCreateParams, UserCreateResult> {
  public readonly subpath: string = 'commands/user/create';
}
