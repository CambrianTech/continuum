/**
 * PanelStyles - Shared CSS for panel-type widgets
 *
 * Used by: Settings, Help, Theme, Diagnostics, and other "assistant panel" widgets.
 * These widgets share a common layout: main content area + optional AI assistant sidebar.
 *
 * Design System Colors:
 * - Primary: #00d4ff (cyan) - links, headers, accents
 * - Success: #00ff64 (green) - configured, operational, local
 * - Warning: #ffc832 (yellow) - out-of-funds, caution
 * - Error: #ff5050 (red) - errors, invalid
 * - Muted: rgba(255, 255, 255, 0.6) - secondary text
 */

/**
 * Core layout styles for panel widgets
 * Two-column layout: main content + assistant sidebar
 */
export const PANEL_LAYOUT_STYLES = `
  :host {
    display: flex;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .panel-layout {
    display: flex;
    flex: 1;
    width: 100%;
    height: 100%;
    gap: 0;
  }

  .panel-main {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
    min-width: 0;
  }

  .panel-assistant {
    flex-shrink: 0;
    height: 100%;
    display: flex;
  }

  .panel-container {
    width: 100%;
  }

  @media (max-width: 768px) {
    .panel-layout {
      flex-direction: column;
    }

    .panel-assistant {
      height: 300px;
      flex-shrink: 0;
    }
  }
`;

/**
 * Header styles for panel titles and subtitles
 */
export const PANEL_HEADER_STYLES = `
  .panel-header {
    margin-bottom: 24px;
  }

  .panel-title {
    font-size: 24px;
    font-weight: 600;
    color: #00d4ff;
    margin: 0 0 8px 0;
  }

  .panel-subtitle {
    color: rgba(255, 255, 255, 0.6);
    font-size: 14px;
  }
`;

/**
 * Section styles - content blocks with headers and borders
 */
export const PANEL_SECTION_STYLES = `
  .panel-section {
    background: rgba(15, 20, 25, 0.8);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 8px;
    padding: 20px;
    margin-bottom: 16px;
  }

  .panel-section.highlighted {
    background: rgba(0, 255, 100, 0.1);
    border-color: rgba(0, 255, 100, 0.3);
  }

  .panel-section.highlighted .section-title {
    color: #00ff64;
  }

  .section-title {
    font-size: 16px;
    font-weight: 600;
    color: #00d4ff;
    margin: 0 0 16px 0;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(0, 212, 255, 0.2);
  }

  .section-intro {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
    margin-bottom: 16px;
    line-height: 1.5;
  }

  .section-intro a {
    color: #00ff64;
    text-decoration: none;
  }

  .section-intro a:hover {
    text-decoration: underline;
  }
`;

/**
 * Info box styles - highlighted information panels
 */
export const PANEL_INFO_STYLES = `
  .info-box {
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 6px;
    padding: 12px 16px;
    margin-bottom: 20px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
  }

  .info-box a {
    color: #00d4ff;
    text-decoration: none;
  }

  .info-box a:hover {
    text-decoration: underline;
  }

  .info-box.success {
    background: rgba(0, 255, 100, 0.1);
    border-color: rgba(0, 255, 100, 0.3);
  }

  .info-box.warning {
    background: rgba(255, 200, 50, 0.1);
    border-color: rgba(255, 200, 50, 0.3);
  }

  .info-box.error {
    background: rgba(255, 50, 50, 0.1);
    border-color: rgba(255, 50, 50, 0.3);
  }
`;

/**
 * Loading and status message styles
 */
export const PANEL_STATUS_STYLES = `
  .loading {
    text-align: center;
    padding: 40px;
    color: rgba(255, 255, 255, 0.6);
  }

  .loading-spinner {
    display: inline-block;
    width: 24px;
    height: 24px;
    border: 2px solid rgba(0, 212, 255, 0.3);
    border-top-color: #00d4ff;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .status-message {
    padding: 8px 16px;
    border-radius: 4px;
    font-size: 13px;
  }

  .status-saved {
    background: rgba(0, 255, 100, 0.1);
    color: #00ff64;
  }

  .status-error {
    background: rgba(255, 50, 50, 0.1);
    color: #ff5050;
  }

  .status-indicator {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 10px;
  }

  .status-configured,
  .status-operational {
    background: rgba(0, 255, 100, 0.15);
    color: #00ff64;
  }

  .status-not-set {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.5);
  }

  .status-invalid,
  .status-error {
    background: rgba(255, 50, 50, 0.1);
    color: #ff5050;
  }

  .status-warning,
  .status-out-of-funds {
    background: rgba(255, 200, 50, 0.1);
    color: #ffc832;
  }

  .status-rate-limited {
    background: rgba(255, 150, 50, 0.1);
    color: #ff9632;
  }

  .status-testing {
    background: rgba(0, 212, 255, 0.1);
    color: #00d4ff;
  }
`;

/**
 * Combined panel styles - import this for full panel styling
 */
export const PANEL_STYLES = `
  ${PANEL_LAYOUT_STYLES}
  ${PANEL_HEADER_STYLES}
  ${PANEL_SECTION_STYLES}
  ${PANEL_INFO_STYLES}
  ${PANEL_STATUS_STYLES}
`;
