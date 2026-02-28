/**
 * LiveParticipantTile - Single participant in a live call
 *
 * Each tile owns its own shadow DOM, so .video-container CSS rules
 * actually apply to the video element. This is THE fix for the
 * video-not-filling-container problem.
 *
 * Video element is rendered directly in the Lit template — no imperative
 * DOM querying or appendChild. Lit handles node insertion natively.
 */

import { LitElement, html, unsafeCSS, nothing, type TemplateResult, type CSSResultGroup } from 'lit';
import { styles as TILE_STYLES } from './public/live-participant-tile.styles';
import { reactive } from '../shared/ReactiveWidget';

export class LiveParticipantTile extends LitElement {
  @reactive() userId: string = '';
  @reactive() displayName: string = '';
  @reactive() isSpeaking: boolean = false;
  @reactive() isPresenter: boolean = false;
  @reactive() isSpotlighted: boolean = false;
  @reactive() isScreenSharing: boolean = false;
  @reactive() isMuted: boolean = false;
  @reactive() isPinned: boolean = false;

  // Video element from LiveKit — rendered directly in template by Lit
  @reactive() videoElement: HTMLVideoElement | null = null;

  // ResizeObserver for adaptive avatar resolution
  private _resizeObserver: ResizeObserver | null = null;
  private _resizeDebounce: ReturnType<typeof setTimeout> | null = null;

  static override styles = [unsafeCSS(TILE_STYLES)] as CSSResultGroup;

  override connectedCallback(): void {
    super.connectedCallback();

    this._resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        if (width === 0 || height === 0) continue;

        // Debounce 500ms to avoid thrashing during window resize
        if (this._resizeDebounce) clearTimeout(this._resizeDebounce);
        this._resizeDebounce = setTimeout(() => {
          this.dispatchEvent(new CustomEvent('tile-resized', {
            bubbles: true,
            composed: true,
            detail: {
              userId: this.userId,
              width: Math.round(width),
              height: Math.round(height),
            }
          }));
        }, 500);
      }
    });
    this._resizeObserver.observe(this);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this._resizeObserver) {
      this._resizeObserver.disconnect();
      this._resizeObserver = null;
    }
    if (this._resizeDebounce) {
      clearTimeout(this._resizeDebounce);
      this._resizeDebounce = null;
    }
  }

  private get _hasVideo(): boolean {
    return !!this.videoElement;
  }

  protected override render(): TemplateResult {
    const classes = [
      'participant-tile',
      this.isSpeaking ? 'speaking' : '',
      this._hasVideo ? 'has-video' : '',
      this.isPresenter ? 'presenter' : '',
      this.isPinned ? 'pinned' : '',
    ].filter(Boolean).join(' ');

    return html`
      <div class="${classes}" @click=${this._onClick}>
        ${this.isScreenSharing
          ? html`<div class="screen-share-placeholder">${this._renderScreenShareIcon()} ${this.displayName} is sharing</div>`
          : this._hasVideo
            ? html`<div class="video-container">${this.videoElement}</div>`
            : html`
                <div class="participant-avatar ${this.isPresenter ? 'large' : ''}">
                  ${this.displayName.charAt(0).toUpperCase()}
                </div>
              `
        }
        <div class="participant-name">${this.displayName}</div>
        <div class="hover-overlay">
          <span class="overlay-name">${this.displayName}</span>
          <button class="pin-btn ${this.isPinned ? 'pinned' : ''}" @click=${this._onPin} title="${this.isPinned ? 'Unpin' : 'Pin'}">
            ${this._renderPinIcon()}
          </button>
        </div>
        ${this.isMuted ? html`
          <div class="participant-indicators">
            <div class="indicator muted">${this._renderMutedIcon()}</div>
          </div>
        ` : nothing}
        ${this.isSpotlighted ? html`
          <button class="exit-spotlight-btn" @click=${this._onExitSpotlight} title="Exit spotlight (Esc)">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        ` : this.isPresenter && !this.isSpotlighted ? html`
          <div class="live-badge">LIVE</div>
        ` : nothing}
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

  private _onPin(e: Event): void {
    e.stopPropagation();
    this.dispatchEvent(new CustomEvent('pin-participant', {
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

  private _renderPinIcon(): TemplateResult {
    return html`
      <svg width="16" height="16" viewBox="0 0 24 24" fill="${this.isPinned ? 'currentColor' : 'none'}" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 17v5"></path>
        <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"></path>
      </svg>
    `;
  }

  private _renderMutedIcon(): TemplateResult {
    return html`
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="2" x2="22" y1="2" y2="22"></line>
        <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"></path>
        <path d="M5 10v2a7 7 0 0 0 12 5"></path>
        <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"></path>
        <path d="M9 9v3a3 3 0 0 0 5.12 2.12"></path>
        <line x1="12" x2="12" y1="19" y2="22"></line>
      </svg>
    `;
  }
}

// Register custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-participant-tile')) {
  customElements.define('live-participant-tile', LiveParticipantTile);
}
