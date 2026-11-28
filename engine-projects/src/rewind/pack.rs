//! Undeltified Git PACK/IDX v2 batches. Callers own handle validation, admission and durable publication.
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

const BUFFER: usize = 1024 * 1024;
const FILE_LIMIT: u64 = 8 * 1024 * 1024;
const PACK_LIMIT: u64 = 65 * 1024 * 1024;
const PATH_LIMIT: usize = 20_000;

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub(super) oid: String,
    pub(super) offset: u64,
    pub(super) bytes: u64,
    pub(super) crc: u32,
    digest: [u8; 32],
}

pub(super) struct Batch {
    file: File,
    entries: Vec<Entry>,
    buffer: Vec<u8>,
}

struct CheckedWriter<'a> {
    file: &'a mut File,
    crc: crc32fast::Hasher,
}
impl Write for CheckedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.file.write(bytes)?;
        self.crc.update(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

impl Batch {
    pub(super) fn new(mut file: File) -> io::Result<Self> {
        if file.metadata()?.len() != 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        file.seek(SeekFrom::Start(0))?;
        file.write_all(b"PACK\0\0\0\x02\0\0\0\0")?;
        Ok(Self { file, entries: Vec::new(), buffer: vec![0; BUFFER] })
    }

    pub(super) fn file(&self) -> &File {
        &self.file
    }

    pub(super) fn into_file(self) -> File {
        self.file
    }

    pub(super) fn count(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn add(&mut self, source: &mut dyn Read, len: u64) -> io::Result<Entry> {
        if len > FILE_LIMIT || self.entries.len() >= PATH_LIMIT {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let offset = self.file.stream_position()?;
        let result = self.add_at(source, len, offset);
        match result {
            Ok(entry) => {
                if let Some(existing) = self.entries.iter().find(|existing| existing.oid == entry.oid).cloned() {
                    self.file.set_len(offset)?;
                    self.file.seek(SeekFrom::Start(offset))?;
                    return Ok(existing);
                }
                self.entries.push(entry.clone());
                Ok(entry)
            }
            Err(error) => {
                // A failed or unstable source cannot poison the next file in this batch.
                self.file.set_len(offset)?;
                self.file.seek(SeekFrom::Start(offset))?;
                Err(error)
            }
        }
    }

    fn add_at(&mut self, source: &mut dyn Read, len: u64, offset: u64) -> io::Result<Entry> {
        let mut writer = CheckedWriter { file: &mut self.file, crc: crc32fast::Hasher::new() };
        let mut remaining = len >> 4;
        let mut byte = 0x30 | (len as u8 & 15);
        loop {
            writer.write_all(&[byte | if remaining != 0 { 128 } else { 0 }])?;
            if remaining == 0 {
                break;
            }
            byte = (remaining & 127) as u8;
            remaining >>= 7;
        }
        let mut encoder = ZlibEncoder::new(writer, Compression::none());
        let mut hash = Sha256::new();
        hash.update(format!("blob {len}\0").as_bytes());
        let mut count = 0u64;
        loop {
            let n = source.read(&mut self.buffer)?;
            if n == 0 {
                break;
            }
            count = count.checked_add(n as u64).ok_or_else(invalid)?;
            if count > len {
                return Err(invalid());
            }
            hash.update(&self.buffer[..n]);
            encoder.write_all(&self.buffer[..n])?;
        }
        if count != len {
            return Err(invalid());
        }
        let writer = encoder.finish()?;
        let crc = writer.crc.finalize();
        if self.file.stream_position()? > PACK_LIMIT - 32 {
            return Err(invalid());
        }
        let digest: [u8; 32] = hash.finalize().into();
        Ok(Entry { oid: crate::hex(&digest), offset, bytes: len, crc, digest })
    }

    /// Does not sync: the caller flushes this one completed payload before publishing any reference to it.
    pub(super) fn finish(mut self) -> io::Result<(File, String, Vec<u8>)> {
        let end = self.file.stream_position()?;
        self.file.seek(SeekFrom::Start(8))?;
        self.file.write_all(&(self.entries.len() as u32).to_be_bytes())?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut remaining = end;
        while remaining != 0 {
            let limit = remaining.min(self.buffer.len() as u64) as usize;
            let n = self.file.read(&mut self.buffer[..limit])?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            hash.update(&self.buffer[..n]);
            remaining -= n as u64;
        }
        let checksum: [u8; 32] = hash.finalize().into();
        self.file.write_all(&checksum)?;
        let index = index(&mut self.entries, &checksum);
        Ok((self.file, crate::hex(&checksum), index))
    }
}

fn index(entries: &mut [Entry], checksum: &[u8; 32]) -> Vec<u8> {
    entries.sort_by_key(|entry| entry.digest);
    let mut bytes = b"\xfftOc\0\0\0\x02".to_vec();
    let mut fanout = [0u32; 256];
    for entry in entries.iter() {
        fanout[usize::from(entry.digest[0])] += 1;
    }
    let mut total = 0;
    for count in fanout {
        total += count;
        bytes.extend_from_slice(&total.to_be_bytes());
    }
    for entry in entries.iter() {
        bytes.extend_from_slice(&entry.digest);
    }
    for entry in entries.iter() {
        bytes.extend_from_slice(&entry.crc.to_be_bytes());
    }
    let mut large = Vec::new();
    for entry in entries.iter() {
        let offset = if entry.offset < 0x8000_0000 {
            entry.offset as u32
        } else {
            let offset = 0x8000_0000 | large.len() as u32;
            large.push(entry.offset);
            offset
        };
        bytes.extend_from_slice(&offset.to_be_bytes());
    }
    for offset in large {
        bytes.extend_from_slice(&offset.to_be_bytes());
    }
    bytes.extend_from_slice(checksum);
    let hash = Sha256::digest(&bytes);
    bytes.extend_from_slice(&hash);
    bytes
}

/// Read one bounded undeltified blob at its recorded offset. A corrupt or truncated stream is never accepted.
pub(super) fn read_entry(source: &mut dyn Read, expected_bytes: u64) -> io::Result<Vec<u8>> {
    if expected_bytes > FILE_LIMIT {
        return Err(invalid());
    }
    let mut byte = [0u8; 1];
    source.read_exact(&mut byte)?;
    if byte[0] >> 4 & 7 != 3 {
        return Err(invalid());
    }
    let mut size = u64::from(byte[0] & 15);
    let mut shift = 4;
    while byte[0] & 128 != 0 {
        if shift >= 25 {
            return Err(invalid());
        }
        source.read_exact(&mut byte)?;
        size |= u64::from(byte[0] & 127) << shift;
        shift += 7;
    }
    if size != expected_bytes {
        return Err(invalid());
    }
    let mut decoder = Decompress::new(true);
    let mut input = vec![0u8; (expected_bytes + 64 * 1024).min(BUFFER as u64).max(64) as usize];
    let mut output = vec![0u8; (expected_bytes + 1).min(BUFFER as u64).max(1) as usize];
    let mut result = Vec::with_capacity(expected_bytes as usize);
    let compressed_limit = expected_bytes + 64 * 1024;
    let mut compressed_read = 0;
    let (mut available, mut offset, mut eof) = (0, 0, false);
    loop {
        if offset == available && !eof {
            let limit = (compressed_limit - compressed_read).min(input.len() as u64) as usize;
            available = source.read(&mut input[..limit])?;
            compressed_read += available as u64;
            offset = 0;
            eof = available == 0;
        }
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(&input[offset..available], &mut output, if eof { FlushDecompress::Finish } else { FlushDecompress::None })
            .map_err(io::Error::other)?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let written = (decoder.total_out() - before_out) as usize;
        if decoder.total_out() > expected_bytes || decoder.total_in() > compressed_limit {
            return Err(invalid());
        }
        result.extend_from_slice(&output[..written]);
        offset += consumed;
        if status == Status::StreamEnd {
            return if result.len() as u64 == expected_bytes { Ok(result) } else { Err(invalid()) };
        }
        if consumed == 0 && written == 0 && (eof || offset != available) {
            return Err(invalid());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn rewind_pack_and_index_match_the_sha256_git_format() {
        let mut batch = Batch::new(tempfile::tempfile().unwrap()).unwrap();
        let a = batch.add(&mut &b"first"[..], 5).unwrap();
        let b = batch.add(&mut &b"second"[..], 6).unwrap();
        let before_duplicate = batch.file().metadata().unwrap().len();
        assert_eq!(batch.add(&mut &b"first"[..], 5).unwrap().offset, a.offset);
        assert_eq!(batch.file().metadata().unwrap().len(), before_duplicate);
        assert_eq!(batch.count(), 2);
        let (mut file, oid, index) = batch.finish().unwrap();
        let mut packed = Vec::new();
        file.rewind().unwrap();
        file.read_to_end(&mut packed).unwrap();
        let end = packed.len() - 32;
        assert_eq!(&packed[..12], b"PACK\0\0\0\x02\0\0\0\x02");
        assert_eq!(&packed[end..], &Sha256::digest(&packed[..end])[..]);
        assert_eq!(oid, crate::hex(&packed[end..]));
        assert_eq!(read_entry(&mut Cursor::new(&packed[a.offset as usize..]), 5).unwrap(), b"first");
        assert_eq!(read_entry(&mut Cursor::new(&packed[b.offset as usize..]), 6).unwrap(), b"second");
        assert_eq!(a.crc, crc32fast::hash(&packed[a.offset as usize..b.offset as usize]));
        assert_eq!(b.crc, crc32fast::hash(&packed[b.offset as usize..end]));
        assert_eq!(&index[..8], b"\xfftOc\0\0\0\x02");
        for i in 0..256 {
            let count = [&a, &b].iter().filter(|entry| usize::from(entry.digest[0]) <= i).count() as u32;
            assert_eq!(&index[8 + i * 4..12 + i * 4], &count.to_be_bytes());
        }
        let mut sorted = [a, b];
        sorted.sort_by(|a, b| a.oid.cmp(&b.oid));
        for (i, entry) in sorted.iter().enumerate() {
            assert_eq!(&index[1032 + i * 32..1064 + i * 32], &entry.digest);
            assert_eq!(&index[1096 + i * 4..1100 + i * 4], &entry.crc.to_be_bytes());
            assert_eq!(&index[1104 + i * 4..1108 + i * 4], &(entry.offset as u32).to_be_bytes());
        }
        assert_eq!(&index[index.len() - 64..index.len() - 32], &packed[end..]);
        assert_eq!(&index[index.len() - 32..], &Sha256::digest(&index[..index.len() - 32])[..]);
    }

    #[test]
    fn rewind_pack_rejects_corrupt_sizes_and_truncated_compression() {
        let mut batch = Batch::new(tempfile::tempfile().unwrap()).unwrap();
        let entry = batch.add(&mut &b"contents"[..], 8).unwrap();
        let end = batch.file().metadata().unwrap().len();
        let (mut file, _, _) = batch.finish().unwrap();
        file.seek(SeekFrom::Start(entry.offset)).unwrap();
        let mut packed = vec![0; (end - entry.offset) as usize];
        file.read_exact(&mut packed).unwrap();
        assert!(read_entry(&mut Cursor::new(&packed), 7).is_err());
        assert!(read_entry(&mut Cursor::new(&packed[..packed.len() - 1]), 8).is_err());
        packed[0] = 0x28;
        assert!(read_entry(&mut Cursor::new(&packed), 8).is_err());
    }

    #[test]
    fn rewind_pack_reads_empty_and_buffered_compressed_blobs() {
        let mut compressed = ZlibEncoder::new(vec![0x30], Compression::fast());
        compressed.write_all(b"").unwrap();
        assert!(read_entry(&mut Cursor::new(compressed.finish().unwrap()), 0).unwrap().is_empty());
        let contents = vec![b'x'; 2 * BUFFER];
        let mut compressed = ZlibEncoder::new(vec![0xb0, 0x80, 0x80, 0x08], Compression::fast());
        compressed.write_all(&contents).unwrap();
        let packed = compressed.finish().unwrap();
        assert_eq!(read_entry(&mut Cursor::new(packed), contents.len() as u64).unwrap(), contents);
    }

    #[test]
    fn rewind_pack_rolls_back_failed_sources_before_accepting_the_next_file() {
        struct Broken(bool);
        impl Read for Broken {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                self.0 = true;
                out[0] = b'x';
                Ok(1)
            }
        }
        let mut batch = Batch::new(tempfile::tempfile().unwrap()).unwrap();
        batch.add(&mut &b"first"[..], 5).unwrap();
        let before = batch.file().metadata().unwrap().len();
        assert!(batch.add(&mut Broken(false), 8).is_err());
        assert_eq!(batch.file().metadata().unwrap().len(), before);
        let next = batch.add(&mut &b"second"[..], 6).unwrap();
        assert_eq!(next.offset, before);
        let (mut file, _, _) = batch.finish().unwrap();
        file.seek(SeekFrom::Start(8)).unwrap();
        let mut count = [0; 4];
        file.read_exact(&mut count).unwrap();
        assert_eq!(u32::from_be_bytes(count), 2);
    }

    #[test]
    fn rewind_pack_rolls_back_when_the_physical_payload_limit_is_exceeded() {
        let mut batch = Batch::new(tempfile::tempfile().unwrap()).unwrap();
        let before = PACK_LIMIT - 32;
        batch.file.set_len(before).unwrap();
        batch.file.seek(SeekFrom::Start(before)).unwrap();
        assert!(batch.add(&mut &b"x"[..], 1).is_err());
        assert_eq!(batch.file.metadata().unwrap().len(), before);
        assert_eq!(batch.file.stream_position().unwrap(), before);
        assert_eq!(batch.count(), 0);
    }
}
