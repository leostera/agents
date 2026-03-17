fn main() -> Result<(), Box<dyn std::error::Error>> {
    borg_evals::build()?;
    Ok(())
}
