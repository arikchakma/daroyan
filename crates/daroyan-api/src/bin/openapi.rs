use daroyan_api::app::build_spec;

fn main() -> anyhow::Result<()> {
    println!("{}", build_spec().to_pretty_json()?);
    Ok(())
}
