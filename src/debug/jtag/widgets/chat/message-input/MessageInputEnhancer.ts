/**
 * Message Input Enhancer - Markdown keyboard shortcuts and auto-formatting
 *
 * Adds power-user markdown features to chat input:
 * - Keyboard shortcuts (Cmd+B for bold, Cmd+I for italic, etc.)
 * - Auto-formatting hints (optional live preview)
 * - Code block auto-completion
 *
 * Separated from ChatWidget to keep widget logic clean
 */

export interface MessageInputConfig {
  enableShortcuts?: boolean;
  enableAutoFormat?: boolean;
  enablePreview?: boolean;
}

export class MessageInputEnhancer {
  private inputElement: HTMLInputElement | HTMLTextAreaElement;
  private config: Required<MessageInputConfig>;
  private keydownHandler?: EventListener;

  constructor(
    inputElement: HTMLInputElement | HTMLTextAreaElement,
    config: MessageInputConfig = {}
  ) {
    this.inputElement = inputElement;
    this.config = {
      enableShortcuts: config.enableShortcuts ?? true,
      enableAutoFormat: config.enableAutoFormat ?? false, // Disabled by default (can be distracting)
      enablePreview: config.enablePreview ?? false // Disabled by default
    };

    this.initialize();
  }

  private initialize(): void {
    if (this.config.enableShortcuts) {
      this.setupKeyboardShortcuts();
    }

    if (this.config.enableAutoFormat) {
      this.setupAutoFormatting();
    }
  }

  /**
   * Setup keyboard shortcuts for common markdown formatting
   */
  private setupKeyboardShortcuts(): void {
    this.keydownHandler = ((e: KeyboardEvent) => {
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
      const modKey = isMac ? e.metaKey : e.ctrlKey;

      if (!modKey) return;

      switch (e.key.toLowerCase()) {
        case 'b':
          e.preventDefault();
          this.wrapSelection('**', '**');
          break;

        case 'i':
          e.preventDefault();
          this.wrapSelection('*', '*');
          break;

        case 'k':
          e.preventDefault();
          this.insertLink();
          break;

        case 'e':
          e.preventDefault();
          this.wrapSelection('`', '`');
          break;

        case 'd': // Cmd+D for strikethrough
          e.preventDefault();
          this.wrapSelection('~~', '~~');
          break;
      }
    }) as EventListener;

    this.inputElement.addEventListener('keydown', this.keydownHandler);

    // Enable browser spell-checking but DISABLE autocorrect (coders hate auto-correct)
    this.inputElement.setAttribute('spellcheck', 'true');
    this.inputElement.setAttribute('autocorrect', 'off');
    this.inputElement.setAttribute('autocapitalize', 'off');
  }

  /**
   * Setup auto-formatting for code blocks (``` triggers language selector)
   */
  private setupAutoFormatting(): void {
    let lastValue = this.inputElement.value;

    this.inputElement.addEventListener('input', () => {
      const currentValue = this.inputElement.value;

      // Detect ``` typed (code block start)
      if (currentValue.endsWith('```') && !lastValue.endsWith('```')) {
        this.startCodeBlock();
      }

      lastValue = currentValue;
    });
  }

  /**
   * Wrap selected text with markdown syntax
   */
  private wrapSelection(before: string, after: string): void {
    const start = this.inputElement.selectionStart || 0;
    const end = this.inputElement.selectionEnd || 0;
    const value = this.inputElement.value;
    const selectedText = value.substring(start, end);

    // If nothing selected, insert syntax with cursor in middle
    if (start === end) {
      const newText = value.substring(0, start) + before + after + value.substring(end);
      this.inputElement.value = newText;
      this.inputElement.setSelectionRange(start + before.length, start + before.length);
    } else {
      // Wrap selected text
      const newText = value.substring(0, start) + before + selectedText + after + value.substring(end);
      this.inputElement.value = newText;
      this.inputElement.setSelectionRange(start + before.length, end + before.length);
    }

    this.inputElement.focus();
    this.inputElement.dispatchEvent(new Event('input', { bubbles: true }));
  }

  /**
   * Insert link markdown with prompt
   */
  private insertLink(): void {
    const start = this.inputElement.selectionStart || 0;
    const end = this.inputElement.selectionEnd || 0;
    const value = this.inputElement.value;
    const selectedText = value.substring(start, end);

    const linkText = selectedText || 'link text';
    const linkUrl = 'url';

    const markdown = `[${linkText}](${linkUrl})`;
    const newText = value.substring(0, start) + markdown + value.substring(end);
    this.inputElement.value = newText;

    // Select the URL part so user can paste/type
    const urlStart = start + linkText.length + 3; // After "[text]("
    const urlEnd = urlStart + linkUrl.length;
    this.inputElement.setSelectionRange(urlStart, urlEnd);

    this.inputElement.focus();
    this.inputElement.dispatchEvent(new Event('input', { bubbles: true }));
  }

  /**
   * Auto-complete code block when user types ```
   */
  private startCodeBlock(): void {
    const value = this.inputElement.value;
    const cursorPos = this.inputElement.selectionStart || 0;

    // Remove the ``` they just typed
    const beforeCursor = value.substring(0, cursorPos - 3);
    const afterCursor = value.substring(cursorPos);

    // Insert full code block template
    const template = '```javascript\n\n```';
    const newText = beforeCursor + template + afterCursor;
    this.inputElement.value = newText;

    // Position cursor inside the code block (after language and newline)
    const newCursorPos = beforeCursor.length + '```javascript\n'.length;
    this.inputElement.setSelectionRange(newCursorPos, newCursorPos);

    this.inputElement.focus();
  }

  /**
   * Cleanup event listeners
   */
  destroy(): void {
    if (this.keydownHandler) {
      this.inputElement.removeEventListener('keydown', this.keydownHandler);
    }
  }

  /**
   * Get markdown formatting hint (for tooltip/help text)
   */
  static getFormattingHints(): string[] {
    const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
    const mod = isMac ? '⌘' : 'Ctrl';

    return [
      `${mod}+B - Bold`,
      `${mod}+I - Italic`,
      `${mod}+K - Link`,
      `${mod}+E - Inline code`,
      `${mod}+D - Strikethrough`,
      '``` - Code block'
    ];
  }
}
