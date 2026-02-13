/**
 * Shared Puppeteer utilities for page interaction commands
 */

import type { Browser, Page } from 'puppeteer-core';

export interface PuppeteerContext {
  browser: Browser;
  page: Page;
}

/**
 * Check if puppeteer is available
 */
export async function checkPuppeteer(): Promise<{ available: boolean; reason: string }> {
  try {
    require.resolve('puppeteer');
    return { available: true, reason: '' };
  } catch {
    try {
      require.resolve('puppeteer-core');
      return { available: true, reason: '' };
    } catch {
      return {
        available: false,
        reason: 'Puppeteer not installed. Run: npm install puppeteer-core',
      };
    }
  }
}

/**
 * Find Chrome/Chromium executable
 */
export async function findChrome(): Promise<string> {
  const { execSync } = await import('child_process');
  const platform = process.platform;

  // Check environment variable first
  if (process.env.PUPPETEER_EXECUTABLE_PATH) {
    return process.env.PUPPETEER_EXECUTABLE_PATH;
  }

  const paths: string[] = [];
  if (platform === 'darwin') {
    paths.push(
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
    );
  } else if (platform === 'linux') {
    paths.push(
      '/usr/bin/google-chrome',
      '/usr/bin/google-chrome-stable',
      '/usr/bin/chromium',
      '/usr/bin/chromium-browser',
    );
  } else if (platform === 'win32') {
    const username = process.env.USERNAME || '';
    paths.push(
      `C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe`,
      `C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe`,
      `C:\\Users\\${username}\\AppData\\Local\\Google\\Chrome\\Application\\chrome.exe`,
    );
  }

  for (const path of paths) {
    try {
      if (platform === 'win32') {
        execSync(`where "${path}" 2>nul`);
      } else {
        execSync(`test -f "${path}"`);
      }
      return path;
    } catch {
      // Try next path
    }
  }

  throw new Error(
    'Chrome/Chromium not found. Install Chrome or set PUPPETEER_EXECUTABLE_PATH environment variable.'
  );
}

/**
 * Get puppeteer module
 */
export async function getPuppeteer(): Promise<typeof import('puppeteer-core')> {
  try {
    return await import('puppeteer-core');
  } catch {
    return await import('puppeteer') as unknown as typeof import('puppeteer-core');
  }
}

/**
 * Launch a browser and navigate to URL
 */
export async function launchAndNavigate(
  url: string,
  waitForSelector?: string
): Promise<PuppeteerContext> {
  const puppeteer = await getPuppeteer();
  const executablePath = await findChrome();

  const browser = await puppeteer.launch({
    executablePath,
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox'],
  });

  const page = await browser.newPage();
  await page.goto(url, { waitUntil: 'networkidle2', timeout: 30000 });

  if (waitForSelector) {
    await page.waitForSelector(waitForSelector, { timeout: 10000 }).catch(() => {
      console.warn(`Selector "${waitForSelector}" not found within timeout`);
    });
  }

  return { browser, page };
}

/**
 * Find a form by ID on the page
 */
export async function findForm(page: Page, formId: string): Promise<boolean> {
  return await page.evaluate((id: string) => {
    // Try by id first
    let form = document.getElementById(id) as HTMLFormElement | null;
    if (form && form.tagName === 'FORM') return true;

    // Try by name
    form = document.querySelector(`form[name="${id}"]`) as HTMLFormElement | null;
    if (form) return true;

    // Try by data attribute or form-index pattern
    if (id.startsWith('form-')) {
      const index = parseInt(id.replace('form-', ''));
      const forms = document.querySelectorAll('form');
      if (forms[index]) return true;
    }

    return false;
  }, formId);
}

/**
 * Get form selector from formId
 */
export function getFormSelector(formId: string): string {
  if (formId.startsWith('form-')) {
    const index = parseInt(formId.replace('form-', ''));
    return `form:nth-of-type(${index + 1})`;
  }
  return `form#${formId}, form[name="${formId}"]`;
}
