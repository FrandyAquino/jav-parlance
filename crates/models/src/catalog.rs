use parlance_protocol::ModelKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archive {
    Zip,
    TarGz,
}

/// One downloadable artifact. Hashes and sizes were taken from the Hugging
/// Face LFS metadata and GitHub release digests (see docs/research/model-licenses.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub kind: ModelKind,
    pub name: &'static str,
    pub description: &'static str,
    pub license: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub is_default: bool,
    /// Archives are extracted into a directory named after `id`.
    pub archive: Option<Archive>,
}

macro_rules! whisper_url {
    ($file:literal) => {
        concat!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/", $file)
    };
}

macro_rules! llama_release_url {
    ($file:literal) => {
        concat!("https://github.com/ggml-org/llama.cpp/releases/download/b12686/", $file)
    };
}

const MODELS: &[CatalogEntry] = &[
    CatalogEntry {
        id: "whisper-base-q8",
        kind: ModelKind::Speech,
        name: "Whisper Base (Q8)",
        description: "Fast multilingual transcription; real-time on most CPUs.",
        license: "MIT",
        file_name: "ggml-base-q8_0.bin",
        url: whisper_url!("ggml-base-q8_0.bin"),
        sha256: "c577b9a86e7e048a0b7eada054f4dd79a56bbfa911fbdacf900ac5b567cbb7d9",
        size_bytes: 81_768_585,
        is_default: true,
        archive: None,
    },
    CatalogEntry {
        id: "whisper-small-q8",
        kind: ModelKind::Speech,
        name: "Whisper Small (Q8)",
        description: "Noticeably more accurate; needs a fast CPU or a GPU.",
        license: "MIT",
        file_name: "ggml-small-q8_0.bin",
        url: whisper_url!("ggml-small-q8_0.bin"),
        sha256: "49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f",
        size_bytes: 264_464_607,
        is_default: false,
        archive: None,
    },
    CatalogEntry {
        id: "whisper-large-v3-turbo-q5",
        kind: ModelKind::Speech,
        name: "Whisper Large v3 Turbo (Q5)",
        description: "Best accuracy; recommended with GPU acceleration.",
        license: "MIT",
        file_name: "ggml-large-v3-turbo-q5_0.bin",
        url: whisper_url!("ggml-large-v3-turbo-q5_0.bin"),
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        size_bytes: 574_041_195,
        is_default: false,
        archive: None,
    },
    CatalogEntry {
        id: "silero-vad-5.1.2",
        kind: ModelKind::VoiceActivity,
        name: "Silero VAD v5.1.2",
        description: "Speech detection that gates transcription.",
        license: "MIT",
        file_name: "ggml-silero-v5.1.2.bin",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
        sha256: "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
        size_bytes: 885_098,
        is_default: true,
        archive: None,
    },
    CatalogEntry {
        id: "qwen3.5-2b-q4",
        kind: ModelKind::Language,
        name: "Qwen3.5 2B (Q4_K_M)",
        description: "Translation and copilot replies; runs on CPU.",
        license: "Apache-2.0",
        file_name: "Qwen3.5-2B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf",
        sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
        size_bytes: 1_280_835_840,
        is_default: true,
        archive: None,
    },
    CatalogEntry {
        id: "qwen3.5-4b-q4",
        kind: ModelKind::Language,
        name: "Qwen3.5 4B (Q4_K_M)",
        description: "Higher quality replies; best with a 4 GB+ GPU.",
        license: "Apache-2.0",
        file_name: "Qwen3.5-4B-Q4_K_M.gguf",
        url: "https://huggingface.co/ggml-org/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
        sha256: "217002fa8008ef4fc6d548353a3aebef055a96e6ff8e0e2c0953ef8254a89c14",
        size_bytes: 3_032_091_584,
        is_default: false,
        archive: None,
    },
    RUNTIME,
];

#[cfg(all(windows, target_arch = "x86_64"))]
const RUNTIME: CatalogEntry = CatalogEntry {
    id: "llama-server",
    kind: ModelKind::Runtime,
    name: "llama.cpp server b12686 (Vulkan)",
    description: "Local LLM runtime; Vulkan GPU offload with CPU fallback.",
    license: "MIT",
    file_name: "llama-b12686-bin-win-vulkan-x64.zip",
    url: llama_release_url!("llama-b12686-bin-win-vulkan-x64.zip"),
    sha256: "c6e65326dc4a529a1ba75ec66130bf493311b3c26ce0e07cf7529b927fb75529",
    size_bytes: 39_075_952,
    is_default: true,
    archive: Some(Archive::Zip),
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const RUNTIME: CatalogEntry = CatalogEntry {
    id: "llama-server",
    kind: ModelKind::Runtime,
    name: "llama.cpp server b12686 (Metal)",
    description: "Local LLM runtime with Metal acceleration.",
    license: "MIT",
    file_name: "llama-b12686-bin-macos-arm64.tar.gz",
    url: llama_release_url!("llama-b12686-bin-macos-arm64.tar.gz"),
    sha256: "845a33b288f1fa1926346a9fd6714bfa3e7ba1c149e78ab95907305b6112a3e8",
    size_bytes: 16_818_959,
    is_default: true,
    archive: Some(Archive::TarGz),
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const RUNTIME: CatalogEntry = CatalogEntry {
    id: "llama-server",
    kind: ModelKind::Runtime,
    name: "llama.cpp server b12686 (Vulkan)",
    description: "Local LLM runtime; Vulkan GPU offload with CPU fallback.",
    license: "MIT",
    file_name: "llama-b12686-bin-ubuntu-vulkan-x64.tar.gz",
    url: llama_release_url!("llama-b12686-bin-ubuntu-vulkan-x64.tar.gz"),
    sha256: "9e34411000a80c49a5e645df9b82d1556fd8869a02275cb3ad267b6a81e34fa8",
    size_bytes: 37_073_391,
    is_default: true,
    archive: Some(Archive::TarGz),
};

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const RUNTIME: CatalogEntry = CatalogEntry {
    id: "llama-server",
    kind: ModelKind::Runtime,
    name: "llama.cpp server b12686 (Intel Mac)",
    description: "Local LLM runtime for Intel Macs.",
    license: "MIT",
    file_name: "llama-b12686-bin-macos-x64.tar.gz",
    url: llama_release_url!("llama-b12686-bin-macos-x64.tar.gz"),
    sha256: "80683324dbc5f6dc88f898fcda0066311ae75724e62e856b768c1f55a3b12d03",
    size_bytes: 12_538_857,
    is_default: true,
    archive: Some(Archive::TarGz),
};

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const RUNTIME: CatalogEntry = CatalogEntry {
    id: "llama-server",
    kind: ModelKind::Runtime,
    name: "llama.cpp server b12686 (ARM64 CPU)",
    description: "Local LLM runtime for ARM64 Linux.",
    license: "MIT",
    file_name: "llama-b12686-bin-ubuntu-arm64.tar.gz",
    url: llama_release_url!("llama-b12686-bin-ubuntu-arm64.tar.gz"),
    sha256: "414a9148bdc6fff0a9689c33a489e5854858ec6dcceae5c08de5abb324d0f862",
    size_bytes: 18_928_872,
    is_default: true,
    archive: Some(Archive::TarGz),
};

#[cfg(all(windows, target_arch = "aarch64"))]
const RUNTIME: CatalogEntry = CatalogEntry {
    id: "llama-server",
    kind: ModelKind::Runtime,
    name: "llama.cpp server b12686 (ARM64 CPU)",
    description: "Local LLM runtime for ARM64 Windows.",
    license: "MIT",
    file_name: "llama-b12686-bin-win-cpu-arm64.zip",
    url: llama_release_url!("llama-b12686-bin-win-cpu-arm64.zip"),
    sha256: "2ca96d0dfb0cc7c4f3c21df8552c7028a840c49403038c9b12276e38fc8bce6e",
    size_bytes: 13_284_194,
    is_default: true,
    archive: Some(Archive::Zip),
};

pub fn catalog() -> &'static [CatalogEntry] {
    MODELS
}

pub fn find(id: &str) -> Option<&'static CatalogEntry> {
    MODELS.iter().find(|entry| entry.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_urls_resolved() {
        let mut ids: Vec<_> = catalog().iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), catalog().len());
        assert!(catalog().iter().all(|e| e.url.starts_with("https://") && e.url.ends_with(e.file_name)));
    }

    #[test]
    fn hashes_are_sha256_hex() {
        assert!(catalog().iter().all(|e| e.sha256.len() == 64 && e.sha256.chars().all(|c| c.is_ascii_hexdigit())));
    }

    #[test]
    fn every_kind_has_a_default() {
        for kind in [ModelKind::Speech, ModelKind::VoiceActivity, ModelKind::Language, ModelKind::Runtime] {
            assert_eq!(catalog().iter().filter(|e| e.kind == kind && e.is_default).count(), 1, "{kind:?}");
        }
    }
}
