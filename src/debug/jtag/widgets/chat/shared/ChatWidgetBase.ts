import { BaseWidget } from '../../shared/BaseWidget';

export abstract class ChatWidgetBase extends BaseWidget {
  
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