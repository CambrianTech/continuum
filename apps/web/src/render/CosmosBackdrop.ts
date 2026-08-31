/**
 * <cosmos-backdrop> — a LIVING generative starfield for the 'cosmos' universe.
 *
 * Not a CSS skin: an animated canvas where the citizens converse among drifting,
 * twinkling stars linked by a faint constellation network that forms + dissolves as they
 * move — the grid as a breathing mind ([[universe-is-an-experience-not-a-theme]]). Owns
 * its own requestAnimationFrame lifecycle and sits behind the translucent chat panels.
 * A universe can be a whole world in MOTION, not just a colour.
 */

import { LitElement, html, css, type TemplateResult } from 'lit';

interface Star {
  x: number;
  y: number;
  r: number; // radius (px)
  vx: number;
  vy: number;
  phase: number; // twinkle offset
}

export class CosmosBackdrop extends LitElement {
  static override styles = css`
    :host {
      position: absolute;
      inset: 0;
      z-index: 0;
      overflow: hidden;
      pointer-events: none;
    }
    canvas {
      width: 100%;
      height: 100%;
      display: block;
    }
  `;

  private _canvas?: HTMLCanvasElement;
  private _ctx?: CanvasRenderingContext2D | null;
  private _stars: Star[] = [];
  /** Live WORK energy 0..1 (the widget feeds working-run count): the field
   *  breathes brighter/faster when the grid is thinking hard. */
  energy = 0;
  private _comets: { x: number; y: number; vx: number; vy: number; life: number }[] = [];

  /** A RESOLVED VERDICT fires a comet — spectacle tied to truth, never
   *  decoration on a dead system. */
  surge(): void {
    const w = this._canvas?.width ?? 800;
    this._comets.push({
      x: Math.random() * w * 0.3,
      y: Math.random() * 120,
      vx: 6 + Math.random() * 5,
      vy: 2.2 + Math.random() * 1.6,
      life: 1,
    });
  }
  private _raf = 0;
  private _t0 = 0;

  /** The room's citizens — rendered as a living constellation (the room, reflected in
   *  the sky). Set by the host each render; the animation reads it live, so who's
   *  present + who's active literally shapes the cosmos. */
  citizens: { name: string; active: boolean }[] = [];

  override render(): TemplateResult {
    return html`<canvas></canvas>`;
  }

  override firstUpdated(): void {
    this._canvas = this.renderRoot.querySelector('canvas') ?? undefined;
    this._ctx = this._canvas?.getContext('2d');
    this._resize();
    window.addEventListener('resize', this._resize);
    this._t0 = performance.now();
    this._raf = requestAnimationFrame(this._loop);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    cancelAnimationFrame(this._raf);
    window.removeEventListener('resize', this._resize);
  }

  private _resize = (): void => {
    if (!this._canvas) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = this.offsetWidth || 1200;
    const h = this.offsetHeight || 800;
    this._canvas.width = w * dpr;
    this._canvas.height = h * dpr;
    this._ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
    const count = Math.max(30, Math.min(110, Math.round((w * h) / 15000)));
    this._stars = Array.from({ length: count }, () => ({
      x: Math.random() * w,
      y: Math.random() * h,
      r: 0.5 + Math.random() * 1.6,
      vx: (Math.random() - 0.5) * 0.09,
      vy: (Math.random() - 0.5) * 0.09,
      phase: Math.random() * Math.PI * 2,
    }));
  };

  private _loop = (t: number): void => {
    const ctx = this._ctx;
    if (!ctx) return;
    const w = this.offsetWidth;
    const h = this.offsetHeight;
    const time = (t - this._t0) / 1000;

    // deep space
    ctx.fillStyle = '#05010f';
    ctx.fillRect(0, 0, w, h);

    // nebula — soft additive blooms that slowly breathe
    ctx.globalCompositeOperation = 'lighter';
    const blobs: [number, number, string, number][] = [
      [w * 0.22, h * 0.24, 'rgba(90,40,180,', 340],
      [w * 0.80, h * 0.34, 'rgba(30,90,205,', 380],
      [w * 0.55, h * 0.82, 'rgba(185,40,140,', 320],
    ];
    for (const [bx, by, col, r] of blobs) {
      const pulse = 0.1 + 0.05 * Math.sin(time * 0.3 + bx * 0.01);
      const g = ctx.createRadialGradient(bx, by, 0, bx, by, r);
      g.addColorStop(0, `${col}${pulse})`);
      g.addColorStop(1, `${col}0)`);
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);
    }

    // drift + wrap
    for (const s of this._stars) {
      s.x += s.vx;
      s.y += s.vy;
      if (s.x < 0) s.x += w;
      else if (s.x > w) s.x -= w;
      if (s.y < 0) s.y += h;
      else if (s.y > h) s.y -= h;
    }

    // constellation network — lines between near stars, forming + dissolving
    const LINK = 130;
    ctx.lineWidth = 1;
    for (let i = 0; i < this._stars.length; i++) {
      for (let j = i + 1; j < this._stars.length; j++) {
        const a = this._stars[i];
        const b = this._stars[j];
        if (!a || !b) continue;
        const dx = a.x - b.x;
        const dy = a.y - b.y;
        const d2 = dx * dx + dy * dy;
        if (d2 < LINK * LINK) {
          const alpha = (1 - Math.sqrt(d2) / LINK) * 0.16;
          ctx.strokeStyle = `rgba(120,180,255,${alpha})`;
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }
    }

    // stars — twinkle with a soft glow
    for (const s of this._stars) {
      const tw = 0.5 + 0.5 * Math.sin(time * 1.6 + s.phase);
      ctx.fillStyle = `rgba(215,228,255,${0.45 * tw + 0.35})`;
      ctx.shadowColor = 'rgba(150,190,255,0.9)';
      ctx.shadowBlur = 4 + 3 * tw;
      ctx.beginPath();
      ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.shadowBlur = 0;
    ctx.globalCompositeOperation = 'source-over';

    // The CITIZENS as a living constellation — the room reflected in the sky. Each
    // persona is a named star slowly orbiting; the active ones pulse; lines link them
    // all. Who is here + who is thinking literally shapes the cosmos.
    const cz = this.citizens;
    if (cz.length > 0) {
      const cx = w / 2;
      const cy = h * 0.46;
      const rr = Math.min(w, h) * 0.32;
      const pos = cz.map((c, i) => {
        const ang = (i / cz.length) * Math.PI * 2 + time * 0.05;
        return { c, x: cx + Math.cos(ang) * rr * 1.15, y: cy + Math.sin(ang) * rr };
      });
      ctx.strokeStyle = 'rgba(150,190,255,0.32)';
      ctx.lineWidth = 1;
      for (let i = 0; i < pos.length; i++) {
        for (let j = i + 1; j < pos.length; j++) {
          const a = pos[i];
          const b = pos[j];
          if (!a || !b) continue;
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }
      ctx.textAlign = 'center';
      ctx.font = '600 12px system-ui, -apple-system, sans-serif';
      for (const p of pos) {
        const pulse = p.c.active
          ? 0.6 + (0.4 + 0.3 * this.energy) * Math.sin(time * (2.2 + 2 * this.energy))
          : 0.4;
        ctx.shadowColor = 'rgba(150,190,255,0.95)';
        ctx.shadowBlur = 14 * pulse + 6;
        ctx.fillStyle = `rgba(222,236,255,${0.7 + 0.3 * pulse})`;
        // (comets painted after the loop — see below)
        ctx.beginPath();
        ctx.arc(p.x, p.y, 3.6, 0, Math.PI * 2);
        ctx.fill();
        ctx.shadowBlur = 0;
        ctx.fillStyle = 'rgba(200,215,255,0.9)';
        ctx.fillText(p.c.name, p.x, p.y + 20);
      }
      ctx.textAlign = 'start';
    }

    // COMETS — one per resolved verdict (surge()); a bright head with a
    // fading tail, gone in ~1.5s. Truth-driven spectacle.
    if (this._comets.length > 0) {
      for (const c of this._comets) {
        c.x += c.vx;
        c.y += c.vy;
        c.life -= 0.012;
        const grad = ctx.createLinearGradient(c.x - c.vx * 12, c.y - c.vy * 12, c.x, c.y);
        grad.addColorStop(0, 'rgba(120,220,255,0)');
        grad.addColorStop(1, `rgba(190,240,255,${0.85 * c.life})`);
        ctx.strokeStyle = grad;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(c.x - c.vx * 12, c.y - c.vy * 12);
        ctx.lineTo(c.x, c.y);
        ctx.stroke();
        ctx.fillStyle = `rgba(230,250,255,${c.life})`;
        ctx.beginPath();
        ctx.arc(c.x, c.y, 2.4, 0, Math.PI * 2);
        ctx.fill();
      }
      this._comets = this._comets.filter((c) => c.life > 0);
    }

    this._raf = requestAnimationFrame(this._loop);
  };
}

if (!customElements.get('cosmos-backdrop')) {
  customElements.define('cosmos-backdrop', CosmosBackdrop);
}
