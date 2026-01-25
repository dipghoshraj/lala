mod agent;
mod cli;


fn main() -> anyhow::Result<()> {
    let model_path = std::env::args()
        .nth(1)
        .expect("Usage: llama-agent <model.gguf>");

    cli::run(&model_path)
}
