/**
 * UserFactory - Factory for creating User instances
 *
 * ARCHITECTURE-RULES.md compliance:
 * - Static imports only (no dynamic imports)
 * - Separate from BaseUser to avoid circular dependencies
 * - Type-safe factory pattern
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGRouter } from '../../core/router/shared/JTAGRouter';
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import type { BaseUser } from './BaseUser';
import { PersonaUser } from './PersonaUser';
import { AgentUser } from './AgentUser';
import { HumanUser } from './HumanUser';

export class UserFactory {
  static async create(
    params: UserCreateParams,
    context: JTAGContext,
    router: JTAGRouter
  ): Promise<BaseUser> {
    switch (params.type) {
      case 'persona':
        return await PersonaUser.create(params, context, router);
      case 'agent':
        return await AgentUser.create(params, context, router);
      case 'human':
        return await HumanUser.create(params, context, router);
      default:
        throw new Error(`Unknown user type: ${params.type}`);
    }
  }
}
