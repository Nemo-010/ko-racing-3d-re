//! Reader for the game's resource archive (`data` index + `data.<n>` pages).
//!
//! Layout (big endian):
//!
//! ```text
//! u16   resource_count
//! i32   page_size
//! repeat resource_count:
//!     u8    name_len
//!     bytes name[name_len]
//!     i32   offset      # absolute offset in the virtual page concatenation
//! ```
//!
//! A resource lives in page `offset / page_size` at byte `offset % page_size`
//! and ends where the next resource of the same page starts (or at EOF).
//! The MIDlet itself never learns resource lengths; it seeks and lets each
//! format parser read as far as it needs.  The length is recovered here from
//! the next entry, which is what makes whole-archive loading possible.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub type Resources = HashMap<String, Vec<u8>>;

pub fn load(dir: &Path) -> Resources {
    let index = fs::read(dir.join("data")).unwrap_or_else(|e| {
        panic!("cannot read {}: {e}\nCopy the pack's data/data.* files next to the binary.", dir.join("data").display())
    });
    let count = u16::from_be_bytes([index[0], index[1]]) as usize;
    let page_size =
        i32::from_be_bytes([index[2], index[3], index[4], index[5]]).max(1) as usize;

    let mut entries: Vec<(String, usize)> = Vec::with_capacity(count);
    let mut pos = 6usize;
    for _ in 0..count {
        let len = index[pos] as usize;
        pos += 1;
        let name = String::from_utf8_lossy(&index[pos..pos + len]).into_owned();
        pos += len;
        let offset =
            i32::from_be_bytes([index[pos], index[pos + 1], index[pos + 2], index[pos + 3]])
                as usize;
        pos += 4;
        entries.push((name, offset));
    }

    let mut by_page: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, (_, offset)) in entries.iter().enumerate() {
        by_page.entry(offset / page_size).or_default().push(i);
    }

    let mut resources = Resources::new();
    for (page, mut indices) in by_page {
        let blob = fs::read(dir.join(format!("data.{page}"))).unwrap_or_else(|e| {
            panic!("cannot read pack page data.{page}: {e}");
        });
        indices.sort_by_key(|&i| entries[i].1);
        for k in 0..indices.len() {
            let i = indices[k];
            let start = entries[i].1 % page_size;
            let end = match indices.get(k + 1) {
                Some(&next) => entries[next].1 % page_size,
                None => blob.len(),
            };
            resources.insert(entries[i].0.clone(), blob[start..end].to_vec());
        }
    }
    resources
}

/// A loose file shipped in the JAR next to the pack (`lists/*`).
pub fn read_jar_file(dir: &Path, name: &str) -> Vec<u8> {
    fs::read(dir.join(name)).unwrap_or_else(|e| panic!("cannot read {name}: {e}"))
}

/// Split a newline-separated list, dropping the CR of CRLF files.
pub fn lines(data: &[u8]) -> Vec<String> {
    data.split(|&b| b == b'\n')
        .map(|line| {
            let line = match line.last() {
                Some(b'\r') => &line[..line.len() - 1],
                _ => line,
            };
            String::from_utf8_lossy(line).into_owned()
        })
        .filter(|line| !line.is_empty())
        .collect()
}
