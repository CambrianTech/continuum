import { BaseWidget } from '../../shared/BaseWidget';

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
}