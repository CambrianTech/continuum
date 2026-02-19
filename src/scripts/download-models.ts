#!/usr/bin/env tsx
/**
 * Model Download Script
 *
 * Automatically downloads required ML models for streaming-core.
 * Run via: npm run models:download
 * Or automatically via postinstall hook.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as https from 'https';
import { IncomingMessage } from 'http';

interface ModelManifest {
  models: Array<{
    name: string;
    type: string;
    required: boolean;
    path: string;
    url: string;
    size: string;
    sha256: string | null;
    description: string;
  }>;
}

const MODELS_DIR = path.join(__dirname, '../workers/streaming-core/models');
const MANIFEST_PATH = path.join(__dirname, '../workers/streaming-core/models.json');

/**
 * Download file with progress
 */
async function downloadFile(url: string, destPath: string, expectedSize: string): Promise<void> {
  return new Promise((resolve, reject) => {
    // Ensure directory exists
    const dir = path.dirname(destPath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    const file = fs.createWriteStream(destPath);
    let downloadedBytes = 0;

    https.get(url, { headers: { 'User-Agent': 'continuum-model-downloader' } }, (response: IncomingMessage) => {
      // Handle redirects
      if (response.statusCode === 301 || response.statusCode === 302) {
        const redirectUrl = response.headers.location;
        if (!redirectUrl) {
          reject(new Error('Redirect without location header'));
          return;
        }
        file.close();
        fs.unlinkSync(destPath);
        downloadFile(redirectUrl, destPath, expectedSize).then(resolve).catch(reject);
        return;
      }

      if (response.statusCode !== 200) {
        reject(new Error(`HTTP ${response.statusCode}: ${response.statusMessage}`));
        return;
      }

      const totalBytes = parseInt(response.headers['content-length'] || '0', 10);

      response.on('data', (chunk: Buffer) => {
        downloadedBytes += chunk.length;
        const progress = totalBytes > 0 ? ((downloadedBytes / totalBytes) * 100).toFixed(1) : '?';
        const downloadedMB = (downloadedBytes / 1024 / 1024).toFixed(1);
        process.stdout.write(`\r   Downloading: ${downloadedMB}MB (${progress}%)`);
      });

      response.pipe(file);

      file.on('finish', () => {
        file.close();
        console.log('\r   ✅ Download complete                    ');
        resolve();
      });

      file.on('error', (err: Error) => {
        fs.unlinkSync(destPath);
        reject(err);
      });
    }).on('error', (err: Error) => {
      fs.unlinkSync(destPath);
      reject(err);
    });
  });
}

/**
 * Check if model exists and is valid
 */
function modelExists(modelPath: string): boolean {
  const fullPath = path.join(MODELS_DIR, modelPath.replace('models/', ''));
  if (!fs.existsSync(fullPath)) {
    return false;
  }

  const stats = fs.statSync(fullPath);
  // Basic validation: file must be > 1KB (not corrupted/empty)
  return stats.size > 1024;
}

/**
 * Main download function
 */
async function downloadModels(force: boolean = false): Promise<void> {
  console.log('🤖 Continuum Model Downloader\n');

  // Load manifest
  if (!fs.existsSync(MANIFEST_PATH)) {
    console.error(`❌ Model manifest not found: ${MANIFEST_PATH}`);
    process.exit(1);
  }

  const manifest: ModelManifest = JSON.parse(fs.readFileSync(MANIFEST_PATH, 'utf-8'));
  const requiredModels = manifest.models.filter(m => m.required);

  console.log(`📦 Found ${requiredModels.length} required models\n`);

  let downloadCount = 0;
  let skipCount = 0;

  for (const model of requiredModels) {
    const destPath = path.join(MODELS_DIR, model.path.replace('models/', ''));
    const exists = modelExists(model.path);

    console.log(`\n📄 ${model.name}`);
    console.log(`   Type: ${model.type}`);
    console.log(`   Size: ${model.size}`);
    console.log(`   Path: ${model.path}`);

    if (exists && !force) {
      console.log(`   ✅ Already exists (skipping)`);
      skipCount++;
      continue;
    }

    if (exists && force) {
      console.log(`   🔄 Re-downloading (--force)`);
    }

    try {
      console.log(`   ⬇️  Downloading from HuggingFace...`);
      await downloadFile(model.url, destPath, model.size);
      downloadCount++;
    } catch (err) {
      console.error(`   ❌ Download failed: ${err}`);
      console.error(`\n⚠️  Manual download instructions:`);
      console.error(`   URL: ${model.url}`);
      console.error(`   Save to: ${destPath}\n`);
      process.exit(1);
    }
  }

  console.log(`\n✅ Model download complete!`);
  console.log(`   Downloaded: ${downloadCount}`);
  console.log(`   Skipped: ${skipCount}`);
  console.log(`   Total: ${requiredModels.length}\n`);

  // Verify all models exist
  const missing = requiredModels.filter(m => !modelExists(m.path));
  if (missing.length > 0) {
    console.error(`❌ Still missing ${missing.length} models:`);
    missing.forEach(m => console.error(`   - ${m.name}`));
    process.exit(1);
  }

  console.log('🎉 All models ready!\n');
}

// Parse args
const args = process.argv.slice(2);
const force = args.includes('--force');

// Run
downloadModels(force).catch((err) => {
  console.error('❌ Fatal error:', err);
  process.exit(1);
});
