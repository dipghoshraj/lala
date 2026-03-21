mod loalYaml;

use loalYaml::loadYaml::load_config;

fn main() {
    let config_path = "../ai-config.yaml";
    match load_config(config_path) {
        Ok(config) => {
            println!("Loaded config v{}", config.version);
            println!("Available model types: {:?}", config.model_types.types);
            for model in &config.models {
                println!("Model: {} ({})", model.name, model.model_type);
                println!("  Path: {}", model.model_path);
                for param in &model.parameters {
                    println!("  Param: {} = {:?}", param.name, param.default);
                }
            }
        }
        Err(e) => eprintln!("Error loading config: {}", e),
    }
}
