/**
 * LiveParticipantTile - Single participant in a live call
 *
 * Each tile owns its own shadow DOM, so .video-container CSS rules
 * actually apply to the video element. This is THE fix for the
 * video-not-filling-container problem.
 *
 * Parent passes in properties; tile handles its own video attachment.
 * Fires events for user interactions (click, exit spotlight).
 */

import { LitElement, html, unsafeCSS, type TemplateResult, type CSSResultGroup } from 'lit';
import { styles as TILE_STYLES } from './public/live-participant-tile.styles';
import { reactive } from '../shared/ReactiveWidget';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export class LiveParticipantTile extends LitElement {
  @reactive() userId: string = '';
  @reactive() displayName: string = '';
  @reactive() isSpeaking: boolean = false;
  @reactive() hasVideo: boolean = false;
  @reactive() isPresenter: boolean = false;
  @reactive() isSpotlighted: boolean = false;
  @reactive() isScreenSharing: boolean = false;
  @reactive() colorIndex: number = 0;

  // Video element from LiveKit — set by parent, attached in updated()
  private _videoElement: HTMLVideoElement | null = null;

  static override styles = [unsafeCSS(TILE_STYLES)] as CSSResultGroup;

  /**
   * Set the video element to display. Triggers re-render and attachment.
   */
  set videoElement(el: HTMLVideoElement | null) {
    this._videoElement = el;
    this.hasVideo = !!el;
    this.requestUpdate();
  }

  get videoElement(): HTMLVideoElement | null {
    return this._videoElement;
  }

  protected override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);

    // Attach video element to the container after DOM update
    if (this._videoElement && this.hasVideo) {
      this._attachVideo();
    }
  }

  private _attachVideo(): void {
    const container = this.shadowRoot?.querySelector('.video-container') as HTMLElement | null;
    if (!container || !this._videoElement) return;

    if (!container.contains(this._videoElement)) {
      container.innerHTML = '';
      container.appendChild(this._videoElement);
    }
  }

  protected override render(): TemplateResult {
    const classes = [
      'participant-tile',
      this.isSpeaking ? 'speaking' : '',
      this.hasVideo ? 'has-video' : '',
      this.isPresenter ? 'presenter' : '',
    ].filter(Boolean).join(' ');

    return html`
      <div
        class="${classes}"
        @click=${this._onClick}
      >
        ${this.isScreenSharing
          ? html`<div class="screen-share-placeholder">${this._renderScreenShareIcon()} ${this.displayName} is sharing</div>`
          : this.hasVideo
            ? html`<div class="video-container"></div>`
            : html`
                <div class="participant-avatar ${this.isPresenter ? 'large' : ''}">
                  ${this.displayName.charAt(0).toUpperCase()}
                </div>
              `
        }
        <div class="participant-name">${this.displayName}</div>
        <div class="participant-indicators"></div>
        ${this.isSpotlighted ? html`
          <button class="exit-spotlight-btn" @click=${this._onExitSpotlight} title="Exit spotlight (Esc)">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        ` : this.isPresenter && !this.isSpotlighted ? html`
          <div class="live-badge">LIVE</div>
        ` : ''}
      </div>
    `;
  }

  private _onClick(): void {
    this.dispatchEvent(new CustomEvent('participant-click', {
      bubbles: true,
      composed: true,
      detail: { userId: this.userId }
    }));
  }

  private _onExitSpotlight(e: Event): void {
    e.stopPropagation();
    this.dispatchEvent(new CustomEvent('exit-spotlight', {
      bubbles: true,
      composed: true,
      detail: { userId: this.userId }
    }));
  }

  private _renderScreenShareIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <rect width="20" height="14" x="2" y="3" rx="2"></rect>
        <line x1="8" x2="16" y1="21" y2="21"></line>
        <line x1="12" x2="12" y1="17" y2="21"></line>
        <path d="m9 10 3-3 3 3"></path>
        <path d="M12 7v7"></path>
      </svg>
    `;
  }
}

// Register custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-participant-tile')) {
  customElements.define('live-participant-tile', LiveParticipantTile);
}
