/**
 * SettingsStyles - CSS styles for SettingsWidget
 *
 * Extracted for maintainability.
 */

export const SETTINGS_STYLES = `
  :host {
    display: flex;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .settings-layout {
    display: flex;
    flex: 1;
    width: 100%;
    height: 100%;
    gap: 0;
  }

  .settings-main {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
    min-width: 0;
  }

  .settings-assistant {
    flex-shrink: 0;
    height: 100%;
    display: flex;
  }

  .settings-container {
    width: 100%;
  }

  .settings-header {
    margin-bottom: 24px;
  }

  .settings-title {
    font-size: 24px;
    font-weight: 600;
    color: #00d4ff;
    margin: 0 0 8px 0;
  }

  .settings-subtitle {
    color: rgba(255, 255, 255, 0.6);
    font-size: 14px;
  }

  .settings-section {
    background: rgba(15, 20, 25, 0.8);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 8px;
    padding: 20px;
    margin-bottom: 16px;
  }

  .section-title {
    font-size: 16px;
    font-weight: 600;
    color: #00d4ff;
    margin: 0 0 16px 0;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(0, 212, 255, 0.2);
  }

  .config-entry {
    margin-bottom: 16px;
  }

  .config-entry:last-child {
    margin-bottom: 0;
  }

  .config-label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .config-key {
    font-family: monospace;
    font-size: 13px;
    color: #00d4ff;
  }

  .config-description {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .config-input {
    width: 100%;
    padding: 10px 12px;
    background: rgba(0, 10, 15, 0.8);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    color: white;
    font-family: monospace;
    font-size: 14px;
    box-sizing: border-box;
  }

  .config-input:focus {
    outline: none;
    border-color: #00d4ff;
    box-shadow: 0 0 0 2px rgba(0, 212, 255, 0.2);
  }

  .config-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .save-section {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
    margin-top: 24px;
    padding-top: 16px;
    border-top: 1px solid rgba(0, 212, 255, 0.2);
  }

  .storage-note {
    display: block;
    margin-top: 8px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  .storage-note code {
    color: rgba(0, 212, 255, 0.8);
    background: rgba(0, 212, 255, 0.1);
    padding: 2px 6px;
    border-radius: 3px;
    user-select: all;
  }

  .btn-refresh {
    background: none;
    border: none;
    color: rgba(0, 212, 255, 0.6);
    cursor: pointer;
    font-size: 12px;
    padding: 2px 4px;
    margin-left: 4px;
    border-radius: 3px;
    transition: all 0.2s ease;
  }

  .btn-refresh:hover {
    color: #00d4ff;
    background: rgba(0, 212, 255, 0.1);
  }

  .btn {
    padding: 10px 20px;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .btn-primary {
    background: linear-gradient(135deg, #00d4ff, #0099cc);
    border: none;
    color: white;
  }

  .btn-primary:hover {
    background: linear-gradient(135deg, #00e5ff, #00aadd);
    transform: translateY(-1px);
  }

  .btn-secondary {
    background: transparent;
    border: 1px solid rgba(0, 212, 255, 0.4);
    color: #00d4ff;
  }

  .btn-secondary:hover {
    background: rgba(0, 212, 255, 0.1);
  }

  .loading {
    text-align: center;
    padding: 40px;
    color: rgba(255, 255, 255, 0.6);
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

  .status-indicator {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 10px;
  }

  .status-configured {
    background: rgba(0, 255, 100, 0.15);
    color: #00ff64;
  }

  .status-not-set {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.5);
  }

  .provider-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .provider-name {
    font-size: 14px;
    font-weight: 500;
    color: #00d4ff;
  }

  .section-intro {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
    margin-bottom: 16px;
    line-height: 1.5;
  }

  .local-highlight {
    background: rgba(0, 255, 100, 0.1);
    border-color: rgba(0, 255, 100, 0.3);
  }

  .local-highlight .section-title {
    color: #00ff64;
  }

  .provider-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .provider-links {
    display: flex;
    gap: 8px;
  }

  .provider-link {
    font-size: 11px;
    color: #00d4ff;
    text-decoration: none;
    padding: 2px 8px;
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    transition: all 0.2s ease;
  }

  .provider-link:hover {
    background: rgba(0, 212, 255, 0.1);
    border-color: rgba(0, 212, 255, 0.5);
  }

  .input-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .input-row .config-input {
    flex: 1;
  }

  .btn-test {
    padding: 10px 16px;
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    color: #00d4ff;
    font-size: 12px;
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
  }

  .btn-test:hover {
    background: rgba(0, 212, 255, 0.2);
    border-color: rgba(0, 212, 255, 0.5);
  }

  .btn-test:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-test.testing {
    color: rgba(255, 255, 255, 0.6);
  }

  .test-status {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    font-size: 12px;
    padding: 4px 8px;
    border-radius: 4px;
  }

  .test-status.operational {
    background: rgba(0, 255, 100, 0.1);
    color: #00ff64;
  }

  .test-status.invalid {
    background: rgba(255, 50, 50, 0.1);
    color: #ff5050;
  }

  .test-status.out-of-funds {
    background: rgba(255, 200, 50, 0.1);
    color: #ffc832;
  }

  .test-status.rate-limited {
    background: rgba(255, 150, 50, 0.1);
    color: #ff9632;
  }

  .test-status.error {
    background: rgba(255, 50, 50, 0.1);
    color: #ff5050;
  }

  .test-status.testing {
    background: rgba(0, 212, 255, 0.1);
    color: #00d4ff;
  }

  .response-time {
    opacity: 0.6;
    font-size: 11px;
  }

  .status-action {
    margin-left: 8px;
    color: #00d4ff;
    text-decoration: none;
    font-size: 11px;
  }

  .status-action:hover {
    text-decoration: underline;
  }

  .status-action-hint {
    margin-left: 8px;
    opacity: 0.7;
    font-size: 11px;
  }

  @media (max-width: 768px) {
    .settings-layout {
      flex-direction: column;
    }

    .settings-assistant {
      height: 300px;
      flex-shrink: 0;
    }
  }
`;
