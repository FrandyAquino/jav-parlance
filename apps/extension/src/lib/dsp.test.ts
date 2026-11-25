import { describe, expect, it } from "vitest";
import { bytesToBase64 } from "./base64";
import { downmixToMono } from "./downmix";
import { FrameBatcher } from "./frame-batcher";
import { encodePcm16Le } from "./pcm16";
import { CapturePipeline } from "./pipeline";
import { SincResampler } from "./resample";

function sine(frequency: number, rate: number, length: number): Float32Array {
  return Float32Array.from({ length }, (_, i) => Math.sin((2 * Math.PI * frequency * i) / rate));
}

function rms(samples: Float32Array): number {
  return Math.sqrt(samples.reduce((sum, s) => sum + s * s, 0) / samples.length);
}

/** Drops the edges where the kernel overlaps the leading silence. */
function steady(samples: Float32Array): Float32Array {
  return samples.subarray(100, samples.length - 100);
}

describe("downmixToMono", () => {
  it("averages channels sample by sample", () => {
    const mono = downmixToMono([Float32Array.of(1, 0.5, -1), Float32Array.of(0, 0.5, 1)]);
    expect(Array.from(mono)).toEqual([0.5, 0.5, 0]);
  });

  it("passes mono through without copying", () => {
    const channel = Float32Array.of(0.25);
    expect(downmixToMono([channel])).toBe(channel);
  });

  it("returns silence for no channels", () => {
    expect(downmixToMono([])).toHaveLength(0);
  });
});

describe("SincResampler", () => {
  it("produces the expected number of samples", () => {
    const output = new SincResampler(48_000, 16_000).process(new Float32Array(48_000));
    expect(output.length).toBeGreaterThan(15_950);
    expect(output.length).toBeLessThanOrEqual(16_000);
  });

  it("keeps unity gain at DC", () => {
    const output = steady(new SincResampler(44_100, 16_000).process(new Float32Array(44_100).fill(0.5)));
    for (const sample of output) expect(sample).toBeCloseTo(0.5, 4);
  });

  it("preserves a 1 kHz tone", () => {
    const output = steady(new SincResampler(48_000, 16_000).process(sine(1_000, 48_000, 48_000)));
    expect(rms(output)).toBeCloseTo(Math.SQRT1_2, 2);
  });

  it("suppresses content above the output Nyquist frequency", () => {
    const output = steady(new SincResampler(48_000, 16_000).process(sine(12_000, 48_000, 48_000)));
    expect(rms(output)).toBeLessThan(0.005);
  });

  it("gives the same result regardless of block size", () => {
    const input = sine(440, 48_000, 4_800);
    const whole = new SincResampler(48_000, 16_000).process(input);
    const chunked = new SincResampler(48_000, 16_000);
    const parts: number[] = [];
    for (let offset = 0; offset < input.length; offset += 128) {
      parts.push(...chunked.process(input.subarray(offset, offset + 128)));
    }
    expect(parts).toHaveLength(whole.length);
    parts.forEach((sample, i) => expect(sample).toBeCloseTo(whole[i] ?? NaN, 5));
  });

  it("rejects non-positive rates", () => {
    expect(() => new SincResampler(0, 16_000)).toThrow(RangeError);
  });
});

describe("encodePcm16Le", () => {
  it("scales, clamps and writes little-endian", () => {
    const bytes = encodePcm16Le(Float32Array.of(0, 1, -1, 2, -2, 0.5, NaN));
    const view = new DataView(bytes.buffer);
    const values = Array.from({ length: 7 }, (_, i) => view.getInt16(i * 2, true));
    expect(values).toEqual([0, 32767, -32768, 32767, -32768, 16384, 0]);
    expect(Array.from(bytes.subarray(2, 4))).toEqual([0xff, 0x7f]);
  });
});

describe("bytesToBase64", () => {
  it("matches the reference encoding", () => {
    expect(bytesToBase64(Uint8Array.of(0, 1, 2, 253, 254, 255))).toBe("AAEC/f7/");
    expect(bytesToBase64(new Uint8Array(0))).toBe("");
  });

  it("handles buffers larger than one chunk", () => {
    const bytes = Uint8Array.from({ length: 100_000 }, (_, i) => i % 256);
    const decoded = Uint8Array.from(atob(bytesToBase64(bytes)), (char) => char.charCodeAt(0));
    expect(decoded).toEqual(bytes);
  });
});

describe("FrameBatcher", () => {
  it("emits fixed-size frames across pushes and keeps the remainder", () => {
    const batcher = new FrameBatcher(4);
    expect(batcher.push(Float32Array.of(1, 2, 3))).toEqual([]);
    const frames = batcher.push(Float32Array.of(4, 5, 6, 7, 8, 9));
    expect(frames.map((frame) => Array.from(frame))).toEqual([
      [1, 2, 3, 4],
      [5, 6, 7, 8],
    ]);
    expect(batcher.push(Float32Array.of(10, 11, 12)).map((frame) => Array.from(frame))).toEqual([[9, 10, 11, 12]]);
  });

  it("does not reuse emitted frames", () => {
    const batcher = new FrameBatcher(2);
    const [first] = batcher.push(Float32Array.of(1, 2));
    batcher.push(Float32Array.of(3, 4));
    expect(Array.from(first ?? [])).toEqual([1, 2]);
  });

  it("rejects invalid frame sizes", () => {
    expect(() => new FrameBatcher(0)).toThrow(RangeError);
    expect(() => new FrameBatcher(1.5)).toThrow(RangeError);
  });
});

describe("CapturePipeline", () => {
  it("turns 48 kHz stereo render quanta into 100 ms PCM16 frames", () => {
    const pipeline = new CapturePipeline(48_000, 16_000, 1_600);
    const quantum = [new Float32Array(128).fill(0.25), new Float32Array(128).fill(0.25)];
    const frames: Uint8Array[] = [];
    // One second of audio.
    for (let i = 0; i < 375; i++) frames.push(...pipeline.push(quantum));

    expect(frames).toHaveLength(9);
    for (const frame of frames) expect(frame.byteLength).toBe(3_200);
    const last = frames.at(-1);
    expect(last && new DataView(last.buffer).getInt16(0, true)).toBe(Math.round(0.25 * 0x7fff));
  });
});
