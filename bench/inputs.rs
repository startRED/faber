//! Run-local benchmark inputs, independent of later edits in the source checkout.
use std::path::{Path, PathBuf};

pub fn snapshot(source: &Path, run: &Path) -> std::io::Result<PathBuf> {
    let inputs = std::path::absolute(run.join("inputs"))?;
    std::fs::create_dir(&inputs)?;
    for dir in ["requests", "fixtures", "proofs"] {
        copy_tree(&source.join(dir), &inputs.join(dir))?;
    }
    Ok(inputs)
}

pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Self {
            Self::under(&std::env::temp_dir())
        }

        fn under(parent: &Path) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = parent.join(format!("faber-bench-inputs-{}-{nonce}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn source(root: &Path) -> PathBuf {
        let source = root.join("source");
        for dir in ["requests", "fixtures", "proofs/shared"] {
            fs::create_dir_all(source.join(dir)).unwrap();
        }
        fs::write(source.join("requests/task.md"), "---\nid: task\n---\nKeep every file.\n").unwrap();
        fs::write(source.join("fixtures/task.mjs"), "original fixture").unwrap();
        fs::write(source.join("proofs/task.mjs"), "original proof").unwrap();
        fs::write(source.join("proofs/shared/data.bin"), [0, 255, 42]).unwrap();
        source
    }

    #[test]
    fn both_sides_keep_the_original_inputs_after_source_edits_and_removal() {
        let temp = Temp::new();
        let source = source(&temp.0);
        let inputs = snapshot(&source, &temp.0).unwrap();
        let plain_proof = fs::read(inputs.join("proofs/task.mjs")).unwrap();
        fs::write(source.join("proofs/task.mjs"), "changed proof").unwrap();
        fs::write(source.join("fixtures/task.mjs"), "changed fixture").unwrap();
        fs::write(source.join("requests/task.md"), "different prompt").unwrap();
        assert_eq!(fs::read(inputs.join("proofs/task.mjs")).unwrap(), plain_proof);
        assert_eq!(fs::read_to_string(inputs.join("fixtures/task.mjs")).unwrap(), "original fixture");
        assert_eq!(fs::read_to_string(inputs.join("requests/task.md")).unwrap(), "---\nid: task\n---\nKeep every file.\n");
        fs::remove_dir_all(source).unwrap();
        assert_eq!(fs::read(inputs.join("proofs/shared/data.bin")).unwrap(), [0, 255, 42]);
    }

    #[test]
    fn a_second_snapshot_cannot_overwrite_the_first_runs_inputs() {
        let temp = Temp::new();
        let source = source(&temp.0);
        let inputs = snapshot(&source, &temp.0).unwrap();
        fs::write(source.join("proofs/task.mjs"), "changed proof").unwrap();
        assert!(snapshot(&source, &temp.0).is_err());
        assert_eq!(fs::read_to_string(inputs.join("proofs/task.mjs")).unwrap(), "original proof");
    }

    #[test]
    fn relative_run_directories_return_inputs_safe_for_another_working_directory() {
        let cwd = std::env::current_dir().unwrap();
        let temp = Temp::under(&cwd);
        let source = source(&temp.0);
        let relative_run = temp.0.strip_prefix(&cwd).unwrap();
        let inputs = snapshot(&source, relative_run).unwrap();
        assert!(inputs.is_absolute(), "proofs run with a different working directory");
        assert_eq!(fs::read_to_string(inputs.join("proofs/task.mjs")).unwrap(), "original proof");
    }

    #[test]
    fn missing_materials_fail_before_a_snapshot_is_returned() {
        let temp = Temp::new();
        let source = source(&temp.0);
        fs::remove_dir_all(source.join("proofs")).unwrap();
        assert!(snapshot(&source, &temp.0).is_err());
    }
}
