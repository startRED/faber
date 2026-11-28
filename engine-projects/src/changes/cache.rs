//! Elide untouched large files using Git's cached stat data, without reading their contents.
use std::fs::Metadata;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq, Eq)]
struct Cached {
    ctime: (u64, u32),
    mtime: (u64, u32),
    dev: u64,
    ino: u64,
    uid: u64,
    gid: u64,
    size: u64,
}

fn timestamp(value: &str) -> Option<(u64, u32)> {
    let (seconds, nanos) = value.split_once(':')?;
    let nanos = nanos.parse().ok()?;
    (nanos < 1_000_000_000).then_some((seconds.parse().ok()?, nanos))
}

fn system_time(time: SystemTime) -> Option<(u64, u32)> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    Some((duration.as_secs(), duration.subsec_nanos()))
}

/// Debug output is explicitly unstable: an unfamiliar field or shape must retain the unknown row.
fn cached(bytes: &[u8], path: &str, expected: &(String, String)) -> Option<Cached> {
    let end = bytes.iter().position(|b| *b == 0)?;
    if identity(&bytes[..=end], path, false).as_ref() != Some(expected) {
        return None;
    }
    let fields: Vec<_> = std::str::from_utf8(&bytes[end + 1..]).ok()?.split_whitespace().collect();
    if fields.len() != 16
        || fields.iter().step_by(2).copied().collect::<Vec<_>>() != ["ctime:", "mtime:", "dev:", "ino:", "uid:", "gid:", "size:", "flags:"]
        || u64::from_str_radix(fields[15], 16).ok()? != 0
    {
        return None;
    }
    Some(Cached {
        ctime: timestamp(fields[1])?,
        mtime: timestamp(fields[3])?,
        dev: fields[5].parse().ok()?,
        ino: fields[7].parse().ok()?,
        uid: fields[9].parse().ok()?,
        gid: fields[11].parse().ok()?,
        size: fields[13].parse().ok()?,
    })
}

fn identity(bytes: &[u8], path: &str, tree: bool) -> Option<(String, String)> {
    let mut found = None;
    for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = std::str::from_utf8(record).ok()?;
        let (header, name) = record.split_once('\t')?;
        if name != path {
            continue;
        }
        let fields: Vec<_> = header.split_whitespace().collect();
        if found.is_some() || !matches!(fields.first().copied(), Some("100644" | "100755")) {
            return None;
        }
        let oid = if tree {
            if fields.len() != 4 || fields[1] != "blob" {
                return None;
            }
            fields[2]
        } else {
            if fields.len() != 3 || fields[2] != "0" {
                return None;
            }
            fields[1]
        };
        if !crate::project::git::valid_oid(oid) {
            return None;
        }
        found = Some((fields[0].to_string(), oid.to_string()));
    }
    found
}

fn matches_disk(cache: &Cached, metadata: &Metadata, mode: &str, index_time: (u64, u32)) -> bool {
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || cache.size != metadata.len()
        || metadata.modified().ok().and_then(system_time) != Some(cache.mtime)
        || cache.mtime >= index_time
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        cache.ctime == (metadata.ctime() as u64, metadata.ctime_nsec() as u32)
            && cache.ino == u64::from(metadata.ino() as u32)
            && cache.uid == u64::from(metadata.uid())
            && cache.gid == u64::from(metadata.gid())
            && cache.dev == u64::from(metadata.dev() as u32)
            && (mode == "100755") == (metadata.mode() & 0o100 != 0)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let _ = mode;
        metadata.file_attributes() & 0x400 == 0
            && metadata.created().ok().and_then(system_time) == Some(cache.ctime)
            && cache.dev == 0
            && cache.ino == 0
            && cache.uid == 0
            && cache.gid == 0
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = mode;
        false
    }
}

pub(super) fn unchanged(request: &mut super::Request<'_>, root: &Path, path: &str, index: &[u8], tree: &[u8]) -> bool {
    let Some(expected) = identity(index, path, false) else {
        return false;
    };
    if identity(tree, path, true).as_ref() != Some(&expected) {
        return false;
    }
    let Some(index_path) = request.git(root, &["rev-parse", "--git-path", "index"], 0, 32 * 1024) else {
        return false;
    };
    if index_path.len() > 32 * 1024 {
        return false;
    }
    let Some(index_path) = std::str::from_utf8(&index_path).ok().map(|s| root.join(s.trim_end_matches(['\r', '\n']))) else {
        return false;
    };
    let Ok(before) = std::fs::metadata(&index_path) else {
        return false;
    };
    if !before.is_file() {
        return false;
    }
    let Some(index_time) = before.modified().ok().and_then(system_time) else {
        return false;
    };
    let Some(debug) = request.git(root, &["ls-files", "--stage", "--debug", "-z", "--full-name", "--", path], 0, 32 * 1024) else {
        return false;
    };
    if debug.len() > 32 * 1024 {
        return false;
    }
    let Some(cache) = cached(&debug, path, &expected) else {
        return false;
    };
    let Ok(metadata) = std::fs::symlink_metadata(root.join(path)) else {
        return false;
    };
    let Ok(after) = std::fs::metadata(&index_path) else {
        return false;
    };
    before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
        && matches_disk(&cache, &metadata, &expected.0, index_time)
}

#[cfg(test)]
mod tests {
    use super::*;
    const OID: &str = "1111111111111111111111111111111111111111";

    fn debug(path: &str) -> Vec<u8> {
        format!("100644 {OID} 0\t{path}\0  ctime: 20:100\n  mtime: 30:200\n  dev: 0\tino: 0\n  uid: 0\tgid: 0\n  size: 100\tflags: 0\n")
            .into_bytes()
    }

    #[test]
    fn changes_cache_requires_exact_stage_zero_identity_and_complete_debug_fields() {
        let expected = ("100644".into(), OID.into());
        let sample = debug("a\tfile.txt");
        let parsed = cached(&sample, "a\tfile.txt", &expected).unwrap();
        assert_eq!((parsed.ctime, parsed.mtime, parsed.size), ((20, 100), (30, 200), 100));
        for bad in [
            sample[..sample.len() - 10].to_vec(),
            debug("different.txt"),
            [sample.clone(), sample.clone()].concat(),
            String::from_utf8(sample.clone()).unwrap().replace("flags: 0", "flags: 8000").into_bytes(),
            String::from_utf8(sample.clone()).unwrap().replace("mtime:", "unknown:").into_bytes(),
        ] {
            assert!(cached(&bad, "a\tfile.txt", &expected).is_none());
        }
        assert!(identity(format!("100644 {OID} 1\tfile\0").as_bytes(), "file", false).is_none());
        assert_ne!(identity(format!("100644 blob {} 100\tfile\0", "2".repeat(40)).as_bytes(), "file", true), Some(expected));
    }

    #[test]
    fn changes_cache_only_trusts_matching_non_racy_file_metadata() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large.txt");
        std::fs::write(&path, vec![b'x'; 1024]).unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let mtime = system_time(metadata.modified().unwrap()).unwrap();
        let mut cache = Cached { ctime: (0, 0), mtime, dev: 0, ino: 0, uid: 0, gid: 0, size: metadata.len() };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            cache.ctime = (metadata.ctime() as u64, metadata.ctime_nsec() as u32);
            cache.dev = u64::from(metadata.dev() as u32);
            cache.ino = u64::from(metadata.ino() as u32);
            cache.uid = u64::from(metadata.uid());
            cache.gid = u64::from(metadata.gid());
        }
        #[cfg(windows)]
        {
            cache.ctime = system_time(metadata.created().unwrap()).unwrap();
        }
        assert!(matches_disk(&cache, &metadata, "100644", (mtime.0 + 1, 0)));
        assert!(!matches_disk(&cache, &metadata, "100644", mtime), "a racily clean entry must retain its unknown row");
        cache.size += 1;
        assert!(!matches_disk(&cache, &metadata, "100644", (mtime.0 + 1, 0)));
        cache.size -= 1;
        cache.ctime.0 += 1;
        assert!(!matches_disk(&cache, &metadata, "100644", (mtime.0 + 1, 0)));
    }
}
