/**
 * Interface Interact Command - Browser Implementation
 *
 * Interact with UI elements: click, type, select, scroll, focus, clear, check.
 * Supports shadow DOM piercing via >> separator in selectors.
 * Runs in browser context — direct DOM access.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfaceInteractParams, InterfaceInteractResult } from '../shared/InterfaceInteractTypes';
import { createInterfaceInteractResultFromParams } from '../shared/InterfaceInteractTypes';

export class InterfaceInteractBrowserCommand extends CommandBase<InterfaceInteractParams, InterfaceInteractResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/interact', context, subpath, commander);
  }

  async execute(params: InterfaceInteractParams): Promise<InterfaceInteractResult> {
    const action = params.action?.toLowerCase() ?? '';
    const selector = params.selector ?? '';

    if (!action || !selector) {
      return this.fail(params, false, `Missing required parameter: ${!action ? 'action' : 'selector'}`);
    }

    // Find element (inside try/catch for invalid CSS selectors)
    let element: Element | null;
    try {
      element = this.querySelector(selector);
    } catch (err) {
      return this.fail(params, false, `Invalid selector: ${err instanceof Error ? err.message : String(err)}`);
    }

    if (!element) {
      return this.fail(params, false, `No element found for selector: ${selector}`);
    }

    const elementTag = element.tagName.toLowerCase();
    const elementText = (element.textContent ?? '').trim().slice(0, 200);
    const previousValue = this.getElementValue(element);

    try {
      switch (action) {
        case 'click':
          (element as HTMLElement).click();
          break;

        case 'type': {
          const input = element as HTMLInputElement | HTMLTextAreaElement;
          input.focus();
          input.value = params.value ?? '';
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
          break;
        }

        case 'select': {
          const select = element as HTMLSelectElement;
          select.value = params.value ?? '';
          select.dispatchEvent(new Event('change', { bubbles: true }));
          break;
        }

        case 'scroll': {
          const dir = params.direction ?? 'down';
          const amount = params.amount ?? 300;
          const scrollMap: Record<string, [number, number]> = {
            up: [0, -amount],
            down: [0, amount],
            left: [-amount, 0],
            right: [amount, 0],
          };
          const [x, y] = scrollMap[dir] ?? [0, amount];
          element.scrollBy({ left: x, top: y, behavior: 'smooth' });
          break;
        }

        case 'focus':
          (element as HTMLElement).focus();
          break;

        case 'clear': {
          const clearTarget = element as HTMLInputElement | HTMLTextAreaElement;
          clearTarget.value = '';
          clearTarget.dispatchEvent(new Event('input', { bubbles: true }));
          clearTarget.dispatchEvent(new Event('change', { bubbles: true }));
          break;
        }

        case 'check': {
          const checkbox = element as HTMLInputElement;
          checkbox.checked = params.value === 'false' ? false : true;
          checkbox.dispatchEvent(new Event('change', { bubbles: true }));
          break;
        }

        default:
          return this.fail(params, true, `Unknown action: ${action}. Use: click, type, select, scroll, focus, clear, check`, elementTag, previousValue);
      }

      // Wait for UI to settle
      const waitMs = params.waitAfterMs ?? 100;
      if (waitMs > 0) {
        await new Promise(resolve => setTimeout(resolve, waitMs));
      }

      return createInterfaceInteractResultFromParams(params, {
        success: true,
        elementFound: true,
        elementTag,
        elementText,
        previousValue,
      });

    } catch (err) {
      return this.fail(params, true, `Action failed: ${err instanceof Error ? err.message : String(err)}`, elementTag, previousValue);
    }
  }

  /**
   * Create a failure result with proper error field.
   */
  private fail(
    params: InterfaceInteractParams,
    elementFound: boolean,
    message: string,
    elementTag = '',
    previousValue = ''
  ): InterfaceInteractResult {
    return createInterfaceInteractResultFromParams(params, {
      success: false,
      elementFound,
      elementTag,
      elementText: '',
      previousValue,
      error: { message } as any,
    });
  }

  /**
   * Query selector with shadow DOM piercing.
   * Uses >> to traverse shadow roots: "host-element >> .inner-selector"
   */
  private querySelector(selector: string): Element | null {
    const parts = selector.split('>>').map(s => s.trim());

    let root: Document | ShadowRoot | Element = document;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const element: Element | null = root.querySelector(part);
      if (!element) return null;

      // If there are more segments, descend into shadow root
      if (i < parts.length - 1) {
        if (element.shadowRoot) {
          root = element.shadowRoot;
        } else {
          // No shadow root — try the element itself as context
          root = element;
        }
      } else {
        return element;
      }
    }

    return null;
  }

  /**
   * Get current value from an input/select/textarea element.
   */
  private getElementValue(element: Element): string {
    if (element instanceof HTMLInputElement) {
      return element.type === 'checkbox' ? String(element.checked) : element.value;
    }
    if (element instanceof HTMLTextAreaElement) return element.value;
    if (element instanceof HTMLSelectElement) return element.value;
    return '';
  }
}
