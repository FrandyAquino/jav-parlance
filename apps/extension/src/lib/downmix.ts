/** Averages planar channels into one; a mono input is returned as-is. */
export function downmixToMono(channels: readonly Float32Array[]): Float32Array {
  const [first] = channels;
  if (first === undefined) return new Float32Array(0);
  if (channels.length === 1) return first;

  const scale = 1 / channels.length;
  const mono = new Float32Array(first.length);
  for (let i = 0; i < mono.length; i++) {
    let sum = 0;
    for (const channel of channels) sum += channel[i] ?? 0;
    mono[i] = sum * scale;
  }
  return mono;
}
