//! A local file read ahead of the decoder by a thread of its own, so a
//! busy disk stalls that thread instead of the sound.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use symphonia::core::io::MediaSource;

/// The most the thread reads in one go.
const CHUNK: usize = 256 * 1024;

const MIB: usize = 1024 * 1024;

/// The file, as the decoder reads it.
pub struct ReadAhead {
    shared: Arc<Shared>,
    len: u64,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

struct State {
    /// The bytes read and not yet taken, from `at` on.
    buf: VecDeque<u8>,
    /// Where the decoder is in the file.
    at: u64,
    /// Bumped by every seek outside `buf`, so a read begun before it is thrown away.
    generation: u64,
    end: End,
    /// The decoder is gone: the thread stops.
    closed: bool,
}

/// Why the thread reads no further.
enum End {
    Open,
    Eof,
    Failed(io::ErrorKind, String),
}

impl ReadAhead {
    /// Opens `path` and starts reading up to `megabytes` ahead.
    pub fn open(path: &Path, megabytes: NonZeroU32) -> io::Result<ReadAhead> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let capacity = megabytes.get() as usize * MIB;
        let state = State { buf: VecDeque::new(), at: 0, generation: 0, end: End::Open, closed: false };
        let shared = Arc::new(Shared { state: Mutex::new(state), changed: Condvar::new() });
        let worker = shared.clone();
        std::thread::Builder::new().name("popamp-readahead".into()).spawn(move || fill(&worker, file, capacity))?;
        Ok(ReadAhead { shared, len })
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn wait<'a>(&self, guard: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed.wait(guard).unwrap_or_else(|e| e.into_inner())
    }
}

/// The thread: keeps `capacity` bytes ahead of the decoder until it is closed.
fn fill(shared: &Shared, mut file: File, capacity: usize) {
    let mut chunk = vec![0; CHUNK];
    let mut file_at = 0u64;
    while let Some((from, generation, room)) = next_read(shared, capacity) {
        let read = read_at(&mut file, &mut file_at, from, &mut chunk[..room.min(CHUNK)]);
        let mut st = shared.lock();
        if st.generation != generation {
            continue;
        }
        match read {
            Ok(0) => st.end = End::Eof,
            Ok(n) => st.buf.extend(&chunk[..n]),
            Err(e) => st.end = End::Failed(e.kind(), e.to_string()),
        }
        shared.changed.notify_all();
    }
}

/// Waits for room to read into: where to read from, the generation it is
/// for and how much fits; none once the decoder is gone.
fn next_read(shared: &Shared, capacity: usize) -> Option<(u64, u64, usize)> {
    let mut st = shared.lock();
    while !st.closed && !(matches!(st.end, End::Open) && st.buf.len() < capacity) {
        st = shared.wait(st);
    }
    (!st.closed).then(|| (st.at + st.buf.len() as u64, st.generation, capacity - st.buf.len()))
}

fn read_at(file: &mut File, file_at: &mut u64, from: u64, into: &mut [u8]) -> io::Result<usize> {
    if *file_at != from {
        file.seek(SeekFrom::Start(from))?;
        *file_at = from;
    }
    let n = loop {
        match file.read(into) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            other => break other?,
        }
    };
    *file_at += n as u64;
    Ok(n)
}

impl Read for ReadAhead {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let mut st = self.shared.lock();
        loop {
            if !st.buf.is_empty() {
                let n = st.buf.read(out)?;
                st.at += n as u64;
                self.shared.changed.notify_all();
                return Ok(n);
            }
            match &st.end {
                End::Open => st = self.shared.wait(st),
                End::Eof => return Ok(0),
                End::Failed(kind, text) => return Err(io::Error::new(*kind, text.clone())),
            }
        }
    }
}

impl Seek for ReadAhead {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let mut st = self.shared.lock();
        let target = match pos {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(d) => st.at.checked_add_signed(d),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start of the file"))?;
        let ahead = target.checked_sub(st.at).filter(|&d| d <= st.buf.len() as u64);
        match ahead {
            Some(skip) => drop(st.buf.drain(..skip as usize)),
            None => {
                st.buf.clear();
                st.generation += 1;
                st.end = End::Open;
            }
        }
        st.at = target;
        self.shared.changed.notify_all();
        Ok(target)
    }
}

impl MediaSource for ReadAhead {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

impl Drop for ReadAhead {
    fn drop(&mut self) {
        self.shared.lock().closed = true;
        self.shared.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of `len` bytes, removed when dropped.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn file_of(len: usize) -> (Scratch, std::path::PathBuf, Vec<u8>) {
        let path = std::env::temp_dir().join(format!("popamp-readahead-{}-{len}", std::process::id()));
        let bytes: Vec<u8> = (0..len).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();
        (Scratch(path.clone()), path, bytes)
    }

    fn one_mib() -> NonZeroU32 {
        NonZeroU32::new(1).unwrap()
    }

    #[test]
    fn reads_a_file_longer_than_the_buffer_whole() {
        let (_dir, path, bytes) = file_of(3 * MIB + 12_345);
        let mut got = Vec::new();
        ReadAhead::open(&path, one_mib()).unwrap().read_to_end(&mut got).unwrap();
        assert_eq!(got, bytes);
    }

    #[test]
    fn seeks_inside_and_outside_what_is_read() {
        let (_dir, path, bytes) = file_of(3 * MIB);
        let mut r = ReadAhead::open(&path, one_mib()).unwrap();
        let mut two = [0u8; 2];
        let mut check = |r: &mut ReadAhead, pos: SeekFrom, at: usize| {
            assert_eq!(r.seek(pos).unwrap(), at as u64);
            r.read_exact(&mut two).unwrap();
            assert_eq!(two, bytes[at..at + 2], "at {at}");
        };
        check(&mut r, SeekFrom::Start(10), 10);
        check(&mut r, SeekFrom::Current(100), 112);
        check(&mut r, SeekFrom::Start(2 * MIB as u64 + 5), 2 * MIB + 5);
        check(&mut r, SeekFrom::Start(7), 7);
        check(&mut r, SeekFrom::End(-2), 3 * MIB - 2);
        assert_eq!(r.read(&mut two).unwrap(), 0);
        assert!(r.seek(SeekFrom::Current(-(4 * MIB as i64))).is_err());
    }

    #[test]
    fn tells_its_length() {
        let (_dir, path, _) = file_of(1000);
        assert_eq!(ReadAhead::open(&path, one_mib()).unwrap().byte_len(), Some(1000));
    }
}
