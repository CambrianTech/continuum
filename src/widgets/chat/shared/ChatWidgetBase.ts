import { BaseWidget } from '../../shared/BaseWidget';

/**
 * Smart path resolution for chat widgets
 * More extensible - automatically infers paths from widget names
 */
function inferChatWidgetPath(widgetName: string, filename: string): string {
  // Convert "UserListWidget" -> "user-list"
  // Convert "ChatWidget" -> "chat-widget"
  // Convert "RoomListWidget" -> "room-list"
  const widgetDir = widgetName
    .replace(/Widget$/, '') // Remove "Widget" suffix first
    .split(/(?=[A-Z])/) // Split on capital letters: ["User", "List"] or ["Chat"]
    .map(part => part.toLowerCase()) // lowercase each part
    .join('-'); // join with hyphens

  return `widgets/chat/${widgetDir}/${filename}`;
}

export abstract class ChatWidgetBase extends BaseWidget {
  
    protected async renderWidget(): Promise<void> {
      // Use external template and styles loaded by BaseWidget
      const styles = this.templateCSS ?? '/* No styles loaded */';

      // Check if widget uses template literals (renderTemplate method) or external template files
      let dynamicContent: string;
      if (!this.config.template && 'renderTemplate' in this) {
        // Use template literal from renderTemplate() method
        dynamicContent = (this as any).renderTemplate();
      } else {
        // Use external template file with placeholder replacements
        const template = this.templateHTML ?? '<div>No template loaded</div>';
        const templateString = typeof template === 'string' ? template : '<div>Template error</div>';

        dynamicContent = Object.entries(this.getReplacements()).reduce(
          (acc, [placeholder, value]) => acc.replace(placeholder, value),
          templateString
        );
      }
      
      this.shadowRoot.innerHTML = `
        <style>${styles}</style>
        ${dynamicContent}
      `;
      
      // Setup event listeners
      this.cleanupEventListeners();
      this.setupEventListeners();      
    }


    protected setupEventListeners(): void {
      
    }

    protected cleanupEventListeners(): void {
        
    }

    protected getReplacements(): Record<string, string> {
        return {};
    }

    /**
     * Smart default path resolution - widgets can override for custom paths
     * More extensible: automatically infers from widget class name
     */
    protected override resolveResourcePath(filename: string): string {
        return inferChatWidgetPath(this.config.widgetName, filename);
    }

}