use std::io::{self, Write};
use crate::agent::model::ModelWrapper;

pub fn run(model_path: &str) -> anyhow::Result<()> {
    let model = ModelWrapper::load(model_path)?;
    let mut session = model.create_session()?;

    println!("🦙 llama-agent ready (/exit to quit)");

    loop {
        print!(">> ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input == "/exit" {
            break;
        }

        let response = session.complete(input, 512)?;
        println!("{}", response);
    }

    Ok(())
}
