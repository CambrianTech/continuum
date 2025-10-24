/**
 * Color Palettes - Reusable heatmap color schemes
 *
 * Each palette is a function that maps normalized value (0-1) to RGB color.
 * Change ONE enum value to switch the entire visualization's color scheme.
 */

export enum ColorPalette {
  TURBO = 'turbo',
  VIRIDIS = 'viridis',
  PLASMA = 'plasma',
  INFERNO = 'inferno',
  COOLWARM = 'coolwarm',
  MAGMA = 'magma'
}

/**
 * Turbo - Google's improved rainbow colormap
 * 0 = blue (cold) → 1 = red (hot)
 */
function turbo(valueNormalized: number): string {
  const t = Math.max(0, Math.min(1, valueNormalized));

  let r: number, g: number, b: number;

  if (t < 0.2) {
    // Blue → Cyan
    const local = t / 0.2;
    r = 0;
    g = Math.floor(local * 255);
    b = 255;
  } else if (t < 0.4) {
    // Cyan → Green
    const local = (t - 0.2) / 0.2;
    r = 0;
    g = 255;
    b = Math.floor((1 - local) * 255);
  } else if (t < 0.6) {
    // Green → Yellow
    const local = (t - 0.4) / 0.2;
    r = Math.floor(local * 255);
    g = 255;
    b = 0;
  } else if (t < 0.8) {
    // Yellow → Orange
    const local = (t - 0.6) / 0.2;
    r = 255;
    g = Math.floor((1 - local * 0.5) * 255);
    b = 0;
  } else {
    // Orange → Red
    const local = (t - 0.8) / 0.2;
    r = 255;
    g = Math.floor((1 - local) * 128);
    b = 0;
  }

  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Viridis - Perceptually uniform, colorblind-friendly
 * 0 = purple (cold) → 1 = yellow (hot)
 */
function viridis(valueNormalized: number): string {
  const t = Math.max(0, Math.min(1, valueNormalized));

  // Simplified viridis approximation
  const r = Math.floor(255 * Math.min(1, Math.max(0, 0.26 + 1.2 * t - 0.5 * t * t)));
  const g = Math.floor(255 * Math.min(1, Math.max(0, t * t * 0.8)));
  const b = Math.floor(255 * Math.min(1, Math.max(0, 0.4 + 0.6 * (1 - t))));

  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Plasma - High contrast, perceptually uniform
 * 0 = dark purple → 1 = bright yellow
 */
function plasma(valueNormalized: number): string {
  const t = Math.max(0, Math.min(1, valueNormalized));

  const r = Math.floor(255 * Math.min(1, Math.max(0, 0.05 + 1.5 * t - 0.6 * t * t)));
  const g = Math.floor(255 * Math.min(1, Math.max(0, t * t * t * 0.9)));
  const b = Math.floor(255 * Math.min(1, Math.max(0, 0.5 - 0.5 * t)));

  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Inferno - Dark background friendly
 * 0 = black → 1 = white/yellow
 */
function inferno(valueNormalized: number): string {
  const t = Math.max(0, Math.min(1, valueNormalized));

  const r = Math.floor(255 * Math.min(1, Math.max(0, t * 1.2)));
  const g = Math.floor(255 * Math.min(1, Math.max(0, (t - 0.3) * 1.5)));
  const b = Math.floor(255 * Math.min(1, Math.max(0, (t - 0.6) * 2)));

  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Coolwarm - Blue-white-red diverging colormap
 * 0 = blue (cold/slow) → 0.5 = white (neutral) → 1 = red (hot/fast)
 */
function coolwarm(valueNormalized: number): string {
  const t = Math.max(0, Math.min(1, valueNormalized));

  let r: number, g: number, b: number;

  if (t < 0.5) {
    // Blue → White (0 to 0.5)
    const local = t * 2; // Map 0-0.5 to 0-1
    r = Math.floor(59 + local * (255 - 59));
    g = Math.floor(76 + local * (255 - 76));
    b = Math.floor(192 + local * (255 - 192));
  } else {
    // White → Red (0.5 to 1)
    const local = (t - 0.5) * 2; // Map 0.5-1 to 0-1
    r = 255;
    g = Math.floor(255 * (1 - local));
    b = Math.floor(255 * (1 - local));
  }

  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Magma - Black → purple → magenta → orange → yellow → white
 * 0 = black (cold) → 1 = white (hot)
 */
function magma(valueNormalized: number): string {
  const t = Math.max(0, Math.min(1, valueNormalized));

  let r: number, g: number, b: number;

  if (t < 0.2) {
    // Black → Dark Purple
    const local = t / 0.2;
    r = Math.floor(local * 50);
    g = Math.floor(local * 10);
    b = Math.floor(local * 80);
  } else if (t < 0.4) {
    // Dark Purple → Magenta
    const local = (t - 0.2) / 0.2;
    r = Math.floor(50 + local * 130);
    g = Math.floor(10 + local * 30);
    b = Math.floor(80 + local * 100);
  } else if (t < 0.6) {
    // Magenta → Red-Orange
    const local = (t - 0.4) / 0.2;
    r = Math.floor(180 + local * 75);
    g = Math.floor(40 + local * 60);
    b = Math.floor(180 - local * 130);
  } else if (t < 0.8) {
    // Red-Orange → Orange-Yellow
    const local = (t - 0.6) / 0.2;
    r = 255;
    g = Math.floor(100 + local * 120);
    b = Math.floor(50 - local * 30);
  } else {
    // Orange-Yellow → White
    const local = (t - 0.8) / 0.2;
    r = 255;
    g = Math.floor(220 + local * 35);
    b = Math.floor(20 + local * 235);
  }

  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Get palette function by enum value
 * Change this ONE place to switch all visualizations
 */
export function getPalette(palette: ColorPalette): (valueNormalized: number) => string {
  switch (palette) {
    case ColorPalette.TURBO:
      return turbo;
    case ColorPalette.VIRIDIS:
      return viridis;
    case ColorPalette.PLASMA:
      return plasma;
    case ColorPalette.INFERNO:
      return inferno;
    case ColorPalette.COOLWARM:
      return coolwarm;
    case ColorPalette.MAGMA:
      return magma;
    default:
      return turbo;
  }
}
