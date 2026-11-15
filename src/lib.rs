//! privdat core library. The public API is exported to other languages through UniFFI.

uniffi::setup_scaffolding!();

/// A column in a data system, the unit that lineage edges connect.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ColumnRef {
    pub system: String,
    pub table: String,
    pub column: String,
}

/// Library version, so a client can check which build it loaded.
#[uniffi::export]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Dotted name of a column: `system.table.column`.
#[uniffi::export]
pub fn qualified_name(col: &ColumnRef) -> String {
    format!("{}.{}.{}", col.system, col.table, col.column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_name_joins_parts() {
        let col = ColumnRef {
            system: "pg".into(),
            table: "users".into(),
            column: "email".into(),
        };
        assert_eq!(qualified_name(&col), "pg.users.email");
    }
}
