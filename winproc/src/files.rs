//! A single validated handle traversal shared by changes and rewind.
pub use crate::rewind::platform::{open_catalog_file, open_plain_file};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn catalog_read_accepts_snapshot_hardlinks_without_weakening_strict_reads() {
        use std::io::Read;
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("source"), b"catalog bytes").unwrap();
        std::fs::hard_link(folder.path().join("source"), folder.path().join("snapshot")).unwrap();
        assert!(open_plain_file(folder.path(), Path::new("source")).is_err());
        let mut read = crate::open_catalog_file(folder.path(), Path::new("snapshot")).unwrap();
        let mut text = String::new();
        read.read_to_string(&mut text).unwrap();
        assert_eq!(text, "catalog bytes");
        assert!(crate::open_catalog_file(folder.path(), Path::new("../source")).is_err());
        assert!(crate::open_catalog_file(folder.path(), Path::new("")).is_err());
        std::fs::create_dir(folder.path().join("directory")).unwrap();
        assert!(crate::open_catalog_file(folder.path(), Path::new("directory")).is_err());
    }

    #[test]
    fn changes_open_rejects_parent_traversal_and_directories() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("notes")).unwrap();
        assert!(open_plain_file(folder.path(), Path::new("../outside.txt")).is_err());
        assert!(open_plain_file(folder.path(), Path::new("notes")).is_err());
        assert!(open_plain_file(folder.path(), Path::new("")).is_err());
    }
}
