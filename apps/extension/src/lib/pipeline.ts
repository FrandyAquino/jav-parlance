import { downmixToMono } from "./downmix";
import { FrameBatcher } from "./frame-batcher";
import { encodePcm16Le } from "./pcm16";
import { SincResampler } from "./resample";

/** Tab audio (planar, any rate) to fixed-size mono PCM16 LE frames at `outputRate`. */
export class CapturePipeline {
  private readonly resampler: SincResampler;
  private readonly batcher: FrameBatcher;

  constructor(inputRate: number, outputRate: number, frameSamples: number) {
    this.resampler = new SincResampler(inputRate, outputRate);
    this.batcher = new FrameBatcher(frameSamples);
  }

  push(channels: readonly Float32Array[]): Uint8Array[] {
    const resampled = this.resampler.process(downmixToMono(channels));
    return this.batcher.push(resampled).map(encodePcm16Le);
  }
}
