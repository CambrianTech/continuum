/**
 * MentionAutocomplete — Slack-style @mention popup for textarea inputs.
 *
 * Entirely client-side: filters against a pre-loaded user list (Map<UUID, UserEntity>).
 * Zero server hops during typing — ms-level latency.
 *
 * Usage:
 *   const ac = new MentionAutocomplete(shadowRoot, textarea, () => this.roomMembers);
 *   // That's it. Handles @detection, filtering, keyboard nav, insertion.
 *   ac.destroy();  // cleanup
 */

import type { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

const MAX_SUGGESTIONS = 8;

export class MentionAutocomplete {
  private _dropdown: HTMLElement | null = null;
  private _suggestions: UserEntity[] = [];
  private _selectedIndex = 0;
  private _mentionStart = -1; // cursor position of the '@' character
  private _active = false;

  // Bound handlers for cleanup
  private _onInput: () => void;
  private _onKeydown: (e: KeyboardEvent) => void;
  private _onBlur: () => void;
  private _onScroll: () => void;

  constructor(
    private readonly shadowRoot: ShadowRoot,
    private readonly textarea: HTMLTextAreaElement,
    private readonly getUserList: () => Map<UUID, UserEntity>,
  ) {
    this._onInput = this._handleInput.bind(this);
    this._onKeydown = this._handleKeydown.bind(this);
    this._onBlur = this._handleBlur.bind(this);
    this._onScroll = () => this._hide();

    textarea.addEventListener('input', this._onInput);
    textarea.addEventListener('keydown', this._onKeydown);
    textarea.addEventListener('blur', this._onBlur);
    textarea.addEventListener('scroll', this._onScroll);

    this._injectStyles();
  }

  /** Whether the autocomplete popup is currently visible. */
  get active(): boolean { return this._active; }

  destroy(): void {
    this.textarea.removeEventListener('input', this._onInput);
    this.textarea.removeEventListener('keydown', this._onKeydown);
    this.textarea.removeEventListener('blur', this._onBlur);
    this.textarea.removeEventListener('scroll', this._onScroll);
    this._hide();
  }

  // === INPUT DETECTION ===

  private _handleInput(): void {
    const { value, selectionStart } = this.textarea;
    if (selectionStart === null) { this._hide(); return; }

    // Walk backwards from cursor to find an unmatched '@'
    const mentionStart = this._findMentionStart(value, selectionStart);
    if (mentionStart === -1) {
      this._hide();
      return;
    }

    this._mentionStart = mentionStart;
    const query = value.slice(mentionStart + 1, selectionStart).toLowerCase();

    // Filter users client-side — already in memory, instant
    const users = this.getUserList();
    this._suggestions = [];
    for (const user of users.values()) {
      if (this._suggestions.length >= MAX_SUGGESTIONS) break;
      const name = (user.displayName || user.uniqueId || '').toLowerCase();
      const uid = (user.uniqueId || '').toLowerCase();
      if (name.includes(query) || uid.includes(query)) {
        this._suggestions.push(user);
      }
    }

    if (this._suggestions.length === 0) {
      this._hide();
      return;
    }

    // Sort: exact prefix matches first, then alphabetical
    this._suggestions.sort((a, b) => {
      const aName = (a.displayName || a.uniqueId).toLowerCase();
      const bName = (b.displayName || b.uniqueId).toLowerCase();
      const aPrefix = aName.startsWith(query) ? 0 : 1;
      const bPrefix = bName.startsWith(query) ? 0 : 1;
      if (aPrefix !== bPrefix) return aPrefix - bPrefix;
      return aName.localeCompare(bName);
    });

    this._selectedIndex = 0;
    this._show();
  }

  /**
   * Walk backwards from cursor to find '@' that starts a mention.
   * Returns -1 if no valid mention context found.
   * A mention is valid if '@' is at position 0 or preceded by whitespace.
   */
  private _findMentionStart(value: string, cursor: number): number {
    for (let i = cursor - 1; i >= 0; i--) {
      const ch = value[i];
      if (ch === '@') {
        // Valid: start of string or preceded by space/newline
        if (i === 0 || /\s/.test(value[i - 1])) return i;
        return -1; // '@' inside a word — not a mention
      }
      if (/\s/.test(ch)) return -1; // Hit whitespace before finding '@'
    }
    return -1;
  }

  // === KEYBOARD NAVIGATION ===

  private _handleKeydown(e: KeyboardEvent): void {
    if (!this._active) return;

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        this._selectedIndex = (this._selectedIndex + 1) % this._suggestions.length;
        this._renderItems();
        break;
      case 'ArrowUp':
        e.preventDefault();
        this._selectedIndex = (this._selectedIndex - 1 + this._suggestions.length) % this._suggestions.length;
        this._renderItems();
        break;
      case 'Enter':
      case 'Tab':
        e.preventDefault();
        e.stopPropagation();
        this._selectCurrent();
        break;
      case 'Escape':
        e.preventDefault();
        this._hide();
        break;
    }
  }

  private _handleBlur(): void {
    // Delay hide so click on dropdown item can fire first
    setTimeout(() => this._hide(), 150);
  }

  // === SELECTION ===

  private _selectCurrent(): void {
    const user = this._suggestions[this._selectedIndex];
    if (!user) return;
    this._insertMention(user);
  }

  private _insertMention(user: UserEntity): void {
    const handle = user.uniqueId || user.displayName;
    const { value, selectionStart } = this.textarea;
    if (selectionStart === null || this._mentionStart === -1) return;

    // Insert @uniqueId (Slack-style short handle, no spaces, unambiguous boundary)
    const before = value.slice(0, this._mentionStart);
    const after = value.slice(selectionStart);
    const mention = `@${handle} `;

    this.textarea.value = before + mention + after;

    // Position cursor after the inserted mention
    const newCursor = this._mentionStart + mention.length;
    this.textarea.setSelectionRange(newCursor, newCursor);

    // Trigger input event so auto-grow and other handlers fire
    this.textarea.dispatchEvent(new Event('input', { bubbles: true }));

    this._hide();
    this.textarea.focus();
  }

  // === RENDERING ===

  private _show(): void {
    if (!this._dropdown) {
      this._dropdown = document.createElement('div');
      this._dropdown.className = 'mention-autocomplete';
      // Insert before input-container so it appears above it
      const inputContainer = this.textarea.closest('.input-container');
      if (inputContainer) {
        inputContainer.insertBefore(this._dropdown, this.textarea);
      } else {
        this.shadowRoot.appendChild(this._dropdown);
      }
    }
    this._active = true;
    this._renderItems();
    this._dropdown.style.display = 'block';
  }

  private _hide(): void {
    if (this._dropdown) {
      this._dropdown.style.display = 'none';
    }
    this._active = false;
    this._mentionStart = -1;
  }

  private _renderItems(): void {
    if (!this._dropdown) return;

    this._dropdown.innerHTML = this._suggestions.map((user, i) => {
      const name = user.displayName || user.uniqueId;
      const uniqueId = user.uniqueId || '';
      const initial = name.charAt(0).toUpperCase();
      const isOnline = user.status === 'online';
      const typeBadge = user.type === 'persona' ? 'AI' : user.type === 'human' ? '' : user.type ?? '';
      const selected = i === this._selectedIndex ? 'selected' : '';

      return `
        <div class="mention-item ${selected}" data-index="${i}">
          <span class="mention-avatar">${initial}</span>
          <span class="mention-status ${isOnline ? 'online' : 'offline'}"></span>
          <span class="mention-name">${this._escapeHtml(name)}</span>
          ${uniqueId !== name ? `<span class="mention-uid">${this._escapeHtml(uniqueId)}</span>` : ''}
          ${typeBadge ? `<span class="mention-badge">${typeBadge}</span>` : ''}
        </div>
      `;
    }).join('');

    // Attach click handlers
    this._dropdown.querySelectorAll('.mention-item').forEach(item => {
      item.addEventListener('mousedown', (e) => {
        e.preventDefault(); // prevent blur
        const idx = parseInt((item as HTMLElement).dataset.index ?? '0');
        this._selectedIndex = idx;
        this._selectCurrent();
      });
    });
  }

  private _escapeHtml(s: string): string {
    const div = document.createElement('div');
    div.textContent = s;
    return div.innerHTML;
  }

  // === STYLES ===

  private _injectStyles(): void {
    const id = 'mention-autocomplete-styles';
    if (this.shadowRoot.getElementById(id)) return;

    const style = document.createElement('style');
    style.id = id;
    style.textContent = `
      .mention-autocomplete {
        position: absolute;
        bottom: 100%;
        left: 0;
        right: 0;
        max-height: 280px;
        overflow-y: auto;
        background: var(--bg-secondary, #1e1e2e);
        border: 1px solid var(--border-color, #333);
        border-radius: 8px;
        box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.4);
        z-index: 100;
        margin-bottom: 4px;
        padding: 4px 0;
        display: none;
      }

      .mention-item {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 6px 12px;
        cursor: pointer;
        font-size: 13px;
        color: var(--text-primary, #e0e0e0);
        position: relative;
      }

      .mention-item:hover,
      .mention-item.selected {
        background: var(--bg-hover, rgba(255, 255, 255, 0.08));
      }

      .mention-item.selected {
        background: var(--bg-active, rgba(0, 212, 255, 0.12));
      }

      .mention-avatar {
        width: 24px;
        height: 24px;
        border-radius: 50%;
        background: var(--accent-dim, #2a4a5a);
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 12px;
        font-weight: 600;
        color: var(--accent, #00d4ff);
        flex-shrink: 0;
      }

      .mention-status {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        flex-shrink: 0;
      }
      .mention-status.online { background: #4caf50; }
      .mention-status.offline { background: #666; }

      .mention-name {
        font-weight: 500;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .mention-uid {
        color: var(--text-secondary, #888);
        font-size: 11px;
        white-space: nowrap;
      }

      .mention-badge {
        margin-left: auto;
        font-size: 10px;
        padding: 1px 5px;
        border-radius: 4px;
        background: var(--accent-dim, rgba(0, 212, 255, 0.15));
        color: var(--accent, #00d4ff);
        font-weight: 600;
        flex-shrink: 0;
      }
    `;
    this.shadowRoot.appendChild(style);
  }
}
