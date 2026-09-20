#!/usr/bin/env node
// android-shot.mjs — the native-Android feedback loop as ONE command: build the Flutter
// app, boot an emulator if none is running, install, launch, and screenshot the live app
// → PNG. The mobile-native arm of shot.mjs ([[feedback-is-a-first-class-cross-modality-dimension-jtag-cu]]).
//
// Resolves the Android SDK from ANDROID_HOME / ANDROID_SDK_ROOT or the per-OS default, and
// Java from JAVA_HOME or Android Studio's bundled JBR. Assumes an AVD exists (create one in
// Android Studio's Device Manager, or `avdmanager create avd`).
//
// Usage:  node scripts/android-shot.mjs [out.png]
// Env:    ANDROID_HOME, JAVA_HOME, ANDROID_AVD (pick a specific AVD), APP_DIR (default apps/mobile)

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import { homedir, platform, tmpdir } from 'node:os';
import { join } from 'node:path';

const OUT = process.argv[2] || join(tmpdir(), 'android-shot.png');
const APP = join(process.cwd(), process.env.APP_DIR || 'apps/mobile');
const PKG = process.env.ANDROID_PKG || 'com.example.continuum_mobile';
const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
const exe = (p) => (platform() === 'win32' ? `${p}.exe` : p);

// ── Resolve the SDK + Java (Android Studio bundles a JBR) ──
const sdk =
  process.env.ANDROID_HOME ||
  process.env.ANDROID_SDK_ROOT ||
  {
    darwin: join(homedir(), 'Library/Android/sdk'),
    linux: join(homedir(), 'Android/Sdk'),
    win32: join(process.env.LOCALAPPDATA || '', 'Android', 'Sdk'),
  }[platform()];
if (!sdk || !existsSync(sdk)) {
  console.error(`android-shot: no Android SDK — set ANDROID_HOME (tried ${sdk}).`);
  process.exit(1);
}
const adb = join(sdk, 'platform-tools', exe('adb'));
const emu = join(sdk, 'emulator', exe('emulator'));
const studioJbr = {
  darwin: '/Applications/Android Studio.app/Contents/jbr/Contents/Home',
  linux: '/opt/android-studio/jbr',
  win32: 'C:\\Program Files\\Android\\Android Studio\\jbr',
}[platform()];
const env = { ...process.env, ANDROID_HOME: sdk };
if (!env.JAVA_HOME && existsSync(studioJbr || '')) env.JAVA_HOME = studioJbr;

const adbOut = (args) => spawnSync(adb, args, { encoding: 'utf8' }).stdout || '';

// ── 1. Ensure a booted device ──
const online = adbOut(['devices']).split('\n').filter((l) => l.endsWith('\tdevice'));
if (online.length === 0) {
  const avds = (spawnSync(emu, ['-list-avds'], { encoding: 'utf8' }).stdout || '')
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean);
  const avd = process.env.ANDROID_AVD || avds[0];
  if (!avd) {
    console.error('android-shot: no AVD found — create one in Android Studio Device Manager.');
    process.exit(1);
  }
  console.log(`android-shot: booting emulator ${avd}…`);
  const child = spawn(
    emu,
    ['-avd', avd, '-no-window', '-no-audio', '-no-snapshot', '-no-boot-anim', '-gpu', 'swiftshader_indirect'],
    { detached: true, stdio: 'ignore', env },
  );
  child.unref();
  spawnSync(adb, ['wait-for-device'], { env });
  let booted = false;
  for (let i = 0; i < 90; i++) {
    if (adbOut(['shell', 'getprop', 'sys.boot_completed']).trim() === '1') {
      booted = true;
      break;
    }
    sleep(3000);
  }
  if (!booted) {
    console.error('android-shot: emulator never finished booting.');
    process.exit(1);
  }
}

// ── 2. Build the debug APK ──
console.log('android-shot: flutter build apk --debug…');
const build = spawnSync('flutter', ['build', 'apk', '--debug'], { cwd: APP, stdio: 'inherit', env });
if (build.status !== 0) {
  console.error('android-shot: flutter build failed.');
  process.exit(1);
}
const apk = join(APP, 'build/app/outputs/flutter-apk/app-debug.apk');

// ── 3. Install + launch ──
spawnSync(adb, ['install', '-r', apk], { env, stdio: 'ignore' });
spawnSync(adb, ['shell', 'am', 'start', '-n', `${PKG}/.MainActivity`], { env, stdio: 'ignore' });
sleep(6000); // let Flutter paint its first frames

// ── 4. Capture ──
const shot = spawnSync(adb, ['exec-out', 'screencap', '-p'], { maxBuffer: 128 * 1024 * 1024 });
if (!shot.stdout || shot.stdout.length === 0) {
  console.error('android-shot: screencap returned no data.');
  process.exit(1);
}
writeFileSync(OUT, shot.stdout);
console.log(`android-shot: ${OUT} (${Math.round(shot.stdout.length / 1024)}KB, native Android)`);
