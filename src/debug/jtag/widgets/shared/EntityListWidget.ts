/**
 * EntityListWidget - Pure extension of BaseWidget
 * Domain-agnostic - works for any entity lists, not just chat-related
 */

import { BaseWidget, type WidgetConfig } from './BaseWidget';

export abstract class EntityListWidget<T> extends BaseWidget {

  constructor(config: WidgetConfig) {
    super(config);
  }

  // Standardized header structure with entity-driven content
  protected renderHeader(entity?: T): string {
    return `
      <div class="entity-list-header">
        <span class="header-title">${this.getEntityTitle(entity)}</span>
        <span class="list-count">${this.getEntityCount()}</span>
      </div>
    `;
  }

  // Optional footer - empty by default, widgets can override if needed
  protected renderFooter(): string {
    return '';
  }

  // Each widget defines its title based on entity context
  protected abstract getEntityTitle(entity?: T): string;

  // Widgets implement their own count logic:
  // - Room/User: total items (equals loaded since no pagination)
  // - Chat: loaded items (subset of total via infinite scroll)
  protected abstract getEntityCount(): number;

  // Shared count update logic
  protected updateEntityCount(): void {
    const countElement = this.shadowRoot.querySelector('.list-count');
    if (countElement) {
      countElement.textContent = this.getEntityCount().toString();
    }
  }

  // Template rendering implementation - similar to ChatWidgetBase pattern
  protected async renderWidget(): Promise<void> {
    // Use external template and styles loaded by BaseWidget
    const styles = this.templateCSS ?? '/* No styles loaded */';

    // Check if widget uses template literals (renderTemplate method) or external template files
    let dynamicContent: string;
    if (!this.config.template && 'renderTemplate' in this) {
      // Use template literal from renderTemplate() method
      dynamicContent = (this as unknown as { renderTemplate(): string }).renderTemplate();
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
    // Default implementation - subclasses can override
  }

  protected cleanupEventListeners(): void {
    // Default implementation - subclasses can override
  }

  protected getReplacements(): Record<string, string> {
    return {};
  }

  // Subclasses must provide their own path resolution
  protected abstract resolveResourcePath(filename: string): string;
}