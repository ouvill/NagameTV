fn main() {
    // Stable proc macros cannot invalidate Cargo's cache for these environment
    // changes themselves. Re-run SQL checking even when sources are unchanged.
    for variable in ["DATABASE_URL", "SQLX_OFFLINE", "SQLX_OFFLINE_DIR"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    for path in [".sqlx", "src/cache/schema.sql", "src/cache/schema-v2.sql"] {
        println!("cargo:rerun-if-changed={path}");
    }
}
