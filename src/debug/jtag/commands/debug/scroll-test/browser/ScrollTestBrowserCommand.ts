/**
 * Scroll Test Browser Command - Animated Scroll Testing
 *
 * Performs animated scrolling with metrics capture for debugging scroll behaviors.
 * Essential for testing intersection observers, infinite scroll, and chat positioning.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ScrollTestParams, ScrollTestResult } from '../shared/ScrollTestTypes';
import { createScrollTestResult } from '../shared/ScrollTestTypes';
import { smartQuerySelector } from '../../../screenshot/shared/browser-utils/BrowserElementUtils';

export class ScrollTestBrowserCommand extends CommandBase<ScrollTestParams, ScrollTestResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('scroll-test', context, subpath, commander);
  }

  async execute(params: ScrollTestParams): Promise<ScrollTestResult> {
    const startTime = Date.now();
    console.log('🔧 SCROLL-TEST: Execute called with params:', params);

    // Handle preset shortcuts
    let effectiveParams = params;
    if (params.preset) {
      console.log('🔧 SCROLL-TEST: Processing preset:', params.preset);
      switch (params.preset) {
        case 'chat-top':
          effectiveParams = { ...params, target: 'top', behavior: 'smooth', captureMetrics: true, waitTime: 1000 };
          break;
        case 'chat-bottom':
          effectiveParams = { ...params, target: 'bottom', behavior: 'smooth', captureMetrics: true, waitTime: 1000 };
          break;
        case 'instant-top':
          effectiveParams = { ...params, target: 'top', behavior: 'instant', captureMetrics: true };
          break;
      }
      console.log('🔧 SCROLL-TEST: Effective params after preset:', effectiveParams);
    }

    // Find target element using Shadow DOM traversal
    const targetElement = this.findScrollContainer(effectiveParams.selector);

    if (!targetElement) {
      console.log('🔧 SCROLL-TEST: Target element not found, returning early');
      return createScrollTestResult(this.context, effectiveParams.sessionId || 'unknown', {
        scrollPerformed: false,
        targetElement: effectiveParams.selector ?? 'unknown',
        initialPosition: 0,
        finalPosition: 0
      });
    }

    console.log('🔧 SCROLL-TEST: Target element found, proceeding with scroll');

    // Capture initial metrics
    const initialPosition = targetElement.scrollTop;
    const metrics = effectiveParams.captureMetrics ? this.captureScrollMetrics(targetElement) : undefined;

    const debugMsg = `🔧 SCROLL-TEST-${startTime}: Starting ${effectiveParams.target} scroll on ${effectiveParams.selector ?? 'chat-widget'}`;
    console.log(debugMsg);
    console.error(debugMsg); // Also log to stderr for better capture

    // Calculate target position
    let targetPosition: number;
    switch (effectiveParams.target) {
      case 'top':
        targetPosition = 0;
        break;
      case 'bottom':
        targetPosition = targetElement.scrollHeight - targetElement.clientHeight;
        break;
      case 'position':
        targetPosition = effectiveParams.position ?? 0;
        break;
    }

    // Perform animated scroll (with optional repeat for intersection observer testing)
    const repeatCount = effectiveParams.repeat ?? 1;
    for (let i = 0; i < repeatCount; i++) {
      targetElement.scrollTo({
        top: targetPosition,
        behavior: effectiveParams.behavior ?? 'smooth'
      });

      // Wait between repeats if specified
      if (i < repeatCount - 1 && effectiveParams.waitTime) {
        await new Promise(resolve => setTimeout(resolve, (effectiveParams.waitTime ?? 1000) / repeatCount));
      }
    }

    // Wait for scroll completion if specified
    if (effectiveParams.waitTime) {
      await new Promise(resolve => setTimeout(resolve, effectiveParams.waitTime));
    }

    const finalPosition = targetElement.scrollTop;
    const scrollDuration = Date.now() - startTime;

    console.log(`✅ SCROLL-TEST-${startTime}: Completed - ${initialPosition} → ${finalPosition} (${scrollDuration}ms)`);

    return createScrollTestResult(this.context, effectiveParams.sessionId || 'unknown', {
      scrollPerformed: true,
      targetElement: effectiveParams.selector ?? 'chat-widget',
      initialPosition,
      finalPosition,
      scrollDuration,
      metrics
    });
  }

  private findScrollContainer(selector?: string): HTMLElement | null {
    console.log('🔧 SCROLL-TEST: Starting DOM traversal with smartQuerySelector');

    let targetElement: Element | null = null;

    if (selector) {
      // Use smartQuerySelector for custom selectors (handles Shadow DOM automatically)
      console.log(`🔧 SCROLL-TEST: Looking for custom selector: "${selector}"`);
      targetElement = smartQuerySelector(selector);
    } else {
      // Default: Chat widget messages container using smartQuerySelector
      console.log('🔧 SCROLL-TEST: Looking for default chat widget (#messages)');
      targetElement = smartQuerySelector('#messages');
    }

    if (targetElement) {
      const scrollContainer = targetElement as HTMLElement;
      console.log('🔧 SCROLL-TEST: Target element found!', {
        tagName: scrollContainer.tagName,
        id: scrollContainer.id,
        className: scrollContainer.className,
        scrollHeight: scrollContainer.scrollHeight,
        clientHeight: scrollContainer.clientHeight,
        scrollTop: scrollContainer.scrollTop
      });
      return scrollContainer;
    }

    console.log('🔧 SCROLL-TEST: Target element not found');
    return null;
  }

  private captureScrollMetrics(element: HTMLElement): { scrollHeight: number; clientHeight: number; messagesCount: number; sentinelVisible: boolean } {
    // Count messages for chat debugging
    const messages = element.querySelectorAll('[data-entity-id]');

    // Check sentinel visibility for intersection observer debugging
    const sentinel = element.querySelector('.entity-scroller-sentinel');
    const sentinelRect = sentinel?.getBoundingClientRect();
    const containerRect = element.getBoundingClientRect();

    const sentinelVisible = sentinelRect && containerRect ?
      (sentinelRect.top >= containerRect.top && sentinelRect.bottom <= containerRect.bottom) : false;

    return {
      scrollHeight: element.scrollHeight,
      clientHeight: element.clientHeight,
      messagesCount: messages.length,
      sentinelVisible
    };
  }
}