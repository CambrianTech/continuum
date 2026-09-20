/**
 * Brain SVG Template
 *
 * The main brain visualization with clickable modules and logging toggles.
 */

export interface BrainModuleData {
  id: string;
  status: 'active' | 'idle' | 'error';
  selected: boolean;
  label: string;
  sublabel: string;
  stat: string;
  logCategory: string;
  logEnabled: boolean;
  logDisabled: boolean;
  memoryCount?: number;
  ltmSize?: string;
  toolsAvailable?: number;
  connections?: number;
}

export interface BrainSVGData {
  modules: {
    prefrontal: BrainModuleData;
    limbic: BrainModuleData;
    hippocampus: BrainModuleData;
    motorCortex: BrainModuleData;
    cns: BrainModuleData;
  };
  readouts: {
    status: string;
    provider: string;
    model: string;
    memories: string;
  };
}

const SVG_DEFS = `
  <defs>
    <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
      <feGaussianBlur stdDeviation="3" result="coloredBlur"/>
      <feMerge>
        <feMergeNode in="coloredBlur"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
    <filter id="innerGlow">
      <feGaussianBlur stdDeviation="2" result="blur"/>
      <feComposite in="SourceGraphic" in2="blur" operator="over"/>
    </filter>
    <linearGradient id="brainGradient" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:#1a1a2e;stop-opacity:0.9" />
      <stop offset="100%" style="stop-color:#16213e;stop-opacity:0.9" />
    </linearGradient>
    <linearGradient id="activeGradient" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" style="stop-color:#00ff88;stop-opacity:0.3" />
      <stop offset="100%" style="stop-color:#00ff88;stop-opacity:0.1" />
    </linearGradient>
    <pattern id="gridPattern" width="20" height="20" patternUnits="userSpaceOnUse">
      <path d="M 20 0 L 0 0 0 20" fill="none" stroke="rgba(0, 255, 136, 0.05)" stroke-width="0.5"/>
    </pattern>
  </defs>
`;

const BRAIN_SHAPE = `
  <ellipse cx="200" cy="180" rx="160" ry="140"
    fill="url(#brainGradient)"
    stroke="rgba(0, 255, 136, 0.3)"
    stroke-width="2"/>
  <path d="M 200 40 Q 280 60, 320 120 Q 360 180, 340 240 Q 320 300, 260 320
           Q 200 340, 140 320 Q 80 300, 60 240 Q 40 180, 80 120 Q 120 60, 200 40"
    fill="none"
    stroke="rgba(0, 255, 136, 0.2)"
    stroke-width="1"
    stroke-dasharray="4,4"/>
`;

const NEURAL_NODES = `
  <g class="neural-nodes" opacity="0.4">
    <circle cx="120" cy="100" r="2" fill="#00ff88"/>
    <circle cx="280" cy="100" r="2" fill="#00ff88"/>
    <circle cx="100" cy="180" r="2" fill="#00ff88"/>
    <circle cx="300" cy="180" r="2" fill="#00ff88"/>
    <circle cx="140" cy="260" r="2" fill="#00ff88"/>
    <circle cx="260" cy="260" r="2" fill="#00ff88"/>
    <line x1="120" y1="100" x2="200" y2="80" stroke="#00ff88" stroke-width="0.5" opacity="0.3"/>
    <line x1="280" y1="100" x2="200" y2="80" stroke="#00ff88" stroke-width="0.5" opacity="0.3"/>
    <line x1="100" y1="180" x2="200" y2="180" stroke="#00ff88" stroke-width="0.5" opacity="0.3"/>
    <line x1="300" y1="180" x2="200" y2="180" stroke="#00ff88" stroke-width="0.5" opacity="0.3"/>
    <line x1="140" y1="260" x2="200" y2="280" stroke="#00ff88" stroke-width="0.5" opacity="0.3"/>
    <line x1="260" y1="260" x2="200" y2="280" stroke="#00ff88" stroke-width="0.5" opacity="0.3"/>
  </g>
`;

const HUD_RING = `
  <circle cx="200" cy="180" r="170" fill="none" stroke="rgba(0, 255, 136, 0.1)" stroke-width="1"/>
  <circle cx="200" cy="180" r="175" fill="none" stroke="rgba(0, 255, 136, 0.05)" stroke-width="1" stroke-dasharray="2,8"/>
`;

function moduleClass(mod: BrainModuleData): string {
  let cls = 'brain-module module-' + mod.status;
  if (mod.selected) cls += ' selected';
  return cls;
}

function logToggle(mod: BrainModuleData, x: number, y: number): string {
  const enabledClass = mod.logEnabled ? 'enabled' : '';
  const disabledClass = mod.logDisabled ? 'disabled' : '';
  const fillColor = mod.logEnabled ? 'rgba(0, 255, 136, 0.3)' : 'rgba(255, 255, 255, 0.1)';
  const strokeColor = mod.logEnabled ? '#00ff88' : 'rgba(255, 255, 255, 0.3)';
  const textColor = mod.logEnabled ? '#00ff88' : 'rgba(255, 255, 255, 0.5)';
  const text = mod.logEnabled ? 'L' : '-';
  return '<g class="module-log-toggle ' + enabledClass + ' ' + disabledClass + '" data-category="' + mod.logCategory + '" transform="translate(' + x + ', ' + y + ')"><circle r="8" fill="' + fillColor + '" stroke="' + strokeColor + '" stroke-width="1"/><text x="0" y="4" text-anchor="middle" fill="' + textColor + '" font-size="8">' + text + '</text></g>';
}

function renderModule(mod: BrainModuleData, cx: number, cy: number, rx: number, ry: number, labelY: number, sublabelY: number, logX: number, logY: number, extraContent?: string): string {
  const fill = mod.status === 'active' ? 'url(#activeGradient)' : 'rgba(26, 26, 46, 0.8)';
  const stroke = mod.status === 'active' ? '#00ff88' : 'rgba(255, 255, 255, 0.2)';
  const strokeWidth = mod.selected ? 2 : 1;
  const filter = mod.status === 'active' ? 'url(#glow)' : 'none';
  
  return '<g class="' + moduleClass(mod) + '" data-module="' + mod.id + '">' +
    '<ellipse cx="' + cx + '" cy="' + cy + '" rx="' + rx + '" ry="' + ry + '" fill="' + fill + '" stroke="' + stroke + '" stroke-width="' + strokeWidth + '" filter="' + filter + '"/>' +
    '<text x="' + cx + '" y="' + labelY + '" text-anchor="middle" class="module-label">' + mod.label + '</text>' +
    '<text x="' + cx + '" y="' + sublabelY + '" text-anchor="middle" class="module-sublabel">' + mod.sublabel + '</text>' +
    (extraContent || '') +
    logToggle(mod, logX, logY) +
    '</g>';
}

export function renderBrainSVG(data: BrainSVGData): string {
  const { modules, readouts } = data;

  const prefrontal = renderModule(modules.prefrontal, 200, 80, 60, 30, 75, 90, 265, 80);
  const limbic = renderModule(modules.limbic, 100, 160, 45, 35, 155, 170, 150, 155);
  const motorCortex = renderModule(modules.motorCortex, 300, 160, 45, 35, 150, 165, 350, 155);
  const hippocampus = renderModule(modules.hippocampus, 200, 220, 55, 40, 215, 230, 260, 220,
    '<text x="200" y="250" text-anchor="middle" class="module-stat">' + modules.hippocampus.stat + ' MEMORIES</text>');
  const cns = renderModule(modules.cns, 200, 310, 50, 25, 305, 320, 255, 310);

  const hudReadouts = '<g class="hud-readouts">' +
    '<text x="30" y="30" class="hud-label">STATUS</text>' +
    '<text x="30" y="50" class="hud-value status-' + readouts.status.toLowerCase() + '">' + readouts.status + '</text>' +
    '<text x="370" y="30" text-anchor="end" class="hud-label">PROVIDER</text>' +
    '<text x="370" y="50" text-anchor="end" class="hud-value">' + readouts.provider + '</text>' +
    '<text x="30" y="370" class="hud-label">MODEL</text>' +
    '<text x="30" y="390" class="hud-value">' + readouts.model + '</text>' +
    '<text x="370" y="370" text-anchor="end" class="hud-label">LTM</text>' +
    '<text x="370" y="390" text-anchor="end" class="hud-value">' + readouts.memories + '</text>' +
    '</g>';

  return '<svg viewBox="0 0 400 400" class="brain-svg">' +
    SVG_DEFS +
    '<rect width="400" height="400" fill="url(#gridPattern)" opacity="0.3"/>' +
    HUD_RING +
    BRAIN_SHAPE +
    NEURAL_NODES +
    prefrontal +
    limbic +
    motorCortex +
    hippocampus +
    cns +
    hudReadouts +
    '</svg>';
}
