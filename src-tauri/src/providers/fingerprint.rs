use crate::error::Result;
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct FileHashes {
    pub sha1: String,
    pub curseforge: u32,
}

const MULTIPLEX: u32 = 0x5bd1_e995;

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 13 | 32)
}

#[cfg(test)]
pub(crate) fn curseforge_fingerprint(bytes: &[u8]) -> u32 {
    let filtered: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|byte| !is_whitespace(*byte))
        .collect();
    let mut hash = 1_u32 ^ filtered.len() as u32;
    for chunk in filtered.chunks_exact(4) {
        let mut word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        word = word.wrapping_mul(MULTIPLEX);
        word ^= word >> 24;
        word = word.wrapping_mul(MULTIPLEX);
        hash = hash.wrapping_mul(MULTIPLEX) ^ word;
    }
    let remainder = filtered.chunks_exact(4).remainder();
    if !remainder.is_empty() {
        let mut word = 0_u32;
        for (index, byte) in remainder.iter().enumerate() {
            word |= (*byte as u32) << (8 * index);
        }
        hash = (hash ^ word).wrapping_mul(MULTIPLEX);
    }
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(MULTIPLEX);
    hash ^ (hash >> 15)
}

pub(crate) fn file_hashes(path: &Path) -> Result<FileHashes> {
    let mut file = File::open(path)?;
    let mut sha = Sha1::new();
    let mut length = 0_u32;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha.update(&buffer[..read]);
        length = length.saturating_add(
            buffer[..read]
                .iter()
                .filter(|byte| !is_whitespace(**byte))
                .count() as u32,
        );
    }
    file.seek(SeekFrom::Start(0))?;
    let mut hash = 1_u32 ^ length;
    let mut word = 0_u32;
    let mut shift = 0_u32;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            if is_whitespace(*byte) {
                continue;
            }
            word |= (*byte as u32) << shift;
            shift += 8;
            if shift == 32 {
                let mut block = word.wrapping_mul(MULTIPLEX);
                block ^= block >> 24;
                block = block.wrapping_mul(MULTIPLEX);
                hash = hash.wrapping_mul(MULTIPLEX) ^ block;
                word = 0;
                shift = 0;
            }
        }
    }
    if shift > 0 {
        hash = (hash ^ word).wrapping_mul(MULTIPLEX);
    }
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(MULTIPLEX);
    hash ^= hash >> 15;
    Ok(FileHashes {
        sha1: hex::encode(sha.finalize()),
        curseforge: hash,
    })
}

#[cfg(test)]
mod tests {
    use super::{curseforge_fingerprint, file_hashes};
    use std::fs;

    #[test]
    fn matches_curseforge_empty_vector() {
        assert_eq!(curseforge_fingerprint(&[]), 0x5bd1_5e36);
    }

    #[test]
    fn strips_curseforge_whitespace() {
        assert_eq!(
            curseforge_fingerprint(b"abc"),
            curseforge_fingerprint(b" a\tb\nc\r ")
        );
    }

    #[test]
    fn hashes_large_file_across_buffer_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.jar");
        let content: Vec<u8> = (0..100_000_u32).map(|index| (index % 251) as u8).collect();
        fs::write(&path, &content).unwrap();
        let hashes = file_hashes(&path).unwrap();
        assert_eq!(hashes.curseforge, curseforge_fingerprint(&content));
    }

    #[test]
    fn hashes_file_with_sha1_and_fingerprint() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mod.jar");
        let content = b"abcdefgh\x00\x09ijklmnop";
        fs::write(&path, content).unwrap();
        let hashes = file_hashes(&path).unwrap();
        assert_eq!(hashes.sha1, {
            use sha1::{Digest, Sha1};
            let mut sha = Sha1::new();
            sha.update(content);
            hex::encode(sha.finalize())
        });
        assert_eq!(hashes.curseforge, curseforge_fingerprint(content));
    }
}
