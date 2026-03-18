fn main() -> Result<(), Box<dyn std::error::Error>> {
    agents::evals::build()?;
    Ok(())
}
