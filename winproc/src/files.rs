//! A single validated handle traversal shared by changes and rewind.
pub use crate::rewind::platform::open_plain_file;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn changes_open_rejects_parent_traversal_and_directories() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("notes")).unwrap();
        assert!(open_plain_file(folder.path(), Path::new("../outside.txt")).is_err());
        assert!(open_plain_file(folder.path(), Path::new("notes")).is_err());
        assert!(open_plain_file(folder.path(), Path::new("")).is_err());
    }
}
