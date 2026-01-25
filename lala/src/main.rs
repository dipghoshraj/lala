use llama_cpp::{LlamaModel, LlamaParams, SessionParams};
use llama_cpp::standard_sampler::StandardSampler;
use std::io::{self, Write};

fn main() {
    // Load model
    let model = LlamaModel::load_from_file("E:/lala/model/mistral-7b-v0.1.Q4_K_M.gguf", LlamaParams::default())
        .expect("Failed to load model");

    // Create session
    let mut session = model.create_session(SessionParams::default())
        .expect("Failed to create session");

    session.advance_context("Once upon a time,").unwrap();

    let max_tokens = 512;

    // Unwrap the Result before calling into_strings
    let mut output_tokens = session
        .start_completing_with(StandardSampler::default(), max_tokens)
        .expect("Failed to start completion")
        .into_strings(); // now this works!

    for token in output_tokens {
        print!("{token}");
        io::stdout().flush().unwrap();
    }
}
