use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct User {
    name: String,
    age: u32,
}

fn main() -> Result<()> {
    let user = User {
        name: "Alice".to_string(),
        age: 30,
    };

    let json = serde_json::to_string(&user).context("serialize user")?;
    println!("{}", json);

    anyhow::bail!("demo error");
}
