mod loalYaml;
mod model;
mod api;

use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};
use loalYaml::loadYaml::load_config;
use model::{ModelParams, ModelRunner};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialise structured logging.
    // Control verbosity via RUST_LOG, e.g.:
    //   RUST_LOG=info          — info and above (default)
    //   RUST_LOG=LLML=debug    — debug for this crate only
    //   RUST_LOG=debug         — everything including deps
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(true)
        .init();

    let config_path = "../ai-config.yaml";
    let config = load_config(config_path).map_err(|e| anyhow::anyhow!(e))?;

    let model_cfg = config.models.first()
        .ok_or_else(|| anyhow::anyhow!("No models defined in config"))?;

    let temperature = model_cfg.parameters.iter()
        .find(|p| p.name == "temperature")
        .and_then(|p| p.default.as_f64())
        .unwrap_or(0.7) as f32;

    let max_tokens = model_cfg.parameters.iter()
        .find(|p| p.name == "max_tokens")
        .and_then(|p| p.default.as_u64())
        .unwrap_or(100) as usize;

    let n_gpu_layers = model_cfg.parameters.iter()
        .find(|p| p.name == "n_gpu_layers")
        .and_then(|p| p.default.as_u64())
        .unwrap_or(0) as u32;

    let n_threads = model_cfg.parameters.iter()
        .find(|p| p.name == "n_threads")
        .and_then(|p| p.default.as_u64())
        .unwrap_or(0) as u32;

    let n_ctx = model_cfg.parameters.iter()
        .find(|p| p.name == "n_ctx")
        .and_then(|p| p.default.as_u64())
        .unwrap_or(2048) as u32;

    let n_batch = model_cfg.parameters.iter()
        .find(|p| p.name == "n_batch")
        .and_then(|p| p.default.as_u64())
        .unwrap_or(512) as u32;

    // Load the model once; wrap in Arc for shared access across requests.
    let runner = Arc::new(ModelRunner::load(
        &model_cfg.model_path,
        ModelParams { temperature, max_tokens, n_gpu_layers, n_threads, n_ctx, n_batch },
    )?);

    let app = api::create_router(runner);
    let addr = "0.0.0.0:3000";

    info!(addr, "LLML API server starting");
    info!("  POST /v1/chat/completions  — OpenAI-compatible chat endpoint");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

