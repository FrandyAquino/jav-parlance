/**
 * Streaming windowed-sinc resampler (Blackman window, band-limited
 * interpolation). Plain linear interpolation would fold everything above
 * 8 kHz of a 48 kHz tab back into the speech band when decimating to 16 kHz,
 * so the kernel doubles as the anti-aliasing low-pass filter.
 *
 * - Cutoff sits at 85 % of the lower Nyquist, leaving the transition band
 *   below the output Nyquist frequency.
 * - 16 zero crossings per side keep the transition around 2.7 kHz wide at
 *   16 kHz output, at ~110 multiply-adds per output sample from 48 kHz.
 * - The kernel is tabulated and linearly interpolated, so no trigonometry
 *   runs on the audio thread.
 * - Latency is half the kernel width (~1.2 ms at 48 kHz input).
 */
const ZERO_CROSSINGS = 16;
const CUTOFF_MARGIN = 0.85;
const TABLE_STEPS_PER_SAMPLE = 64;

export class SincResampler {
  private readonly step: number;
  private readonly halfWidth: number;
  private readonly reach: number;
  private readonly kernel: Float32Array;
  private pending: Float32Array;
  private position: number;

  constructor(inputRate: number, outputRate: number) {
    if (!(inputRate > 0 && outputRate > 0)) {
      throw new RangeError(`invalid resampling rates ${inputRate} -> ${outputRate}`);
    }
    const cutoff = Math.min(1, outputRate / inputRate) * CUTOFF_MARGIN;
    this.step = inputRate / outputRate;
    this.halfWidth = ZERO_CROSSINGS / cutoff;
    this.reach = Math.ceil(this.halfWidth);
    this.kernel = tabulateKernel(cutoff, this.halfWidth);
    // Leading silence centres the first output on the first input sample.
    this.pending = new Float32Array(this.reach);
    this.position = this.reach;
  }

  process(input: Float32Array): Float32Array {
    const buffer = concat(this.pending, input);
    const output = new Float32Array(Math.ceil(buffer.length / this.step) + 1);
    let written = 0;
    while (this.position + this.halfWidth < buffer.length) {
      output[written++] = this.sampleAt(buffer, this.position);
      this.position += this.step;
    }
    const keepFrom = Math.max(0, Math.floor(this.position) - this.reach);
    this.pending = buffer.slice(keepFrom);
    this.position -= keepFrom;
    return output.slice(0, written);
  }

  private sampleAt(buffer: Float32Array, position: number): number {
    const first = Math.ceil(position - this.halfWidth);
    const last = Math.floor(position + this.halfWidth);
    let sum = 0;
    let weights = 0;
    for (let k = first; k <= last; k++) {
      const weight = this.weightAt(Math.abs(position - k));
      sum += (buffer[k] ?? 0) * weight;
      weights += weight;
    }
    // Normalising by the tap sum keeps unity DC gain at every fractional phase.
    return weights === 0 ? 0 : sum / weights;
  }

  private weightAt(distance: number): number {
    const index = distance * TABLE_STEPS_PER_SAMPLE;
    const lower = Math.floor(index);
    const a = this.kernel[lower] ?? 0;
    const b = this.kernel[lower + 1] ?? 0;
    return a + (b - a) * (index - lower);
  }
}

function tabulateKernel(cutoff: number, halfWidth: number): Float32Array {
  const table = new Float32Array(Math.ceil(halfWidth * TABLE_STEPS_PER_SAMPLE) + 2);
  for (let i = 0; i < table.length; i++) {
    const distance = i / TABLE_STEPS_PER_SAMPLE;
    table[i] = distance > halfWidth ? 0 : cutoff * sinc(cutoff * distance) * blackman(distance / halfWidth);
  }
  return table;
}

function sinc(x: number): number {
  if (x === 0) return 1;
  const t = Math.PI * x;
  return Math.sin(t) / t;
}

/** Symmetric Blackman window over `u` in [0, 1] (centre to edge). */
function blackman(u: number): number {
  return 0.42 + 0.5 * Math.cos(Math.PI * u) + 0.08 * Math.cos(2 * Math.PI * u);
}

function concat(a: Float32Array, b: Float32Array): Float32Array {
  const joined = new Float32Array(a.length + b.length);
  joined.set(a);
  joined.set(b, a.length);
  return joined;
}
