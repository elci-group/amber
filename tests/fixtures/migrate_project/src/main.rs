use anyhow::{Context, Result};

mod helpers;

fn load_name() -> Result<String> {
    let raw = std::fs::read_to_string("name.txt").context("read name file")?;
    Ok(raw.trim().to_string())
}

fn main() -> Result<()> {
    match load_name() {
        Ok(name) => println!("hello {name}"),
        Err(error) => {
            // This comment mentions anyhow::Context and must stay untouched.
            let wrapped = anyhow::anyhow(format!("loading failed: {error}"));
            println!("{wrapped}");
        }
    }
    let _ = helpers::describe();
    Ok(())
}
