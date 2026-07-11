use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
struct Args {
    /// Path to the JSON document to read.
    #[arg(short, long, default_value = "data.json")]
    input: String,
}

#[derive(Serialize, Deserialize)]
struct Document {
    title: String,
    tags: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let raw = std::fs::read_to_string(&args.input)
        .with_context(|| format!("reading {}", args.input))?;
    let doc: Document = serde_json::from_str(&raw).context("parsing JSON")?;
    println!("{} ({} tags)", doc.title, doc.tags.len());
    Ok(())
}
