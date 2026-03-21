use std::io::{self, Write};
use llama_cpp::{LlamaModel, LlamaParams, SessionParams};
use llama_cpp::standard_sampler::StandardSampler;

/// Parameters extracted from `ai-config.yaml` for a model entry.
pub struct ModelParams {
    pub temperature: f32,
    pub max_tokens: usize,
}

/// Owns the loaded `LlamaModel`. Load once with [`ModelRunner::load`],
/// then call [`ModelRunner::generate`] as many times as needed.
pub struct ModelRunner {
    model: LlamaModel,
    params: ModelParams,
}

impl ModelRunner {
    /// Load the GGUF model from `path` using the given `params`.
    /// This is the only place `LlamaModel::load_from_file` is called.
    pub fn load(path: &str, params: ModelParams) -> anyhow::Result<Self> {
        println!("Loading model from: {}", path);
        let model = LlamaModel::load_from_file(path, LlamaParams::default())?;
        println!("Model loaded successfully.");
        Ok(Self { model, params })
    }

    /// Run a single inference pass. A fresh session is created per call so
    /// there is no context bleed between prompts.
    pub fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        let mut session = self.model.create_session(SessionParams::default())?;

        let full_prompt = format!("<s>[INST]\n{}\n[/INST]", prompt);
        session.advance_context(&full_prompt)?;

        let mut stream = session.start_completing_with(
            StandardSampler::default(),
            self.params.max_tokens,
        )?;

        let mut output = String::new();
        while let Some(token) = stream.next_token() {
            let piece = self.model.token_to_piece(token);
            // Guard against the model echoing the instruction marker
            if piece.contains("[/INST]") {
                break;
            }
            output.push_str(&piece);
            print!("{}", piece);
            io::stdout().flush()?;
        }
        println!();
        Ok(output)
    }

    pub fn params(&self) -> &ModelParams {
        &self.params
    }
}
