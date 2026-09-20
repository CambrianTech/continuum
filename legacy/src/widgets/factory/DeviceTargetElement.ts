/**
 * DeviceTargetElement — Live device compatibility matrix
 *
 * Shows which devices can run the forged model based on current pipeline config.
 * Estimates final size from base model + prune level + compact + quant format.
 * Devices can be locked as hard requirements — pipeline must produce a model that fits.
 *
 * Renders at the bottom of the forge console as a visual summary of "what am I building?"
 */

import {
  ReactiveWidget,
  html,
  css,
  reactive,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';

interface DeviceSpec {
  id: string;
  label: string;
  icon: string;
  vramMb: number;
  category: 'phone' | 'laptop' | 'desktop';
}

interface DeviceFit {
  device: DeviceSpec;
  fits: boolean;
  headroomPct: number;     // How much VRAM left (negative = doesn't fit)
  estimatedTokS: number;   // Rough tok/s estimate
  bestQuant: string;       // Best quant format that fits
  locked: boolean;         // Hard requirement — must fit
}

const DEVICES: DeviceSpec[] = [
  { id: 'iphone',        label: 'iPhone',       icon: '\u{1F4F1}', vramMb: 6144,  category: 'phone' },
  { id: 'air-8gb',       label: 'Air 8GB',      icon: '\u{1F4BB}', vramMb: 6400,  category: 'laptop' },
  { id: 'air-16gb',      label: 'Air 16GB',     icon: '\u{1F4BB}', vramMb: 12800, category: 'laptop' },
  { id: 'pro-18gb',      label: 'Pro 18GB',     icon: '\u{1F4BB}', vramMb: 14400, category: 'laptop' },
  { id: 'pro-36gb',      label: 'Pro 36GB',     icon: '\u{1F4BB}', vramMb: 28800, category: 'laptop' },
  { id: 'rtx-3090',      label: '3090',         icon: '\u{1F5A5}', vramMb: 24576, category: 'desktop' },
  { id: 'rtx-4090',      label: '4090',         icon: '\u{1F5A5}', vramMb: 24576, category: 'desktop' },
  { id: 'rtx-5090',      label: '5090',         icon: '\u{1F5A5}', vramMb: 32768, category: 'desktop' },
];

// Quant format size multipliers relative to fp16
const QUANT_MULTIPLIERS: Record<string, number> = {
  'F16': 1.0, 'Q8_0': 0.53, 'Q6_K': 0.41, 'Q5_K_M': 0.36,
  'Q4_K_M': 0.30, 'Q4_K_S': 0.28, 'Q3_K_M': 0.24, 'Q2_K': 0.18,
};

export class DeviceTargetElement extends ReactiveWidget {

  // These are set as properties by the parent ForgeControlsElement
  @reactive() baseModelGb = 8;
  @reactive() pruneLevel = 0.3;
  @reactive() quantFormats: string[] = ['Q4_K_M', 'Q8_0'];
  @reactive() private _lockedDevices: Set<string> = new Set();
  @reactive() compactEnabled = false;

  constructor() {
    super({ widgetName: 'DeviceTargetElement' });
  }

  // Properties set by parent ForgeControlsElement via Lit property binding.
  // No event subscription needed — Lit re-renders when properties change.

  private toggleLock(deviceId: string): void {
    const next = new Set(this._lockedDevices);
    if (next.has(deviceId)) next.delete(deviceId);
    else next.add(deviceId);
    this._lockedDevices = next;
  }

  private get fits(): DeviceFit[] {
    // Estimate post-prune size
    const prunedGb = this.baseModelGb * (1 - this.pruneLevel * 0.6); // Pruning doesn't remove 1:1
    const compactedGb = this.compactEnabled ? prunedGb * 0.7 : prunedGb;

    return DEVICES.map(device => {
      const deviceGb = device.vramMb / 1024;

      // Find best quant that fits
      let bestQuant = 'F16';
      let bestSizeGb = compactedGb;

      for (const [format, mult] of Object.entries(QUANT_MULTIPLIERS).sort((a, b) => b[1] - a[1])) {
        const sizeGb = compactedGb * mult;
        if (sizeGb <= deviceGb * 0.85 && this.quantFormats.some(q => q === format || format === 'Q4_K_M')) {
          bestQuant = format;
          bestSizeGb = sizeGb;
          break;
        }
      }

      // Check all quant formats
      const fits = bestSizeGb <= deviceGb * 0.85; // 85% VRAM — leave room for KV cache
      const headroomPct = fits ? Math.round(((deviceGb * 0.85 - bestSizeGb) / deviceGb) * 100) : Math.round(((bestSizeGb - deviceGb * 0.85) / deviceGb) * -100);

      // Rough tok/s estimate based on device category and model size
      let tokS = 0;
      if (fits) {
        const ratio = bestSizeGb / deviceGb;
        if (device.category === 'phone') tokS = Math.round(10 * (1 - ratio));
        else if (device.category === 'laptop') tokS = Math.round(30 * (1 - ratio));
        else tokS = Math.round(100 * (1 - ratio));
      }

      return {
        device,
        fits,
        headroomPct,
        estimatedTokS: tokS,
        bestQuant: fits ? bestQuant : '-',
        locked: this._lockedDevices.has(device.id),
      };
    });
  }

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        margin-top: 12px;
      }

      .target-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 8px;
      }

      .target-title {
        font-size: 9px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: var(--content-secondary, #8a92a5);
      }

      .target-estimate {
        font-size: 10px;
        color: var(--content-tertiary, #5a6070);
      }

      .device-grid {
        display: grid;
        grid-template-columns: repeat(4, 1fr);
        gap: 4px;
      }

      .device-card {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 3px;
        padding: 8px 4px;
        border: 1px solid rgba(255,255,255,0.06);
        border-radius: 6px;
        background: rgba(0,0,0,0.2);
        cursor: pointer;
        transition: all 0.2s;
        position: relative;
      }

      .device-card:hover {
        border-color: rgba(255,255,255,0.15);
      }

      .device-card.fits {
        border-color: rgba(0, 255, 200, 0.2);
        background: rgba(0, 255, 200, 0.03);
      }

      .device-card.tight {
        border-color: rgba(255, 170, 0, 0.2);
        background: rgba(255, 170, 0, 0.03);
      }

      .device-card.nope {
        border-color: rgba(255, 68, 68, 0.1);
        opacity: 0.4;
      }

      .device-card.locked {
        border-color: rgba(0, 212, 255, 0.5);
        box-shadow: 0 0 6px rgba(0, 212, 255, 0.15);
      }

      .lock-indicator {
        position: absolute;
        top: 2px;
        right: 3px;
        font-size: 8px;
        color: #00d4ff;
      }

      .device-icon {
        font-size: 10px;
        font-weight: 800;
        width: 20px;
        height: 20px;
        border-radius: 50%;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(255,255,255,0.06);
        color: var(--content-secondary, #8a92a5);
      }

      .device-card.fits .device-icon {
        background: rgba(0, 255, 200, 0.15);
        color: #00ffc8;
      }

      .device-label {
        font-size: 8px;
        font-weight: 600;
        color: var(--content-secondary, #8a92a5);
      }

      .device-quant {
        font-size: 7px;
        font-family: monospace;
        color: var(--content-tertiary, #5a6070);
      }

      .device-card.fits .device-quant {
        color: #00ffc8;
      }

      .device-toks {
        font-size: 8px;
        font-weight: 700;
        color: var(--content-tertiary, #5a6070);
      }

      .device-card.fits .device-toks {
        color: var(--content-secondary, #8a92a5);
      }

      .headroom-bar {
        width: 100%;
        height: 2px;
        border-radius: 1px;
        background: rgba(255,255,255,0.06);
        overflow: hidden;
        margin-top: 2px;
      }

      .headroom-fill {
        height: 100%;
        border-radius: 1px;
        transition: width 0.3s;
      }

      .headroom-fill.good { background: #00ffc8; }
      .headroom-fill.tight { background: #ffaa00; }
      .headroom-fill.over { background: #ff4444; }
    `,
  ];

  protected override render(): TemplateResult {
    const results = this.fits;
    const fitsCount = results.filter(r => r.fits).length;
    const lockedCount = this._lockedDevices.size;
    const lockedAllFit = results.filter(r => r.locked).every(r => r.fits);

    return html`
      <div class="target-header">
        <span class="target-title">Device Targets ${fitsCount}/${DEVICES.length} fit${lockedCount > 0 ? html` · ${lockedCount} locked${lockedAllFit ? '' : html` <span style="color:#ff4444">&#9888;</span>`}` : nothing}</span>
        <span class="target-estimate">~${this.baseModelGb}GB fp16 → ~${(this.baseModelGb * (1 - this.pruneLevel * 0.6) * 0.30).toFixed(1)}GB Q4</span>
      </div>
      <div class="device-grid">
        ${results.map(r => this.renderDeviceCard(r))}
      </div>
    `;
  }

  private renderDeviceCard(r: DeviceFit): TemplateResult {
    const cls = r.fits ? (r.headroomPct > 20 ? 'fits' : 'tight') : 'nope';
    const barPct = r.fits ? Math.min(100, 100 - r.headroomPct) : 100;
    const barCls = r.fits ? (r.headroomPct > 20 ? 'good' : 'tight') : 'over';

    return html`
      <div class="device-card ${cls} ${r.locked ? 'locked' : ''}"
        @click=${() => this.toggleLock(r.device.id)}
        title="${r.locked ? 'Click to unlock' : 'Click to lock as requirement'}${r.fits ? ` · ${r.headroomPct}% headroom` : ' · Does not fit'}">
        ${r.locked ? html`<span class="lock-indicator">&#128274;</span>` : nothing}
        <div class="device-icon">${r.device.icon}</div>
        <span class="device-label">${r.device.label}</span>
        <span class="device-quant">${r.bestQuant}</span>
        <span class="device-toks">${r.fits ? `~${r.estimatedTokS} t/s` : 'N/A'}</span>
        <div class="headroom-bar">
          <div class="headroom-fill ${barCls}" style="width:${barPct}%"></div>
        </div>
      </div>
    `;
  }
}

if (!customElements.get('device-target-element')) {
  customElements.define('device-target-element', DeviceTargetElement);
}
