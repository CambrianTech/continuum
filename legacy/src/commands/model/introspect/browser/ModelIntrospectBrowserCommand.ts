/**
 * Model Introspect Command - Browser Implementation
 *
 * Introspect a model — detect architecture, capabilities, and possible forge stages. Returns the model's current state as an alloy-compatible spec. Works from HF cache or API, no weight download needed.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelIntrospectParams, ModelIntrospectResult } from '../shared/ModelIntrospectTypes';

export class ModelIntrospectBrowserCommand extends CommandBase<ModelIntrospectParams, ModelIntrospectResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/introspect', context, subpath, commander);
  }

  async execute(params: ModelIntrospectParams): Promise<ModelIntrospectResult> {
    console.log('🌐 BROWSER: Delegating Model Introspect to server');
    return await this.remoteExecute(params);
  }
}
