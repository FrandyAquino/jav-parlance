/** Converts [-1, 1] float samples to PCM16 little-endian bytes, clamping overs. */
export function encodePcm16Le(samples: Float32Array): Uint8Array {
  const bytes = new Uint8Array(samples.length * 2);
  const view = new DataView(bytes.buffer);
  for (let i = 0; i < samples.length; i++) {
    view.setInt16(i * 2, toInt16(samples[i] ?? 0), true);
  }
  return bytes;
}

function toInt16(sample: number): number {
  const clamped = Math.max(-1, Math.min(1, Number.isNaN(sample) ? 0 : sample));
  return Math.round(clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff);
}
