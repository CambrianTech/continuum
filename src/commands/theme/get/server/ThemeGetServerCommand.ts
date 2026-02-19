/**
 * ThemeGet Server Command - Handle theme retrieval on server side (pass through to browser)
 */

import type { ThemeGetParams, ThemeGetResult } from '../shared/ThemeGetTypes';
import { createThemeGetResult } from '../shared/ThemeGetTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';

export class ThemeGetServerCommand {
  async execute(params: ThemeGetParams): Promise<ThemeGetResult> {
    console.log(`🎨 ThemeGetServer: Theme information must be retrieved from browser environment`);
    
    // Theme information is browser-specific since it involves DOM inspection
    // Server should pass this through to browser or return informational result
    
    const error = new EnhancementError(
      'theme-get-server',
      'Theme information retrieval is only supported in browser environment where DOM inspection is possible'
    );
    
    return createThemeGetResult(params.context, params.sessionId, {
      success: false,
      currentTheme: 'server-unknown',
      themeApplied: false,
      error
    });
  }
}