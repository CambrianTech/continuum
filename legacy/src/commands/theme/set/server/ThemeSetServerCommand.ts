/**
 * ThemeSet Server Command - Handle theme switching on server side (pass through to browser)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { ThemeSetParams, ThemeSetResult } from '../shared/ThemeSetTypes';

export class ThemeSetServerCommand extends CommandBase<ThemeSetParams, ThemeSetResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('theme/set', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<ThemeSetResult> {
    const themeParams = params as ThemeSetParams;
    
    console.log(`🔧 CLAUDE-THEME-DEBUG: ThemeSetServer received params:`, JSON.stringify(params, null, 2));
    console.log(`🔧 CLAUDE-THEME-DEBUG: themeParams.themeName:`, themeParams.themeName);
    console.log(`🔧 CLAUDE-THEME-DEBUG: Raw params type:`, typeof params);
    console.log(`🔧 CLAUDE-THEME-DEBUG: Raw params keys:`, Object.keys(params));
    
    console.log(`🎨 ThemeSetServer: Delegating theme setting to browser environment`);
    
    // Theme setting is a browser-only operation since it affects DOM and CSS
    // Delegate to browser environment automatically
    return await this.remoteExecute(themeParams);
  }
}