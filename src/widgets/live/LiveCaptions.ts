/**
 * LiveCaptions - Caption overlay for live transcription
 *
 * Pure display component. Shows multi-speaker captions with auto-fade.
 * Parent sets captions via the `captions` property; this component handles
 * its own fade timeouts internally.
 */

import { LitElement, html, unsafeCSS, type TemplateResult, type CSSResultGroup } from 'lit';
import { styles as CAPTIONS_STYLES } from './public/live-captions.styles';
import { reactive } from '../shared/ReactiveWidget';

export interface CaptionEntry {
  speakerName: string;
  text: string;
  timestamp: number;
}

export class LiveCaptions extends LitElement {
  @reactive() visible: boolean = true;
  @reactive() private _captions: Map<string, CaptionEntry> = new Map();

  // Fade timeouts per speaker key
  private _fadeTimeouts: Map<string, ReturnType<typeof setTimeout>> = new Map();

  static override styles = [unsafeCSS(CAPTIONS_STYLES)] as CSSResultGroup;

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._clearAllTimeouts();
  }

  /**
   * Set or update a caption for a speaker.
   * Auto-fades after the given duration (default 15s).
   */
  setCaption(speakerName: string, text: string, durationMs: number = 15000): void {
    // Clear existing timeout for this speaker
    const existing = this._fadeTimeouts.get(speakerName);
    if (existing) clearTimeout(existing);

    // Set/update caption
    const newCaptions = new Map(this._captions);
    newCaptions.set(speakerName, { speakerName, text, timestamp: Date.now() });
    this._captions = newCaptions;

    // Schedule fade
    const timeout = setTimeout(() => {
      this.removeCaption(speakerName);
    }, durationMs);
    this._fadeTimeouts.set(speakerName, timeout);
  }

  /**
   * Extend the fade timeout for an existing caption without changing the text.
   * Used for AI speech where audio duration arrives after the initial transcription.
   */
  extendCaption(speakerName: string, durationMs: number): void {
    if (!this._captions.has(speakerName)) return;

    const existing = this._fadeTimeouts.get(speakerName);
    if (existing) clearTimeout(existing);

    const timeout = setTimeout(() => {
      this.removeCaption(speakerName);
    }, durationMs);
    this._fadeTimeouts.set(speakerName, timeout);
  }

  /**
   * Check if a caption exists for a speaker
   */
  hasCaption(speakerName: string): boolean {
    return this._captions.has(speakerName);
  }

  /**
   * Remove a specific caption
   */
  removeCaption(speakerName: string): void {
    const timeout = this._fadeTimeouts.get(speakerName);
    if (timeout) clearTimeout(timeout);
    this._fadeTimeouts.delete(speakerName);

    const newCaptions = new Map(this._captions);
    newCaptions.delete(speakerName);
    this._captions = newCaptions;
  }

  /**
   * Clear all captions and timeouts
   */
  clearAll(): void {
    this._clearAllTimeouts();
    this._captions = new Map();
  }

  get captionCount(): number {
    return this._captions.size;
  }

  private _clearAllTimeouts(): void {
    this._fadeTimeouts.forEach(t => clearTimeout(t));
    this._fadeTimeouts.clear();
  }

  protected override render(): TemplateResult {
    if (!this.visible || this._captions.size === 0) {
      return html``;
    }

    return html`
      <div class="caption-display multi-speaker">
        ${Array.from(this._captions.values()).map(caption => html`
          <div class="caption-line">
            <span class="caption-speaker">${caption.speakerName}:</span>
            <span class="caption-text">${caption.text}</span>
          </div>
        `)}
      </div>
    `;
  }
}

// Register custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-captions')) {
  customElements.define('live-captions', LiveCaptions);
}
