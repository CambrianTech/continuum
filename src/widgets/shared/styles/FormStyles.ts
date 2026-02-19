/**
 * FormStyles - Shared CSS for form elements
 *
 * Buttons, inputs, and interactive controls used across panel widgets.
 * Consistent cyberpunk aesthetic with cyan/green color scheme.
 */

/**
 * Input field styles
 */
export const INPUT_STYLES = `
  .form-input,
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
    transition: border-color 0.2s ease, box-shadow 0.2s ease;
  }

  .form-input:focus,
  .config-input:focus {
    outline: none;
    border-color: var(--content-accent, #00d4ff);
    box-shadow: 0 0 0 2px var(--input-focus-shadow, rgba(0, 212, 255, 0.2));
  }

  .form-input::placeholder,
  .config-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .form-input:disabled,
  .config-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .input-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .input-row .form-input,
  .input-row .config-input {
    flex: 1;
  }

  .input-label {
    display: block;
    margin-bottom: 6px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
  }

  .input-hint {
    display: block;
    margin-top: 4px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  .input-error {
    display: block;
    margin-top: 4px;
    font-size: 11px;
    color: #ff5050;
  }
`;

/**
 * Button styles - primary, secondary, and utility buttons
 */
export const BUTTON_STYLES = `
  .btn {
    padding: 10px 20px;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
    border: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary {
    background: var(--button-primary-background, linear-gradient(135deg, #00d4ff, #0099cc));
    color: var(--button-primary-text, white);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--button-primary-background-hover, linear-gradient(135deg, #00e5ff, #00aadd));
    transform: translateY(-1px);
  }

  .btn-secondary {
    background: var(--button-secondary-background, transparent);
    border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.4));
    color: var(--content-accent, #00d4ff);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--button-secondary-background-hover, rgba(0, 212, 255, 0.1));
    border-color: var(--widget-border-focus, rgba(0, 212, 255, 0.6));
  }

  .btn-success {
    background: linear-gradient(135deg, var(--content-success, #00ff64), #00cc50);
    color: white;
  }

  .btn-success:hover:not(:disabled) {
    background: linear-gradient(135deg, #00ff78, #00dd5a);
  }

  .btn-danger {
    background: linear-gradient(135deg, var(--content-warning, #ff5050), #cc3030);
    color: white;
  }

  .btn-danger:hover:not(:disabled) {
    background: linear-gradient(135deg, #ff6060, #dd4040);
  }

  .btn-sm {
    padding: 6px 12px;
    font-size: 12px;
  }

  .btn-lg {
    padding: 14px 28px;
    font-size: 16px;
  }

  .btn-icon {
    padding: 8px;
    min-width: 36px;
    justify-content: center;
  }

  .btn-test {
    padding: 10px 16px;
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid var(--widget-border, rgba(0, 212, 255, 0.3));
    border-radius: 4px;
    color: var(--content-accent, #00d4ff);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
  }

  .btn-test:hover:not(:disabled) {
    background: rgba(0, 212, 255, 0.2);
    border-color: var(--widget-border-focus, rgba(0, 212, 255, 0.5));
  }

  .btn-test.testing {
    color: rgba(255, 255, 255, 0.6);
  }

  .btn-refresh {
    background: none;
    border: none;
    color: var(--content-secondary, rgba(0, 212, 255, 0.6));
    cursor: pointer;
    font-size: 12px;
    padding: 2px 4px;
    margin-left: 4px;
    border-radius: 3px;
    transition: all 0.2s ease;
  }

  .btn-refresh:hover {
    color: var(--content-accent, #00d4ff);
    background: rgba(0, 212, 255, 0.1);
  }
`;

/**
 * Link styles
 */
export const LINK_STYLES = `
  a {
    color: var(--content-accent, #00d4ff);
    text-decoration: none;
    transition: color 0.2s ease;
  }

  a:hover {
    color: #00e5ff;
    text-decoration: underline;
  }

  .link-subtle {
    color: var(--content-secondary, rgba(0, 212, 255, 0.7));
    font-size: 12px;
  }

  .link-subtle:hover {
    color: var(--content-accent, #00d4ff);
  }

  .action-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    font-size: 11px;
    transition: all 0.2s ease;
  }

  .action-link:hover {
    background: rgba(0, 212, 255, 0.1);
    border-color: rgba(0, 212, 255, 0.5);
    text-decoration: none;
  }
`;

/**
 * Form section styles (save buttons, etc.)
 */
export const FORM_SECTION_STYLES = `
  .form-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
    margin-top: 24px;
    padding-top: 16px;
    border-top: 1px solid rgba(0, 212, 255, 0.2);
  }

  .form-group {
    margin-bottom: 16px;
  }

  .form-group:last-child {
    margin-bottom: 0;
  }

  .form-row {
    display: flex;
    gap: 16px;
  }

  .form-row > * {
    flex: 1;
  }
`;

/**
 * Combined form styles
 */
export const FORM_STYLES = `
  ${INPUT_STYLES}
  ${BUTTON_STYLES}
  ${LINK_STYLES}
  ${FORM_SECTION_STYLES}
`;
