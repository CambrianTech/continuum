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

  // Video element from LiveKit — rendered directly in template by Lit
  @reactive() videoElement: HTMLVideoElement | null = null;

  static override styles = [unsafeCSS(TILE_STYLES)] as CSSResultGroup;

  private get _hasVideo(): boolean {
    return !!this.videoElement;
  }

  protected override render(): TemplateResult {
    const classes = [
      'participant-tile',
      this.isSpeaking ? 'speaking' : '',
      this._hasVideo ? 'has-video' : '',
      this.isPresenter ? 'presenter' : '',
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
