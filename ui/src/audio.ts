// Motor de audio procedural de v3.0 (Web Audio API), con los cinco presets.
// El AudioContext se reanuda tras el primer gesto del usuario (política de autoplay).

export type SoundPreset = "ping" | "bkb" | "blink" | "buyback" | "alert";

export class AudioEngine {
  private ctx: AudioContext | null = null;
  muted = false;

  /** Llamar desde un gesto del usuario (click/tecla). */
  unlock(): void {
    if (!this.ctx) this.ctx = new AudioContext();
    if (this.ctx.state === "suspended") void this.ctx.resume();
  }

  play(preset: SoundPreset): void {
    if (this.muted || !this.ctx || this.ctx.state !== "running") return;
    try {
      ({ ping: () => this.ping(), bkb: () => this.bkb(), blink: () => this.blink(), buyback: () => this.buyback(), alert: () => this.siren() })[preset]();
    } catch {
      /* el audio nunca debe romper la app */
    }
  }

  private tone(type: OscillatorType, at: number, dur: number, gain: number, freq: (o: OscillatorNode, t: number) => void): void {
    const ctx = this.ctx!;
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = type;
    freq(osc, at);
    g.gain.setValueAtTime(gain, at);
    g.gain.exponentialRampToValueAtTime(0.001, at + dur);
    osc.connect(g);
    g.connect(ctx.destination);
    osc.start(at);
    osc.stop(at + dur);
  }

  private ping(): void {
    const now = this.ctx!.currentTime;
    for (const d of [0, 0.1]) {
      this.tone("sine", now + d, 0.09, 0.28, (o, t) => {
        o.frequency.setValueAtTime(1320, t);
        o.frequency.exponentialRampToValueAtTime(1180, t + 0.08);
      });
    }
  }

  private bkb(): void {
    const now = this.ctx!.currentTime;
    this.tone("sine", now, 0.55, 0.45, (o, t) => {
      o.frequency.setValueAtTime(55, t);
      o.frequency.exponentialRampToValueAtTime(170, t + 0.4);
    });
    this.tone("sawtooth", now, 0.65, 0.22, (o, t) => {
      o.frequency.setValueAtTime(140, t);
      o.frequency.exponentialRampToValueAtTime(680, t + 0.28);
    });
  }

  private blink(): void {
    const now = this.ctx!.currentTime;
    this.tone("sine", now, 0.16, 0.32, (o, t) => {
      o.frequency.setValueAtTime(2400, t);
      o.frequency.exponentialRampToValueAtTime(420, t + 0.15);
    });
  }

  private buyback(): void {
    const now = this.ctx!.currentTime;
    [587.33, 739.99, 880.0, 1174.66].forEach((f, i) => {
      this.tone("triangle", now + i * 0.04, 0.65, 0.14, (o, t) => o.frequency.setValueAtTime(f, t));
    });
  }

  private siren(): void {
    const now = this.ctx!.currentTime;
    this.tone("square", now, 0.35, 0.18, (o, t) => {
      o.frequency.setValueAtTime(440, t);
      o.frequency.setValueAtTime(880, t + 0.15);
    });
  }
}
