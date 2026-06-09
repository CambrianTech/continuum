/**
 * Generate static avatar PNGs for all AI personas.
 *
 * Creates colored circle images with the persona's initial letter,
 * using their accent color. These are the BASE avatars — always available,
 * no Bevy, no GPU, no runtime rendering needed.
 *
 * When Bevy 3D is available (local dev, game mode), avatars upgrade to
 * VRM model renders. But these static PNGs are the guaranteed fallback
 * that works in Docker, on headless servers, everywhere.
 *
 * Output: ~/.continuum/avatars/{uniqueId}.png (480x480)
 *
 * Called during `npm run data:seed` — avatars are ready before any
 * browser connects.
 */

import sharp from 'sharp';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';

const AVATAR_SIZE = 480;
const AVATAR_DIR = path.join(os.homedir(), '.continuum', 'avatars');

interface AvatarSpec {
  uniqueId: string;
  displayName: string;
  accentColor: string;
}

/**
 * Generate a single avatar PNG: colored circle with initial letter.
 */
async function generateAvatar(spec: AvatarSpec): Promise<void> {
  const outPath = path.join(AVATAR_DIR, `${spec.uniqueId}.png`);

  // Skip if already exists (don't overwrite Bevy-rendered avatars)
  if (fs.existsSync(outPath)) {
    return;
  }

  const initial = spec.displayName.charAt(0).toUpperCase();
  const size = AVATAR_SIZE;
  const r = size / 2;

  // Parse hex color
  const hex = spec.accentColor.replace('#', '');
  const cr = parseInt(hex.substring(0, 2), 16);
  const cg = parseInt(hex.substring(2, 4), 16);
  const cb = parseInt(hex.substring(4, 6), 16);

  // Darker background variant (30% darker for depth)
  const bgR = Math.round(cr * 0.7);
  const bgG = Math.round(cg * 0.7);
  const bgB = Math.round(cb * 0.7);

  // SVG with gradient circle and centered letter
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
    <defs>
      <radialGradient id="bg" cx="40%" cy="35%" r="65%">
        <stop offset="0%" style="stop-color:rgb(${cr},${cg},${cb});stop-opacity:1" />
        <stop offset="100%" style="stop-color:rgb(${bgR},${bgG},${bgB});stop-opacity:1" />
      </radialGradient>
    </defs>
    <circle cx="${r}" cy="${r}" r="${r}" fill="url(#bg)" />
    <text x="${r}" y="${r}"
          text-anchor="middle"
          dominant-baseline="central"
          font-family="system-ui, -apple-system, 'Segoe UI', Arial, sans-serif"
          font-size="${size * 0.45}"
          font-weight="600"
          fill="white"
          filter="drop-shadow(0 2px 4px rgba(0,0,0,0.3))">
      ${initial}
    </text>
  </svg>`;

  await sharp(Buffer.from(svg))
    .resize(size, size)
    .png()
    .toFile(outPath);
}

/**
 * Generate avatars for all known personas.
 */
export async function generateAllAvatars(personas: AvatarSpec[]): Promise<number> {
  // Ensure avatar directory exists
  fs.mkdirSync(AVATAR_DIR, { recursive: true });

  let generated = 0;
  for (const persona of personas) {
    const outPath = path.join(AVATAR_DIR, `${persona.uniqueId}.png`);
    if (!fs.existsSync(outPath)) {
      await generateAvatar(persona);
      generated++;
    }
  }

  return generated;
}

// CLI entry point
if (require.main === module) {
  // Import persona configs inline to avoid circular deps
  const PERSONAS: AvatarSpec[] = [
    { uniqueId: 'helper', displayName: 'Helper AI', accentColor: '#00d4ff' },
    { uniqueId: 'teacher', displayName: 'Teacher AI', accentColor: '#ff9800' },
    { uniqueId: 'codereview', displayName: 'CodeReview AI', accentColor: '#e91e63' },
    { uniqueId: 'claude', displayName: 'Claude Code', accentColor: '#d4a574' },
    { uniqueId: 'general', displayName: 'General AI', accentColor: '#7c4dff' },
    { uniqueId: 'deepseek', displayName: 'DeepSeek Assistant', accentColor: '#00bcd4' },
    { uniqueId: 'groq', displayName: 'Groq Lightning', accentColor: '#ff5722' },
    { uniqueId: 'claudeassistant', displayName: 'Claude Assistant', accentColor: '#d4a574' },
    { uniqueId: 'gpt', displayName: 'GPT Assistant', accentColor: '#4caf50' },
    { uniqueId: 'grok', displayName: 'Grok', accentColor: '#f44336' },
    { uniqueId: 'together', displayName: 'Together Assistant', accentColor: '#2196f3' },
    { uniqueId: 'fireworks', displayName: 'Fireworks AI', accentColor: '#ff6d00' },
    { uniqueId: 'local', displayName: 'Local Assistant', accentColor: '#8bc34a' },
    { uniqueId: 'gemini', displayName: 'Gemini', accentColor: '#4285f4' },
    { uniqueId: 'qwen3-omni', displayName: 'Qwen3-Omni', accentColor: '#6c5ce7' },
    { uniqueId: 'geminilive', displayName: 'Gemini Live', accentColor: '#34a853' },
    { uniqueId: 'sentinel', displayName: 'Sentinel', accentColor: '#ff1744' },
  ];

  generateAllAvatars(PERSONAS).then(count => {
    console.log(`✅ Generated ${count} avatar PNGs (${PERSONAS.length - count} already existed)`);
  }).catch(err => {
    console.error('❌ Avatar generation failed:', err);
    process.exit(1);
  });
}
