use rusqlite::Connection;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=migrations");

    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo");
    let migrations_dir = Path::new(&manifest_dir).join("migrations");
    let sqlx_dev_db = Path::new(&manifest_dir).join(".sqlx-dev.db");

    rebuild_sqlx_dev_db(&sqlx_dev_db, &migrations_dir);
    let db_url = format!("sqlite://{}", sqlx_dev_db.display());
    println!("cargo:rustc-env=DATABASE_URL={db_url}");

    emit_watch_entries(&migrations_dir);
}

fn emit_watch_entries(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            emit_watch_entries(&path);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("sql") {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn rebuild_sqlx_dev_db(db_path: &Path, migrations_dir: &Path) {
    if db_path.exists() {
        fs::remove_file(db_path).expect("failed to remove existing .sqlx-dev.db");
    }

    let conn = Connection::open(db_path).expect("failed to create .sqlx-dev.db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("failed to initialize sqlite pragmas");

    let mut migration_files = collect_migration_files(migrations_dir);
    migration_files.sort();
    for migration in migration_files {
        let sql = fs::read_to_string(&migration).unwrap_or_else(|err| {
            panic!("failed to read migration {}: {err}", migration.display())
        });
        conn.execute_batch(&sql).unwrap_or_else(|err| {
            panic!(
                "failed applying migration {} to {}: {err}",
                migration.display(),
                db_path.display()
            )
        });
    }
}

fn collect_migration_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_migration_files(&path));
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("sql") {
            out.push(path);
        }
    }
    out
}
