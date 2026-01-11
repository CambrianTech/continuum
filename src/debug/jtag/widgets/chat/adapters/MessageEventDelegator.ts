/**
 * MessageEventDelegator - Single delegated event handler for all chat messages
 *
 * Instead of attaching listeners to each message element (which causes memory leaks
 * when elements are removed), we attach ONE listener to the container and use
 * event bubbling to handle all message interactions.
 *
 * This is the standard pattern used by React, Vue, and all modern frameworks.
 */

// Action handlers by action name
type ActionHandler = (target: HTMLElement, event: Event) => void;

interface DelegatorOptions {
  /** Enable verbose logging */
  verbose?: boolean;
}

/**
 * Event delegator for chat message interactions
 * Attach once to message container, handles all message events efficiently
 */
export class MessageEventDelegator {
  private container: HTMLElement | null = null;
  private clickHandlers = new Map<string, ActionHandler>();
  private hoverHandlers = new Map<string, ActionHandler>();
  private boundClickHandler: (e: Event) => void;
  private boundMouseOverHandler: (e: Event) => void;
  private options: DelegatorOptions;

  constructor(options: DelegatorOptions = {}) {
    this.options = options;
    this.boundClickHandler = this.handleClick.bind(this);
    this.boundMouseOverHandler = this.handleMouseOver.bind(this);
  }

  /**
   * Attach delegator to a container element
   * Call once when ChatWidget initializes
   */
  attach(container: HTMLElement): void {
    if (this.container) {
      this.detach();
    }

    this.container = container;
    container.addEventListener('click', this.boundClickHandler);
    container.addEventListener('mouseover', this.boundMouseOverHandler);

    this.log('Attached to container');
  }

  /**
   * Detach from container - call on widget cleanup
   */
  detach(): void {
    if (this.container) {
      this.container.removeEventListener('click', this.boundClickHandler);
      this.container.removeEventListener('mouseover', this.boundMouseOverHandler);
      this.container = null;
      this.log('Detached from container');
    }
  }

  /**
   * Register a click action handler
   * Elements with data-action="actionName" will trigger this handler
   */
  onAction(actionName: string, handler: ActionHandler): void {
    this.clickHandlers.set(actionName, handler);
  }

  /**
   * Register a hover action handler
   * Elements with data-hover-action="actionName" will trigger this handler
   */
  onHover(actionName: string, handler: ActionHandler): void {
    this.hoverHandlers.set(actionName, handler);
  }

  /**
   * Remove an action handler
   */
  offAction(actionName: string): void {
    this.clickHandlers.delete(actionName);
  }

  /**
   * Handle click events via delegation
   */
  private handleClick(event: Event): void {
    const target = event.target as HTMLElement;

    // Find closest element with data-action attribute, scoped to message-row boundary
    const actionElement = target.closest('.message-row [data-action], [data-action]') as HTMLElement;
    if (!actionElement) return;

    const action = actionElement.dataset.action;
    if (!action) return;

    const handler = this.clickHandlers.get(action);
    if (handler) {
      event.stopPropagation();
      this.log(`Action: ${action}`);
      handler(actionElement, event);
    }
  }

  /**
   * Handle mouseover events via delegation
   */
  private handleMouseOver(event: Event): void {
    const target = event.target as HTMLElement;

    const hoverElement = target.closest('[data-hover-action]') as HTMLElement;
    if (!hoverElement) return;

    const action = hoverElement.dataset.hoverAction;
    if (!action) return;

    const handler = this.hoverHandlers.get(action);
    if (handler) {
      handler(hoverElement, event);
    }
  }

  /**
   * Get data from the message element containing this target
   */
  static getMessageData(target: HTMLElement): { messageId?: string; contentType?: string } {
    const messageEl = target.closest('[data-entity-id]') as HTMLElement;
    const contentEl = target.closest('[data-content-type]') as HTMLElement;

    return {
      messageId: messageEl?.dataset.entityId,
      contentType: contentEl?.dataset.contentType
    };
  }

  /**
   * Get specific data attribute from target or ancestors
   */
  static getData(target: HTMLElement, key: string): string | undefined {
    // Check target first
    if (target.dataset[key]) return target.dataset[key];

    // Walk up to find data
    const withData = target.closest(`[data-${key}]`) as HTMLElement;
    return withData?.dataset[key];
  }

  private log(message: string): void {
    if (this.options.verbose) {
      console.log(`📨 MessageEventDelegator: ${message}`);
    }
  }
}

/**
 * Singleton instance for chat widget
 * Multiple chat widgets can create their own instances if needed
 */
export const chatEventDelegator = new MessageEventDelegator();
