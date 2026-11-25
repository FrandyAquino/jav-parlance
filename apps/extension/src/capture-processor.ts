import { BRIDGE_SAMPLE_RATE, FRAME_SAMPLES, PROCESSOR_NAME } from "./lib/constants";
import { CapturePipeline } from "./lib/pipeline";

export interface CaptureProcessorOptions {
  inputSampleRate: number;
}

/** Runs on the audio rendering thread; posts each 100 ms PCM16 frame as a transferred buffer. */
class CaptureProcessor extends AudioWorkletProcessor {
  private readonly pipeline: CapturePipeline;

  constructor(options: AudioWorkletNodeOptions) {
    super(options);
    const { inputSampleRate } = options.processorOptions as CaptureProcessorOptions;
    this.pipeline = new CapturePipeline(inputSampleRate, BRIDGE_SAMPLE_RATE, FRAME_SAMPLES);
  }

  process(inputs: Float32Array[][]): boolean {
    const channels = inputs[0];
    if (channels !== undefined && channels.length > 0) {
      for (const frame of this.pipeline.push(channels)) {
        this.port.postMessage(frame.buffer, [frame.buffer]);
      }
    }
    return true;
  }
}

registerProcessor(PROCESSOR_NAME, CaptureProcessor);
