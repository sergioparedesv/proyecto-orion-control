// Fondo animado Radiant/Dire de v3.0, con las correcciones del §8.3:
// - un solo bucle requestAnimationFrame, con cancelación;
// - con wallpaper propio el canvas se detiene, se limpia y se oculta;
// - en modo partida o con la ventana oculta, el bucle se detiene.
// (Math.random aquí es solo decorativo: nunca se usa para métricas.)

type Particle = { x: number; y: number; size: number; vx: number; vy: number; alpha: number; color: string };

export class Background {
  private ctx: CanvasRenderingContext2D;
  private raf = 0;
  private particles: Particle[] = [];
  private wave = 0;
  private mode: "canvas" | "image" = "canvas";
  private paused = false;
  private imageUrl: string | null = null;

  constructor(
    private canvas: HTMLCanvasElement,
    private imageLayer: HTMLElement,
    private overlay: HTMLElement,
  ) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 2D no disponible");
    this.ctx = ctx;
    window.addEventListener("resize", () => this.resize());
    document.addEventListener("visibilitychange", () => this.sync());
    this.resize();
    this.particles = Array.from({ length: 90 }, () => this.spawn(true));
    this.sync();
  }

  get currentMode() {
    return this.mode;
  }

  setPaused(paused: boolean): void {
    this.paused = paused;
    this.sync();
  }

  setImage(url: string): void {
    if (this.imageUrl) URL.revokeObjectURL(this.imageUrl);
    this.imageUrl = url;
    this.imageLayer.style.backgroundImage = `url("${url}")`;
    this.mode = "image";
    this.sync();
  }

  /** Alterna canvas / imagen. Devuelve false si no hay imagen cargada. */
  toggle(): boolean {
    if (!this.imageUrl) return false;
    this.mode = this.mode === "canvas" ? "image" : "canvas";
    this.sync();
    return true;
  }

  private sync(): void {
    const image = this.mode === "image" && this.imageUrl !== null;
    this.imageLayer.classList.toggle("hidden", !image);
    this.overlay.classList.toggle("hidden", !image);
    this.canvas.classList.toggle("hidden", image);
    const shouldRun = !image && !this.paused && document.visibilityState === "visible";
    if (shouldRun && !this.raf) {
      this.raf = requestAnimationFrame(this.frame);
    } else if (!shouldRun && this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
    if (image) this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    else if (!shouldRun) this.drawStatic();
  }

  private resize(): void {
    this.canvas.width = window.innerWidth;
    this.canvas.height = window.innerHeight;
    if (!this.raf && this.mode === "canvas") this.drawStatic();
  }

  private spawn(anywhere: boolean): Particle {
    const w = this.canvas.width;
    const h = this.canvas.height;
    const x = Math.random() * w;
    const mid = w / 2;
    const palette = x < mid - 40 ? ["#00ffa3", "#ecc94b", "#00c3ff"] : x > mid + 40 ? ["#ff2a4b", "#ff7700", "#d69e2e"] : ["#00c3ff"];
    return {
      x,
      y: anywhere ? Math.random() * h : h,
      size: Math.random() * 3 + 1,
      vx: (Math.random() - 0.5) * 0.6,
      vy: -(Math.random() * 0.9 + 0.3),
      alpha: Math.random() * 0.7 + 0.25,
      color: palette[Math.floor(Math.random() * palette.length)],
    };
  }

  private frame = (): void => {
    this.raf = 0;
    this.wave += 0.03;
    this.drawTerrain();
    const { ctx } = this;
    for (let i = 0; i < this.particles.length; i++) {
      const p = this.particles[i];
      p.x += p.vx;
      p.y += p.vy;
      if (p.y < 0 || p.x < 0 || p.x > this.canvas.width) this.particles[i] = this.spawn(false);
      ctx.globalAlpha = p.alpha;
      ctx.fillStyle = p.color;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    this.raf = requestAnimationFrame(this.frame);
  };

  private drawStatic(): void {
    this.drawTerrain();
  }

  private drawTerrain(): void {
    const { ctx } = this;
    const w = this.canvas.width;
    const h = this.canvas.height;
    ctx.clearRect(0, 0, w, h);

    const radiant = ctx.createLinearGradient(0, 0, w * 0.5, h);
    radiant.addColorStop(0, "rgba(0, 45, 28, 0.45)");
    radiant.addColorStop(1, "rgba(2, 4, 8, 0.96)");
    ctx.fillStyle = radiant;
    ctx.fillRect(0, 0, w * 0.5, h);

    const dire = ctx.createLinearGradient(w * 0.5, 0, w, h);
    dire.addColorStop(0, "rgba(55, 6, 16, 0.45)");
    dire.addColorStop(1, "rgba(2, 4, 8, 0.96)");
    ctx.fillStyle = dire;
    ctx.fillRect(w * 0.5, 0, w * 0.5, h);

    const river = ctx.createLinearGradient(w * 0.42, 0, w * 0.58, h);
    river.addColorStop(0, "rgba(0, 195, 255, 0.14)");
    river.addColorStop(0.5, "rgba(0, 255, 163, 0.18)");
    river.addColorStop(1, "rgba(0, 195, 255, 0.14)");
    ctx.fillStyle = river;
    ctx.beginPath();
    ctx.moveTo(w * 0.38, 0);
    ctx.lineTo(w * 0.46, 0);
    ctx.lineTo(w * 0.62, h);
    ctx.lineTo(w * 0.54, h);
    ctx.closePath();
    ctx.fill();

    ctx.strokeStyle = "rgba(0, 195, 255, 0.25)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let y = 0; y < h; y += 10) {
      const x = w * 0.42 + y * 0.15 + Math.sin(y * 0.02 + this.wave) * 12;
      if (y === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  }
}
