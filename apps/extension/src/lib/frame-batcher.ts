/** Regroups arbitrarily sized sample blocks into fixed-size frames. */
export class FrameBatcher {
  private readonly frameSize: number;
  private current: Float32Array;
  private filled = 0;

  constructor(frameSize: number) {
    if (!Number.isInteger(frameSize) || frameSize <= 0) {
      throw new RangeError(`frame size must be a positive integer, got ${frameSize}`);
    }
    this.frameSize = frameSize;
    this.current = new Float32Array(frameSize);
  }

  /** Returns every frame completed by `samples`, in order. */
  push(samples: Float32Array): Float32Array[] {
    const completed: Float32Array[] = [];
    let offset = 0;
    while (offset < samples.length) {
      const take = Math.min(this.frameSize - this.filled, samples.length - offset);
      this.current.set(samples.subarray(offset, offset + take), this.filled);
      this.filled += take;
      offset += take;
      if (this.filled === this.frameSize) {
        completed.push(this.current);
        this.current = new Float32Array(this.frameSize);
        this.filled = 0;
      }
    }
    return completed;
  }
}
