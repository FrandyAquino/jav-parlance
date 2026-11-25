# Model Licenses and Sizing

Verified 2026-11-24 via Hugging Face model cards and the HF API (file sizes and LFS SHA-256 values). RAM/VRAM figures are approximate.

## Whisper (ASR) — MIT (OpenAI weights) / MIT (whisper.cpp)

- **Upstream:** whisper.cpp latest release **v1.9.6** (2026-11-23). `whisper-rs` 0.16.0 (2026-03-12, Unlicense, now hosted on Codeberg) bundles whisper.cpp **1.8.3** through `whisper-rs-sys` 0.15.0, so the Rust binding lags upstream.
- **Languages:** 99 (the `.en` variants are English-only). The card says strong ASR in about 10 languages and uneven quality elsewhere.
- **Hashes:** SHA-1 values come from `models/README.md` in whisper.cpp. SHA-256 values are the HF LFS object IDs for `ggerganov/whisper.cpp`.

| File | Bytes | Mem (whisper.cpp table) | SHA-1 (README) | SHA-256 (HF LFS) |
|---|---|---|---|---|
| ggml-tiny.bin | 77,691,713 | ~273 MB | bd577a113a864445d4c299885e0cb97d4ba92b5f | be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21 |
| ggml-base.bin | 147,951,465 | ~388 MB | 465707469ff3a37a2b9b8d8f89f2f99de7299dac | 60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe |
| ggml-base-q8_0.bin | 81,768,585 | — | not listed | c577b9a86e7e048a0b7eada054f4dd79a56bbfa911fbdacf900ac5b567cbb7d9 |
| ggml-small.bin | 487,601,967 | ~852 MB | 55356645c2b361a969dfd0ef2c5a50d530afd8d5 | 1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b |
| ggml-small-q8_0.bin | 264,464,607 | — | not listed | 49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f |
| ggml-small-q5_1.bin | 190,085,487 | — | not listed | ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb |
| ggml-medium-q5_0.bin | 539,212,467 | — | not listed | 19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f |
| ggml-large-v3.bin | 3,095,033,483 | ~3.9 GB | ad82bf6a9043ceed055076d0fd39f5f186ff8062 | 64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2 |
| ggml-large-v3-q5_0.bin | 1,081,140,203 | — | e6e2ed78495d403bef4b7cff42ef4aaadcfea8de | d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1 |
| ggml-large-v3-turbo.bin | 1,624,555,275 | — | 4af2b29d7ec73d781377bfd1758ca957a807e941 | 1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69 |
| ggml-large-v3-turbo-q5_0.bin | 574,041,195 | — | e050f7970618a659205450ad97eb95a18d69c9ee | 394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2 |
| ggml-large-v3-turbo-q8_0.bin | 874,188,075 | — | not listed | 317eb69c11673c9de1e1f0d459b253999804ec71ac4c23c17ecf5fbe24e259a1 |

- **Other formats:** the repo also has `q5_1` and `q8_0` variants for tiny, base and small, `medium-q8_0`, `large-v2-q5_0`/`q8_0`, and CoreML encoders (`*-encoder.mlmodelc.zip`).
- **large-v3-turbo:**
  - 809M parameters; the decoder was cut from 32 to 4 layers.
  - The card does not say whether it was trained for `translate`. Community reports say turbo translates poorly (unverified).
  - For speech→English, use the Parlance MT stage rather than Whisper `translate`.
- **Verifying downloads:** use the SHA-256 above. The HF API returns it at `/api/models/ggerganov/whisper.cpp/tree/main` as `lfs.oid`.

## Silero VAD — MIT

- **Upstream:** https://github.com/snakers4/silero-vad, MIT. One badge's alt text says CC BY-NC, but the badge and the LICENSE link point to MIT.
- **Specs:** 8 kHz or 16 kHz input, chunks of 30 ms or more, under 1 ms per chunk on one CPU thread, about 2 MB JIT model, ONNX available.
- **whisper.cpp has native Silero VAD** (`--vad`, `--vad-model`, `--vad-threshold`, `--vad-min-speech-duration-ms`, `--vad-min-silence-duration-ms`, `--vad-max-speech-duration-s`, `--vad-speech-pad-ms`, `--vad-samples-overlap`).
- **ggml models** are in HF `ggml-org/whisper-vad` (`models/download-vad-model.{sh,cmd}`):

| File | Bytes | SHA-256 (HF LFS) |
|---|---|---|
| ggml-silero-v5.1.2.bin | 885,098 | 29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf |
| ggml-silero-v6.2.0.bin | 885,098 | 2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987 |

- **whisper-rs 0.16** exposes the VAD API (`VadContext`, `VadContextParams`, `VadParams`, `VadSegments`), so no ONNX runtime is needed.
- **Format check:** v6.2.0 is the version the upstream README documents now. Confirm that whisper.cpp 1.8.3, the version bundled in whisper-rs-sys, loads v6.2.0 (unverified). Fall back to v5.1.2 if it does not.

## Translation models

| Model | License | Sizes | Formats | Languages | Verdict |
|---|---|---|---|---|---|
| **MADLAD-400** (`google/madlad400-{3b,7b,7b-bt,10b}-mt`) | **Apache-2.0** | 3B (Q4 GGUF ≈1.65–1.73 GB), 7B, 10B | Safetensors; Google ships a Candle-format `model-q4k.gguf` (3B, 7B) that is **not llama.cpp-compatible**; community llama.cpp GGUFs (e.g. `mtsdurica/madlad400-3b-mt-Q4_K_M-GGUF`); CTranslate2 int8 (`Nextcloud-AI/madlad400-3b-mt-ct2-int8`) | 400+ (419 listed) | Best permissive many-to-many option. 3B is the realistic size. |
| **OPUS-MT / MarianMT** (Helsinki-NLP) | Mixed: older `opus-mt-xx-yy` = **Apache-2.0**, `opus-mt-tc-*` (Tatoeba Challenge) = **CC-BY-4.0** (attribution required) | ~75M (base), ~200M (tc-big) per pair | PyTorch/TF/JAX; CTranslate2 converts Marian natively; no GGUF/llama.cpp | One model per pair | Fast and small, good for common pairs (en↔es, en↔de…). The license must be checked per pair. |
| **NLLB-200** (`facebook/nllb-200-*`) | **CC-BY-NC-4.0** (non-commercial). The card says it is "not released for production deployment". | 600M distilled, 1.3B, 3.3B | PyTorch; CTranslate2 | ~200 | **Do not ship** in a product meant to be commercially usable. |

- **MADLAD usage:** prepend `<2xx>` (for example `<2es> Hello`).
- **llama.cpp T5 status:**
  - Encoder-decoder decoding works in `llama-cli` (T5 support was merged mid-2024 and uses `llama_encode()`).
  - **`llama-server` does not run the encoder.** Issue #26565 ("llama-server / llama-cli never call `llama_encode()` for encoder-decoder (T5/mT5) models") was closed as *not planned* on 2026-09-18. PR #17956 ("server: add encoder-decoder model support (T5, BART, MADLAD)") is still open. Other PRs claim that T5 through the server produced garbage output.
  - In practice, run MADLAD in-process with `llama-cpp-2` (`encode` then `decode`) or through **CTranslate2** (`ct2rs` 0.11.3, used by Nextcloud's translate app). It cannot run behind the stock `llama-server` sidecar.
- **Alternative:** use the copilot LLM (Qwen) for translation through `llama-server`. Quality is acceptable for major languages, but it costs GPU time shared with the copilot.

## Qwen small instruct models — Apache-2.0

The newest small dense family as of 2026-11 is **Qwen3.5** (0.8B/2B/4B/9B, released 2026-02-27/28). Qwen3.6 and Qwen3.8 were released only at 27B and larger (or as MoE). The HF API shows no Qwen3.8 model under 27B.

| Model | License | Official GGUF | Q4_K_M | Q8_0 | Notes |
|---|---|---|---|---|---|
| Qwen3-0.6B | Apache-2.0 | `Qwen/Qwen3-0.6B-GGUF` | — | 0.64 GB | Text-only; `/think` `/no_think` switches |
| Qwen3-1.7B | Apache-2.0 | `Qwen/Qwen3-1.7B-GGUF` | — | 1.83 GB | |
| Qwen3-4B | Apache-2.0 | `Qwen/Qwen3-4B-GGUF` | 2.50 GB | 4.28 GB | 100+ languages |
| Qwen3-8B | Apache-2.0 | `Qwen/Qwen3-8B-GGUF` | 5.03 GB | 8.71 GB | |
| Qwen3-4B-Instruct-2507 | Apache-2.0 | none official (community) | — | — | Non-thinking variant |
| Qwen3.5-0.8B | Apache-2.0 | `ggml-org/Qwen3.5-0.8B-GGUF` | 0.53 GB* | 0.81 GB* | Vision+text, 201 languages |
| Qwen3.5-2B | Apache-2.0 | community (`unsloth/…`) | 1.28 GB* | 2.01 GB* | |
| Qwen3.5-4B | Apache-2.0 | `ggml-org/Qwen3.5-4B-GGUF` (Q4_K_M, Q8_0, mmproj, MTP draft) | 2.74 GB* | 4.48 GB* | 262k context |
| Qwen3.5-9B | Apache-2.0 | community (`unsloth/…`) | 5.68 GB* | 9.53 GB* | |

\* Sizes are from `unsloth/Qwen3.5-*-GGUF`.

- **No official Qwen-org GGUF for Qwen3.5.** `ggml-org` (the llama.cpp maintainers) publishes 0.8B and 4B.
- **Architecture:** Qwen3.5 is a hybrid of Gated DeltaNet and attention, ships with a vision tower, and thinks by default. Disable thinking with `--chat-template-kwargs '{"enable_thinking":false}'` or `--reasoning-budget 0`. It does **not** honor the `/think` `/no_think` soft switches.
- **Sampling:** the card recommends temperature 0.7, top_p 0.8, top_k 20 and presence_penalty 1.5 for non-thinking mode.
- **Budgeting:** KV cache and context come on top of the file size. Budget about 1–1.5 GB extra for an 8k context on 4B models. The mmproj (about 670 MB) can be skipped with `--no-mmproj` for text-only use.

## Recommended defaults

### GTX 1650 4 GB VRAM (Turing, CC 7.5) + 16 GB RAM

- **ASR (live):**
  - Use `ggml-small-q5_1` or `ggml-large-v3-turbo-q5_0` (574 MB) on CUDA or Vulkan.
  - Turbo-q5_0 fits next to a small LLM, but measure the real-time factor. If it misses about 0.5×RT on 1–2 s steps, fall back to small.
- **VAD:** `ggml-silero-v6.2.0.bin` through whisper.cpp (CPU).
- **Copilot LLM:** `Qwen3.5-4B Q4_K_M` (2.74 GB) with partial offload (`-ngl auto`, `--fit on`) and `-c 8192`. Alternatively Qwen3-4B Q4_K_M (2.5 GB, text-only, simpler template).
- **VRAM:** 4 GB cannot hold turbo plus a 4B model fully. Keep ASR on the GPU and let llama-server place layers automatically, with the remainder in system RAM.
- **Translation:** MADLAD-400-3B via CTranslate2 int8 on CPU (about 3 GB RAM), or OPUS-MT per pair for the lowest latency.
- **CUDA builds:** CUDA 13.x dropped Maxwell, Pascal and Volta. Turing is still supported (secondary sources; confirm in NVIDIA's support matrix). Prefer the `win-cuda-12.4` llama.cpp build for wider driver compatibility, or Vulkan.

### CPU-only (8+ cores, 16 GB)

- **ASR:** `ggml-base-q8_0` (82 MB) live, with `ggml-small-q5_1` as an optional final pass.
- **VAD:** Silero v6.2.0.
- **LLM:** `Qwen3.5-2B Q4_K_M` (1.28 GB) for the copilot. Use 0.8B on low-end machines.
- **MT:** OPUS-MT (CTranslate2 int8), with MADLAD-3B int8 as the "any language" option.

## Sources

- https://huggingface.co/ggerganov/whisper.cpp, https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md, https://github.com/ggml-org/whisper.cpp/releases
- https://huggingface.co/ggml-org/whisper-vad, https://github.com/snakers4/silero-vad
- https://huggingface.co/openai/whisper-large-v3-turbo, https://crates.io/crates/whisper-rs, https://docs.rs/whisper-rs/0.16.0
- https://huggingface.co/google/madlad400-3b-mt, https://huggingface.co/google/madlad400-7b-mt, https://huggingface.co/mtsdurica/madlad400-3b-mt-Q4_0-GGUF
- https://github.com/ggml-org/llama.cpp/issues/26565, https://github.com/ggml-org/llama.cpp/pull/17956
- https://huggingface.co/Helsinki-NLP/opus-mt-en-es, https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-en-es, https://huggingface.co/facebook/nllb-200-distilled-600M
- https://opennmt.net/CTranslate2/guides/transformers.html
- https://huggingface.co/Qwen/Qwen3.5-4B, https://huggingface.co/Qwen/Qwen3-4B-GGUF, https://huggingface.co/ggml-org/Qwen3.5-4B-GGUF, https://huggingface.co/unsloth/Qwen3.5-4B-GGUF
- https://docs.nvidia.com/cuda/archive/13.0.0/cuda-toolkit-release-notes/
