use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "src/context.rs"]
mod context;
#[path = "src/scalars.rs"]
mod scalars;
#[path = "src/sdl/mod.rs"]
mod sdl;

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/context.rs");
    println!("cargo:rerun-if-changed=src/scalars.rs");
    println!("cargo:rerun-if-changed=src/sdl/mod.rs");
    println!("cargo:rerun-if-changed=src/sdl/resolvers/mod.rs");
    println!("cargo:rerun-if-changed=src/sdl/resolvers/query.rs");
    println!("cargo:rerun-if-changed=src/sdl/resolvers/mutation.rs");
    println!("cargo:rerun-if-changed=src/sdl/resolvers/subscription.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime for schema generation");

    runtime
        .block_on(generate_schema_artifacts())
        .expect("failed to generate GraphQL schema artifacts");
}

async fn generate_schema_artifacts() -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    let db_path = out_dir.join("schema-build-config.db");
    let memory_path = out_dir.join("schema-build-memory.db");
    let search_path = out_dir.join("schema-build-search.db");

    let db_path_text = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("invalid db path"))?;
    let db = borg_db::BorgDb::open_local(db_path_text).await?;
    db.migrate().await?;

    let memory = borg_memory::MemoryStore::new(&memory_path, &search_path)?;
    memory.migrate().await?;

    let schema =
        async_graphql::Schema::build(sdl::QueryRoot, sdl::MutationRoot, sdl::SubscriptionRoot)
            .data(context::BorgGqlData::new(db, memory))
            .limit_depth(100)
            .limit_complexity(4_000)
            .finish();
    let schema_sdl = schema.sdl();

    let schema_path = manifest_dir.join("schema.graphql");
    let out_schema_path = out_dir.join("schema.graphql");

    write_if_changed(&schema_path, &schema_sdl)?;
    write_if_changed(&out_schema_path, &schema_sdl)?;

    Ok(())
}

fn write_if_changed(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let should_write = match fs::read_to_string(path) {
        Ok(existing) => existing != contents,
        Err(_) => true,
    };

    if should_write {
        fs::write(path, contents)?;
    }

    Ok(())
}
