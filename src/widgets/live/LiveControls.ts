/**
 * LiveControls - Media control bar for live calls
 *
 * Pure presentational component. Renders mic/speaker/camera/screenshare/captions/leave
 * buttons with SVG icons. Fires custom events for each action — parent (LiveWidget)
 * handles the actual logic.
 */

import { LitElement, html, unsafeCSS, type TemplateResult, type CSSResultGroup } from 'lit';
import { styles as CONTROLS_STYLES } from './public/live-controls.styles';

// Re-use the reactive decorator from the shared module
import { reactive } from '../shared/ReactiveWidget';

export class LiveControls extends LitElement {
  @reactive() micEnabled: boolean = true;
  @reactive() speakerEnabled: boolean = true;
  @reactive() cameraEnabled: boolean = false;
  @reactive() screenShareEnabled: boolean = false;
  @reactive() captionsEnabled: boolean = true;

  static override styles = [unsafeCSS(CONTROLS_STYLES)] as CSSResultGroup;

  protected override render(): TemplateResult {
    return html`
      <div class="controls">
        <button
          class="control-btn ${this.micEnabled ? 'active' : 'inactive'}"
          @click=${this._onToggleMic}
          title="${this.micEnabled ? 'Mute your mic' : 'Unmute your mic'}"
        >
          ${this.micEnabled ? this._renderMicOnIcon() : this._renderMicOffIcon()}
          ${this.micEnabled ? html`<span class="mic-level-indicator" id="mic-level"></span>` : ''}
        </button>
        <button
          class="control-btn ${this.speakerEnabled ? 'active' : 'inactive'}"
          @click=${this._onToggleSpeaker}
          title="${this.speakerEnabled ? 'Mute audio' : 'Unmute audio'}"
        >
          ${this.speakerEnabled ? this._renderSpeakerOnIcon() : this._renderSpeakerOffIcon()}
        </button>
        <button
          class="control-btn ${this.cameraEnabled ? 'active' : 'inactive'}"
          @click=${this._onToggleCamera}
          title="${this.cameraEnabled ? 'Turn off camera' : 'Turn on camera'}"
        >
          ${this.cameraEnabled ? this._renderCameraOnIcon() : this._renderCameraOffIcon()}
        </button>
        <button
          class="control-btn ${this.screenShareEnabled ? 'active' : 'inactive'}"
          @click=${this._onToggleScreenShare}
          title="${this.screenShareEnabled ? 'Stop sharing' : 'Share screen'}"
        >
          ${this._renderScreenShareIcon()}
        </button>
        <button
          class="control-btn ${this.captionsEnabled ? 'active' : 'inactive'}"
          @click=${this._onToggleCaptions}
          title="${this.captionsEnabled ? 'Hide captions' : 'Show captions'}"
        >
          ${this._renderCaptionsIcon()}
        </button>
        <button
          class="control-btn leave"
          @click=${this._onLeave}
          title="Leave"
        >
          ${this._renderLeaveIcon()}
        </button>
      </div>
    `;
  }

  /**
   * Update mic level indicator directly (bypasses reactive render for performance).
   * Called at ~30fps from AudioStreamClient.
   */
  setMicLevel(level: number): void {
    const indicator = this.shadowRoot?.getElementById('mic-level') as HTMLElement;
    if (indicator) {
      indicator.style.height = `${Math.min(100, level * 300)}%`;
    }
  }

  // ========================================
  // Event dispatchers
  // ========================================

  private _onToggleMic(): void {
    this.dispatchEvent(new CustomEvent('toggle-mic', { bubbles: true, composed: true }));
  }

  private _onToggleSpeaker(): void {
    this.dispatchEvent(new CustomEvent('toggle-speaker', { bubbles: true, composed: true }));
  }

  private _onToggleCamera(): void {
    this.dispatchEvent(new CustomEvent('toggle-camera', { bubbles: true, composed: true }));
  }

  private _onToggleScreenShare(): void {
    this.dispatchEvent(new CustomEvent('toggle-screenshare', { bubbles: true, composed: true }));
  }

  private _onToggleCaptions(): void {
    this.dispatchEvent(new CustomEvent('toggle-captions', { bubbles: true, composed: true }));
  }

  private _onLeave(): void {
    this.dispatchEvent(new CustomEvent('leave', { bubbles: true, composed: true }));
  }

  // ========================================
  // SVG Icon Renderers
  // ========================================

  private _renderMicOnIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"></path>
        <path d="M19 10v2a7 7 0 0 1-14 0v-2"></path>
        <line x1="12" x2="12" y1="19" y2="22"></line>
      </svg>
    `;
  }

  private _renderMicOffIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="2" x2="22" y1="2" y2="22"></line>
        <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"></path>
        <path d="M5 10v2a7 7 0 0 0 12 5"></path>
        <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"></path>
        <path d="M9 9v3a3 3 0 0 0 5.12 2.12"></path>
        <line x1="12" x2="12" y1="19" y2="22"></line>
      </svg>
    `;
  }

  private _renderCameraOnIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="m22 8-6 4 6 4V8Z"></path>
        <rect width="14" height="12" x="2" y="6" rx="2" ry="2"></rect>
      </svg>
    `;
  }

  private _renderCameraOffIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="2" x2="22" y1="2" y2="22"></line>
        <path d="M7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h12"></path>
        <path d="m22 8-6 4 6 4V8Z"></path>
        <path d="M10.3 7.7A4 4 0 0 1 14 6h2a2 2 0 0 1 2 2v3.34l1 .66"></path>
      </svg>
    `;
  }

  private _renderSpeakerOnIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon>
        <path d="M15.54 8.46a5 5 0 0 1 0 7.07"></path>
        <path d="M19.07 4.93a10 10 0 0 1 0 14.14"></path>
      </svg>
    `;
  }

  private _renderSpeakerOffIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon>
        <line x1="22" x2="16" y1="9" y2="15"></line>
        <line x1="16" x2="22" y1="9" y2="15"></line>
      </svg>
    `;
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

  private _renderLeaveIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M10.68 13.31a16 16 0 0 0 3.41 2.6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7 2 2 0 0 1 1.72 2v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.42 19.42 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.63A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91"></path>
        <line x1="22" x2="16" y1="2" y2="8"></line>
        <line x1="16" x2="22" y1="2" y2="8"></line>
      </svg>
    `;
  }

  private _renderCaptionsIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="2" y="4" width="20" height="16" rx="2"></rect>
        <text x="12" y="15" text-anchor="middle" fill="currentColor" stroke="none" font-size="9" font-weight="700" font-family="sans-serif">CC</text>
      </svg>
    `;
  }
}

// Register custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-controls')) {
  customElements.define('live-controls', LiveControls);
}
