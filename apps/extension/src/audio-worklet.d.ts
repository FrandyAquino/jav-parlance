// AudioWorkletGlobalScope members that TypeScript's DOM lib does not declare.

declare abstract class AudioWorkletProcessor {
  readonly port: MessagePort;
  constructor(options?: AudioWorkletNodeOptions);
  abstract process(inputs: Float32Array[][], outputs: Float32Array[][]): boolean;
}

declare function registerProcessor(
  name: string,
  processorCtor: new (options: AudioWorkletNodeOptions) => AudioWorkletProcessor,
): void;
