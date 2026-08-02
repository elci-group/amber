use anyhow::Result;

/// Return a static description, exercising a fully-qualified path.
pub fn describe() -> anyhow::Result<&'static str> {
    Ok("helper module")
}
