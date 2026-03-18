fn main() -> Result<(), Box<dyn std::error::Error>> {
    evals::build()?;
    Ok(())
}
