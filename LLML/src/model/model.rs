use llama_cpp::{LlamaModel, LlamaParams, SessionParams};
use llama_cpp::standard_sampler::StandardSampler;
use tracing::{debug, error, info, instrument, warn};

/// Parameters extracted from `ai-config.yaml` for a model entry.
#[derive(Debug)]
pub struct ModelParams {
    pub temperature: f32,
    pub max_tokens: usize,
}

/// Owns the loaded `LlamaModel`. Load once with [`ModelRunner::load`],
/// then call [`ModelRunner::generate_from_prompt`] for each request.
pub struct ModelRunner {
    pub(crate) model: LlamaModel,
    params: ModelParams,
}

// llama_cpp's LlamaModel is backed by a thread-safe C++ object.
// Sessions are created per-call so no shared mutable state exists.
unsafe impl Send for ModelRunner {}
unsafe impl Sync for ModelRunner {}

impl ModelRunner {
    /// Load the GGUF model from `path`. Called exactly once at startup.
    #[instrument(fields(path))]
    pub fn load(path: &str, params: ModelParams) -> anyhow::Result<Self> {
        info!(path, "loading GGUF model");
        let model = LlamaModel::load_from_file(path, LlamaParams::default())
            .map_err(|e| {
                error!(path, error = %e, "failed to load model from file");
                e
            })?;
        info!(path, "model loaded successfully");
        Ok(Self { model, params })
    }

    /// Run inference on a fully-formed prompt string.
    /// `max_tokens` overrides the config default when supplied by the caller.
    #[instrument(skip(self, prompt), fields(prompt_len = prompt.len(), max_tokens))]
    pub fn generate_from_prompt(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
    ) -> anyhow::Result<String> {
        let max = max_tokens.unwrap_or(self.params.max_tokens);
        debug!(prompt_len = prompt.len(), max_tokens = max, "creating inference session");

        let mut session = self.model.create_session(SessionParams::default())
            .map_err(|e| {
                error!(error = %e, "failed to create llama session");
                e
            })?;

        session.advance_context(prompt)
            .map_err(|e| {
                error!(error = %e, "failed to advance context");
                e
            })?;

        let mut stream = session
            .start_completing_with(StandardSampler::default(), max)
            .map_err(|e| {
                error!(error = %e, "failed to start completion stream");
                e
            })?;

        info!(max_tokens = max, "inference started");
        let mut output = String::new();
        let mut token_count: usize = 0;

        while let Some(token) = stream.next_token() {
            let piece = self.model.token_to_piece(token);
            if piece.contains("[/INST]") {
                warn!("[/INST] marker found in output — stopping early");
                break;
            }
            output.push_str(&piece);
            token_count += 1;
        }

        info!(tokens_generated = token_count, output_len = output.len(), "inference complete");
        Ok(output)
    }
}
