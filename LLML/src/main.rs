mod loalYaml;
mod model;

use loalYaml::loadYaml::load_config;
use model::{ModelParams, ModelRunner};

fn main() -> anyhow::Result<()> {
    let config_path = "../ai-config.yaml";
    let config = load_config(config_path).map_err(|e| anyhow::anyhow!(e))?;

    // Use the first model entry in the config.
    let model_cfg = config.models.first()
        .ok_or_else(|| anyhow::anyhow!("No models defined in config"))?;

    // Extract parameters with fallback to defaults.
    let temperature = model_cfg.parameters.iter()
        .find(|p| p.name == "temperature")
        .and_then(|p| p.default.as_f64())
        .unwrap_or(0.7) as f32;

    let max_tokens = model_cfg.parameters.iter()
        .find(|p| p.name == "max_tokens")
        .and_then(|p| p.default.as_u64())
        .unwrap_or(100) as usize;

    println!("Config: model={}, temperature={}, max_tokens={}", model_cfg.name, temperature, max_tokens);

    // Load the model once.
    let runner = ModelRunner::load(
        &model_cfg.model_path,
        ModelParams { temperature, max_tokens },
    )?;

    // Simple REPL — type a prompt, press Enter, get a response. Empty line exits.
    let stdin = std::io::stdin();
    let mut input = String::new();
    loop {
        print!(">> ");
        std::io::Write::flush(&mut std::io::stdout())?;
        input.clear();
        stdin.read_line(&mut input)?;
        let prompt = input.trim();
        if prompt.is_empty() || prompt == "/exit" {
            break;
        }
        runner.generate(prompt)?;
    }

    Ok(())
}

