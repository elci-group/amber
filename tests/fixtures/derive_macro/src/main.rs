use serde::Serialize;

#[derive(Serialize)]
struct User {
    name: String,
}

fn main() {
    let _ = User { name: "Alice".to_string() };
}
