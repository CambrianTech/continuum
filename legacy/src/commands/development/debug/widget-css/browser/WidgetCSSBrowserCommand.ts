/**
 * Widget CSS Debug Browser Command
 * Hot-inject CSS into widgets for rapid iteration without full deployment
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WidgetCSSDebugParams, WidgetCSSDebugResult } from '../shared/WidgetCSSDebugTypes';
import { createWidgetCSSDebugResult } from '../shared/WidgetCSSDebugTypes';
import { WidgetDiscovery } from '@system/core/browser/utils/WidgetIntrospection';

export class WidgetCSSBrowserCommand extends CommandBase<WidgetCSSDebugParams, WidgetCSSDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-css-debug', context, subpath, commander);
  }

  async execute(params: WidgetCSSDebugParams): Promise<WidgetCSSDebugResult> {
    try {
      const widgetSelector = params.widgetSelector;

      // EXTRACT MODE: Pull CSS out of widget
      if (params.extract) {
        const widgetRef = WidgetDiscovery.findWidget(widgetSelector);
        if (!widgetRef) {
          return createWidgetCSSDebugResult(this.context, this.context.uuid, {
            success: false,
            widgetSelector,
            cssInjected: false,
            error: `Widget ${widgetSelector} not found`
          });
        }

        const shadowRoot = widgetRef.element.shadowRoot;
        if (!shadowRoot) {
          return createWidgetCSSDebugResult(this.context, this.context.uuid, {
            success: false,
            widgetSelector,
            cssInjected: false,
            error: 'Widget has no shadow root'
          });
        }

        // Extract all CSS from widget
        const styleElements = shadowRoot.querySelectorAll('style');
        const cssBlocks: string[] = [];

        styleElements.forEach((style, index) => {
          const isDebugInjected = style.hasAttribute('data-debug-injected');
          const timestamp = style.getAttribute('data-timestamp');

          let header = `/* Style Block ${index + 1}`;
          if (isDebugInjected) {
            header += ` - DEBUG INJECTED`;
            if (timestamp) {
              header += ` at ${new Date(parseInt(timestamp)).toISOString()}`;
            }
          } else {
            header += ` - ORIGINAL WIDGET STYLES`;
          }
          header += ' */\n';

          cssBlocks.push(header + (style.textContent || ''));
        });

        const extractedCSS = cssBlocks.join('\n\n');

        return createWidgetCSSDebugResult(this.context, this.context.uuid, {
          success: true,
          widgetSelector,
          cssInjected: false,
          extractedCSS
        });
      }

      // INJECTION MODE
      const cssContent: string = params.cssContent ?? '';

      // If cssFile provided, that takes precedence (handled by server command)
      if (!cssContent && !params.cssFile) {
        return createWidgetCSSDebugResult(this.context, this.context.uuid, {
          success: false,
          widgetSelector,
          cssInjected: false,
          error: 'Either cssContent or cssFile must be provided'
        });
      }

      // Find widget(s) in shadow DOM
      const widgets = params.multiWidget
        ? this.findAllWidgets(widgetSelector)
        : [WidgetDiscovery.findWidget(widgetSelector)].filter(Boolean);

      if (widgets.length === 0) {
        return createWidgetCSSDebugResult(this.context, this.context.uuid, {
          success: false,
          widgetSelector,
          cssInjected: false,
          error: `No widgets found matching ${widgetSelector}`
        });
      }

      let widgetsAffected = 0;

      // Apply CSS to each widget
      for (const widgetRef of widgets) {
        if (!widgetRef) continue;

        const shadowRoot = widgetRef.element.shadowRoot;
        if (!shadowRoot) continue;

        // Determine injection mode (backwards compatible with clearExisting)
        const mode = params.mode || (params.clearExisting ? 'replaceAll' : 'replace');

        // Handle different injection modes
        switch (mode) {
          case 'replaceAll':
            // Remove ALL styles (original + debug)
            shadowRoot.querySelectorAll('style').forEach(el => el.remove());
            break;
          case 'replace':
            // Remove only debug-injected styles, keep original widget styles
            shadowRoot.querySelectorAll('style[data-debug-injected]').forEach(el => el.remove());
            break;
          case 'append':
            // Keep everything, just add more
            break;
          case 'debugOnly':
            // Remove previous debug overlays only
            shadowRoot.querySelectorAll('style[data-debug-overlay]').forEach(el => el.remove());
            break;
        }

        // Build CSS with advanced features
        let finalCSS = cssContent;

        // Layout debugging overlays (marked separately for debugOnly mode)
        const hasDebugOverlays = params.showBoundingBoxes || params.highlightFlexboxes || params.animateChanges;

        if (params.showBoundingBoxes) {
          finalCSS += `\n* { outline: 1px solid rgba(255, 0, 0, 0.3) !important; }`;
        }

        if (params.highlightFlexboxes) {
          finalCSS += `\n[style*="display: flex"], [style*="display:flex"] { outline: 2px solid rgba(0, 255, 0, 0.5) !important; }`;
          finalCSS += `\n.entity-list-container, .user-list, .user-item { outline: 2px solid rgba(0, 255, 0, 0.5) !important; }`;
        }

        // Animate changes
        if (params.animateChanges) {
          finalCSS += `\n* { transition: all 0.3s ease !important; }`;
        }

        // Inject CSS
        const styleElement = globalThis.document.createElement('style');
        styleElement.setAttribute('data-debug-injected', 'true');
        if (hasDebugOverlays) {
          styleElement.setAttribute('data-debug-overlay', 'true');
        }
        styleElement.setAttribute('data-timestamp', Date.now().toString());
        styleElement.textContent = finalCSS;
        shadowRoot.appendChild(styleElement);

        widgetsAffected++;
      }

      console.log(`✅ CSS injected into ${widgetsAffected} widget(s) matching ${widgetSelector}`);

      // Extract CSS variables from first widget
      const firstWidget = widgets[0];
      const cssVariables = firstWidget ? this.extractCSSVariables(firstWidget.element) : {};

      return createWidgetCSSDebugResult(this.context, this.context.uuid, {
        success: true,
        widgetSelector,
        cssInjected: true,
        widgetsAffected,
        cssVariables
      });

    } catch (error) {
      return createWidgetCSSDebugResult(this.context, this.context.uuid, {
        success: false,
        widgetSelector: params.widgetSelector,
        cssInjected: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  /**
   * Find all widgets matching selector (for multiWidget mode)
   */
  private findAllWidgets(selector: string): Array<{ element: HTMLElement; path: string } | null> {
    const widgets: Array<{ element: HTMLElement; path: string }> = [];

    // Search entire DOM tree for matching widgets
    const allElements = globalThis.document.querySelectorAll(selector);
    allElements.forEach(el => {
      if (el instanceof HTMLElement && el.shadowRoot) {
        widgets.push({ element: el, path: this.getElementPath(el) });
      }
    });

    return widgets;
  }

  /**
   * Get path to element for debugging
   */
  private getElementPath(element: HTMLElement): string {
    const parts: string[] = [];
    let current: HTMLElement | null = element;

    while (current && current !== globalThis.document.body) {
      parts.unshift(current.tagName.toLowerCase());
      current = current.parentElement;
    }

    return parts.join(' > ');
  }

  /**
   * Extract CSS variables from widget for reference
   */
  private extractCSSVariables(element: HTMLElement): Record<string, string> {
    const computedStyle = globalThis.getComputedStyle(element);
    const variables: Record<string, string> = {};

    // Get all CSS custom properties
    for (let i = 0; i < computedStyle.length; i++) {
      const prop = computedStyle[i];
      if (prop.startsWith('--')) {
        variables[prop] = computedStyle.getPropertyValue(prop).trim();
      }
    }

    return variables;
  }
}
