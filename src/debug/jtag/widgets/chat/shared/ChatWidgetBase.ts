import { BaseWidget } from '../../shared/BaseWidget';

export abstract class ChatWidgetBase extends BaseWidget {

  protected override resolveResourcePath(filename: string): string {
      // Extract widget directory name from widget name (ChatWidget -> chat)
      const widgetDir = this.config.widgetName.toLowerCase().replace('widget', '');
      // Return relative path from current working directory
      return `widgets/${widgetDir}/chat-widget/${filename}`;
    }
  
    protected async renderWidget(): Promise<void> {
      // Use external template and styles loaded by BaseWidget
      const styles = this.templateCSS ?? '/* No styles loaded */';
      const template = this.templateHTML ?? '<div>No template loaded</div>';
  
      // Ensure template is a string before calling replace
      const templateString = typeof template === 'string' ? template : '<div>Template error</div>';
      
        const dynamicContent = Object.entries(this.getReplacements()).reduce(
            (acc, [placeholder, value]) => acc.replace(placeholder, value),
            templateString
        );
      
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
}