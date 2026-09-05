/**
 * `DomSurface` — the `Surface` for a web page, via Playwright. Outlier A of #187:
 * ONE concrete, complete implementation of all three channels, from which the
 * `Surface` trait is validated (a 3D/video Surface is outlier B). Playwright is the
 * reference DOM surface because it hands you pixels (screenshot), STRUCTURE (DOM walk
 * + accessibility tree), and a DRIVER (click/type/hot-swap) in one auto-waiting API.
 *
 * Reuses the machine's installed Chrome via `channel: 'chrome'` — no 150MB browser
 * download forced on a public user ([[solve-for-public-users]]); pass `channel: null`
 * to use Playwright's bundled Chromium instead.
 *
 * ## This is an ADAPTER that lives on an eye-node — NOT a core dependency
 *
 * The headless Rust core never renders and never assumes a browser is present (a datacenter
 * rack instance has none). `DomSurface` runs wherever a browser exists — a laptop client, or
 * a render-worker node that CHOSE to install a Chromium-family browser — and registers the
 * `perception/*` capability from there via `WireShape::Provided` (one command name, N
 * adapters, like `interface/screenshot`). The core routes to it by capability, never links
 * or spawns it, and never calls `findChromium()`. So: nothing here may be pulled into the
 * core's build or startup — if a browserless rack can't run the core, this seam is wrong.
 * See README.md § "the core is agnostic and browserless".
 */

import { existsSync } from 'node:fs';
import { platform } from 'node:os';
import { chromium, type Browser, type LaunchOptions, type Page } from 'playwright';
import { imageDiff } from './imageDiff';
import {
  ActError,
  type Delta,
  type Percept,
  type ProbeNode,
  type SetViewportAction,
  type StructuredState,
  type Surface,
  type ViewSpec,
} from './surface';

/** The DOM surface's view-hints (outlier A): the neutral `ViewSpec` plus the web-specific
 *  clip/theme/full-page knobs. This is one of the two surface-flavored axes the trait is
 *  generic over — a 3D surface's `SceneViewSpec` carries a camera here instead. */
export interface DomViewSpec extends ViewSpec {
  /** Clip the render to this element (a CSS selector). Omit = whole view. */
  readonly selector?: string;
  /** Force a colour scheme (UI surfaces honour `prefers-color-scheme`). */
  readonly theme?: 'light' | 'dark';
  /** Capture the full scrollable page, not just the viewport. */
  readonly fullPage?: boolean;
}

/** The DOM surface's act-verbs (outlier A): the universal base `Action` (`setViewport`)
 *  plus the web driver — click/type/hot-swap CSS. `injectCss` is the hot-swap seam
 *  (retheme/relayout the LIVE page, no redeploy) that makes design iteration fast;
 *  `hotPatchCss` is its idempotent sibling — ONE named hot-patch layer
 *  (`<style data-continuum-hot-edit>`) REPLACED wholesale each call (empty css clears
 *  it), so `perception/hot-edit` can re-apply a persona's full accumulated stylesheet
 *  without stacking style tags. */
export type DomAction =
  | SetViewportAction
  | { readonly kind: 'click'; readonly selector: string }
  | { readonly kind: 'type'; readonly selector: string; readonly text: string }
  | { readonly kind: 'press'; readonly key: string }
  | { readonly kind: 'hover'; readonly selector: string }
  | { readonly kind: 'injectCss'; readonly css: string }
  | { readonly kind: 'hotPatchCss'; readonly css: string };

export interface DomSurfaceOptions {
  readonly url: string;
  readonly viewport?: { readonly width: number; readonly height: number };
  readonly headless?: boolean;
  /** Pixel density of the capture. Default 1 — retina (2) is an opt-in for
   *  tasks grading fine detail; it quadruples every screenshot's bytes. */
  readonly deviceScaleFactor?: number;
  /** Force a specific Chromium binary (Opera GX / Brave / Chromium / any). Highest priority.
   *  Env fallbacks: `PERCEPTION_CHROME`, then `CHROME`. */
  readonly executablePath?: string;
  /** A Playwright channel (`'chrome'`, `'msedge'`) to reuse an installed browser. */
  readonly channel?: 'chrome' | 'msedge';
  /** Force Playwright's bundled Chromium (reproducible renders) instead of a system browser. */
  readonly bundled?: boolean;
}

/** Common Chromium-family binaries per OS — the "chromium-compatible browsers are our
 *  expectation" set (Chrome, Chromium, Brave, Edge, Opera/Opera GX). First hit wins.
 *  Exported so a caller can report which browser perception will use. */
export function findChromium(): string | undefined {
  const candidates: Record<string, readonly string[]> = {
    darwin: [
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
      '/Applications/Brave Browser.app/Contents/MacOS/Brave Browser',
      '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
      '/Applications/Opera GX.app/Contents/MacOS/Opera',
      '/Applications/Opera.app/Contents/MacOS/Opera',
    ],
    win32: [
      'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
      'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
      'C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe',
      'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    ],
    linux: [
      '/usr/bin/google-chrome',
      '/usr/bin/google-chrome-stable',
      '/usr/bin/chromium',
      '/usr/bin/chromium-browser',
      '/usr/bin/brave-browser',
      '/usr/bin/microsoft-edge',
    ],
  };
  const list = candidates[platform()] ?? [];
  return list.find((p) => existsSync(p));
}

/** Resolve HOW to launch Chromium, in priority order:
 *  1. explicit `executablePath` (opt → `PERCEPTION_CHROME` → `CHROME`) — any Chromium binary;
 *  2. explicit `channel` (opt → `PERCEPTION_CHANNEL`);
 *  3. `bundled: true` → Playwright's own Chromium (deterministic);
 *  4. a discovered system Chromium (no download, matches whatever the user runs);
 *  5. fall through to Playwright's bundled Chromium.
 *  Fail-loud only happens at `chromium.launch` if the resolved binary is unusable. */
function resolveLaunch(opts: DomSurfaceOptions): LaunchOptions {
  const exe = opts.executablePath ?? process.env.PERCEPTION_CHROME ?? process.env.CHROME;
  if (exe) return { executablePath: exe };
  const channel = opts.channel ?? (process.env.PERCEPTION_CHANNEL as 'chrome' | 'msedge' | undefined);
  if (channel) return { channel };
  if (opts.bundled) return {};
  const discovered = findChromium();
  return discovered ? { executablePath: discovered } : {};
}

/** Max DOM depth the structural walk descends — deep enough for real UIs, bounded so a
 *  pathological tree can't produce an unbounded probe. */
const MAX_DEPTH = 14;

/** PNG width/height straight from the IHDR header (bytes 16..24) — no full decode. */
function pngSize(bytes: Uint8Array): { width: number; height: number } {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: dv.getUint32(16), height: dv.getUint32(20) };
}

export class DomSurface implements Surface<DomViewSpec, DomAction> {
  private constructor(
    private readonly browser: Browser,
    private readonly page: Page,
  ) {}

  /** Launch, navigate, and settle. The page is ready to render/probe/act on return. */
  static async open(opts: DomSurfaceOptions): Promise<DomSurface> {
    // PERCEPTION_CHROMIUM_ARGS: extra Chromium flags, space-separated — the seam
    // that lets the node's own self-exercise SPEAK into a live call with no human:
    //   --use-fake-device-for-media-stream --use-file-for-fake-audio-capture=<wav>
    //   --autoplay-policy=no-user-gesture-required
    // (a real microphone is the one sense a headless eye cannot otherwise drive;
    // Joel 2026-09-05: "you have to figure out how to test on your own").
    const extraArgs = (process.env.PERCEPTION_CHROMIUM_ARGS ?? '')
      .split(/\s+/)
      .filter((a) => a.length > 0);
    const browser = await chromium.launch({
      headless: opts.headless ?? true,
      ...resolveLaunch(opts),
      ...(extraArgs.length > 0 ? { args: extraArgs } : {}),
    });
    // PERCEPTION_GRANT_MEDIA=1 pre-grants microphone + camera to the page so the
    // fake device publishes without a permission prompt nobody can click.
    const grantMedia = process.env.PERCEPTION_GRANT_MEDIA === '1';
    const page = await browser.newPage({
      viewport: opts.viewport ?? { width: 1440, height: 900 },
      ...(grantMedia ? { permissions: ['microphone', 'camera'] } : {}),
      // 1 by default (2026-08-23 pixel audit): the hardcoded 2 quadrupled every
      // screenshot's pixels for consumers that render thumbnails — retina
      // capture is an OPT-IN for tasks that grade fine detail, never a tax on
      // every observation.
      deviceScaleFactor: opts.deviceScaleFactor ?? 1,
    });
    // esbuild's `keepNames` (tsx, vite, ts-node all use it) wraps named functions with a
    // `__name(fn, 'name')` helper. When Playwright serializes our `domWalk` into the page,
    // that helper isn't defined in the browser → ReferenceError. Shim a no-op so the
    // serialized walk runs under ANY bundler, not just the one whose transform omits it.
    // (The arrow args here are anonymous, so they are never `__name`-wrapped themselves.)
    await page.addInitScript(() => {
      const g = globalThis as { __name?: (fn: unknown) => unknown };
      g.__name = g.__name ?? ((fn) => fn);
    });
    // GLASS-BOX the page's own voice: console lines (warnings, [Violation]s,
    // errors) and page crashes stream to OUR stderr, so an operator/agent
    // reading the eye-node log sees exactly what a human sees in devtools
    // (2026-08-31: "the site draws at 5fps — are you able to get these
    // warnings?" — now yes).
    page.on('console', (msg) => {
      const type = msg.type();
      if (type === 'warning' || type === 'error') {
        console.error(`[page-console:${type}] ${msg.text().slice(0, 400)}`);
      }
    });
    page.on('pageerror', (err) => {
      console.error(`[page-error] ${String(err).slice(0, 400)}`);
    });
    await page.goto(opts.url, { waitUntil: 'networkidle' });
    // networkidle is not "at rest": an app that routes on a websocket envelope
    // (a deep link resolving once the nav state arrives) is still repainting
    // after the network goes quiet — caught live 2026-09-03 as a frame of the
    // landing room with the deep-linked page already in the structure. Wait
    // for the DOM to stop mutating (a quiet window, bounded) before observing.
    await page.evaluate(
      ({ quietMs, capMs }) =>
        new Promise<void>((resolve) => {
          let timer = window.setTimeout(resolve, quietMs);
          const cap = window.setTimeout(() => {
            observer.disconnect();
            resolve();
          }, capMs);
          const observer = new MutationObserver(() => {
            window.clearTimeout(timer);
            timer = window.setTimeout(() => {
              observer.disconnect();
              window.clearTimeout(cap);
              resolve();
            }, quietMs);
          });
          observer.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
        }),
      { quietMs: 600, capMs: 5000 },
    );
    return new DomSurface(browser, page);
  }

  async render(view: DomViewSpec = {}): Promise<Percept> {
    if (view.viewport) await this.page.setViewportSize({ width: view.viewport.width, height: view.viewport.height });
    if (view.theme) await this.page.emulateMedia({ colorScheme: view.theme });
    const bytes = view.selector
      ? await this.page.locator(view.selector).first().screenshot({ type: 'png' })
      : await this.page.screenshot({ type: 'png', fullPage: view.fullPage ?? false });
    const { width, height } = pngSize(bytes);
    return { kind: 'image', mime: 'image/png', bytes, width, height };
  }

  async probe(): Promise<StructuredState> {
    const url = this.page.url();
    const title = await this.page.title();
    // Accessibility tree — the SEMANTIC view (roles + accessible names), and it pierces
    // shadow DOM for free (our widgets render in a shadow root). Sourced from CDP's
    // Accessibility domain (protocol-stable) rather than the deprecated `page.accessibility`.
    const a11y = await this.axTree();
    // Layout/DOM tree — the geometry view a persona reasons about position/overflow with.
    const tree = (await this.page.evaluate(domWalk, MAX_DEPTH)) as ProbeNode;
    return { url, title, tree, a11y };
  }

  /** The accessibility tree via CDP, reconstructed from the flat AX node list. */
  private async axTree(): Promise<ProbeNode | undefined> {
    const client = await this.page.context().newCDPSession(this.page);
    try {
      await client.send('Accessibility.enable');
      const res = (await client.send('Accessibility.getFullAXTree')) as { nodes?: AxNode[] };
      const nodes = res.nodes ?? [];
      if (nodes.length === 0) return undefined;
      const byId = new Map(nodes.map((n) => [n.nodeId, n]));
      const build = (n: AxNode): ProbeNode => ({
        tag: n.role?.value ?? 'node',
        ...(n.role?.value ? { role: n.role.value } : {}),
        ...(n.name?.value ? { name: n.name.value } : {}),
        children: (n.childIds ?? [])
          .map((id) => byId.get(id))
          .filter((c): c is AxNode => c !== undefined)
          .map(build),
      });
      const root = nodes[0];
      return root ? build(root) : undefined;
    } finally {
      await client.detach().catch(() => undefined);
    }
  }

  async act(action: DomAction): Promise<void> {
    switch (action.kind) {
      case 'click':
        await this.page.locator(action.selector).first().click();
        return;
      case 'type':
        await this.page.locator(action.selector).first().fill(action.text);
        return;
      case 'press':
        await this.page.keyboard.press(action.key);
        return;
      case 'hover':
        await this.page.locator(action.selector).first().hover();
        return;
      case 'injectCss':
        // Hot-swap: retheme/relayout the LIVE page, no redeploy — the fast-iteration seam.
        await this.page.addStyleTag({ content: action.css });
        return;
      case 'hotPatchCss':
        // The ONE hot-patch layer: find-or-create `<style data-continuum-hot-edit>` and
        // REPLACE its content (append would stack patches and make re-applying a full
        // stylesheet non-idempotent). Empty css leaves an empty layer = patch cleared.
        await this.page.evaluate((css) => {
          const attr = 'data-continuum-hot-edit';
          let el = document.querySelector<HTMLStyleElement>(`style[${attr}]`);
          if (!el) {
            el = document.createElement('style');
            el.setAttribute(attr, '');
            document.head.appendChild(el);
          }
          el.textContent = css;
        }, action.css);
        return;
      case 'setViewport':
        await this.page.setViewportSize({ width: action.width, height: action.height });
        return;
      default: {
        const exhaustive: never = action;
        throw new ActError(`unsupported action: ${JSON.stringify(exhaustive)}`);
      }
    }
  }

  // JUDGE is universal — one shared pixel diff for every surface (see imageDiff.ts).
  diff(before: Percept, after: Percept): Delta {
    return imageDiff(before, after);
  }

  async close(): Promise<void> {
    await this.browser.close();
  }
}

/** One node of CDP's `Accessibility.getFullAXTree` flat list (the fields we read). */
interface AxNode {
  readonly nodeId: string;
  readonly role?: { readonly value?: string };
  readonly name?: { readonly value?: string };
  readonly childIds?: readonly string[];
}

/**
 * Runs IN THE BROWSER (serialized by Playwright) — a bounded DOM walk into a plain tree.
 * Descends shadow roots (our widgets render there) so the probe reaches the real content,
 * and skips display:none / hidden subtrees (invisible = not perceived).
 */
function domWalk(maxDepth: number): unknown {
  const ATTRS = ['id', 'class', 'href', 'role', 'aria-label', 'data-kind', 'data-status'];
  // The craft-fact subset a design grade measures (contrast, type scale, rhythm,
  // reflow). DECLARED, not the whole computed style — the observation stays a
  // bounded fact sheet, never a style dump.
  const STYLE_PROPS = [
    'color', 'background-color', 'font-size', 'font-weight', 'font-family',
    'margin', 'padding', 'display', 'overflow', 'z-index',
  ];
  function walk(el: Element, depth: number): unknown {
    const rect = el.getBoundingClientRect();
    const ownStyle = getComputedStyle(el);
    const style: Record<string, string> = {};
    for (const prop of STYLE_PROPS) {
      const v = ownStyle.getPropertyValue(prop);
      if (v) style[prop] = v;
    }
    const attrs: Record<string, string> = {};
    for (const a of ATTRS) {
      const v = el.getAttribute(a);
      if (v != null) attrs[a] = v;
    }
    let text = '';
    el.childNodes.forEach((n) => {
      if (n.nodeType === 3) text += n.textContent ?? '';
    });
    text = text.trim();
    const shadow = (el as HTMLElement).shadowRoot;
    const roots: Element[] = shadow ? Array.from(shadow.children) : Array.from(el.children);
    const children: unknown[] = [];
    if (depth < maxDepth) {
      for (const c of roots) {
        const cs = getComputedStyle(c);
        if (cs.display === 'none' || cs.visibility === 'hidden') continue;
        children.push(walk(c, depth + 1));
      }
    }
    const role = el.getAttribute('role');
    return {
      tag: el.tagName.toLowerCase(),
      ...(role ? { role } : {}),
      ...(text ? { text } : {}),
      box: {
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      },
      ...(Object.keys(attrs).length ? { attrs } : {}),
      ...(Object.keys(style).length ? { style } : {}),
      children,
    };
  }
  return walk(document.body, 0);
}
