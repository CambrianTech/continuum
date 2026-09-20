/**
 * Chat Send Command - Browser Implementation
 *
 * Two modes:
 * - viaBrowser=true:  Fill the chat widget's textarea and click Send (simulates real user)
 * - viaBrowser=false: Delegate to server (direct DB insert)
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ChatSendCommand } from '../shared/ChatSendCommand';
import type { ChatSendParams, ChatSendResult } from '../shared/ChatSendTypes';

export class ChatSendBrowserCommand extends ChatSendCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatSend(params: ChatSendParams): Promise<ChatSendResult> {
    if (params.viaBrowser) {
      return this.sendViaWidget(params);
    }
    // Default: delegate to server
    return await this.remoteExecute(params);
  }

  /**
   * Send message by filling the chat widget's shadow DOM input and clicking Send.
   * Goes through the exact same path as a real user typing in the browser.
   *
   * Shadow DOM path (from browser inspector):
   * body > continuum-widget → shadowRoot
   *   → div > main-widget → shadowRoot
   *     → div > div.content-view > chat-widget → shadowRoot
   *       → div > div.input-container > #messageInput, #sendButton
   */
  private sendViaWidget(params: ChatSendParams): Promise<ChatSendResult> {
    // Walk the shadow DOM chain
    const chatShadow = this.getChatWidgetShadowRoot();

    const textarea = chatShadow.querySelector('#messageInput') as HTMLTextAreaElement | null;
    const sendButton = chatShadow.querySelector('#sendButton') as HTMLButtonElement | null;

    if (!textarea) {
      throw new Error('chat-widget #messageInput not found in shadow DOM');
    }
    if (!sendButton) {
      throw new Error('chat-widget #sendButton not found in shadow DOM');
    }

    // Set the value and fire input event so the widget's reactivity picks it up
    textarea.value = params.message;
    textarea.dispatchEvent(new Event('input', { bubbles: true, composed: true }));

    // Brief delay to let reactivity propagate, then click Send
    return new Promise((resolve) => {
      requestAnimationFrame(() => {
        sendButton.click();

        resolve({
          success: true,
          message: `Message submitted via browser widget`,
          messageEntity: {} as any, // Widget handles entity creation
          shortId: 'via-browser',
          roomId: '' as any,
        } as ChatSendResult);
      });
    });
  }

  /**
   * Navigate the shadow DOM chain to reach the chat widget's shadow root.
   * Throws descriptive errors at each step so we know exactly where it breaks.
   */
  private getChatWidgetShadowRoot(): ShadowRoot {
    const continuumWidget = document.querySelector('body > continuum-widget');
    if (!continuumWidget?.shadowRoot) {
      throw new Error('viaBrowser: continuum-widget not found or has no shadowRoot');
    }

    const mainWidget = continuumWidget.shadowRoot.querySelector('main-widget');
    if (!mainWidget?.shadowRoot) {
      throw new Error('viaBrowser: main-widget not found inside continuum-widget shadowRoot');
    }

    const chatWidget = mainWidget.shadowRoot.querySelector('chat-widget');
    if (!chatWidget?.shadowRoot) {
      throw new Error('viaBrowser: chat-widget not found inside main-widget shadowRoot');
    }

    return chatWidget.shadowRoot;
  }
}
