use std::{iter::Peekable, ops::Range};

use lapce_xi_rope::{DeltaBuilder, Rope, RopeDelta};
use memchr::{memchr, memchr2};
use std::sync::LazyLock;

// Cached ropes for the two line endings
static CR_LF: LazyLock<Rope> = LazyLock::new(|| Rope::from("\r\n"));
static LF: LazyLock<Rope> = LazyLock::new(|| Rope::from("\n"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineEnding {
    /// `\r\n` Windows
    CrLf,
    /// `\n` Unix
    Lf,
}
impl LineEnding {
    /// Replace the line endings (`\n`, `\r\n`, `\r`) used in `text` with the line ending named by
    /// `self`.
    pub fn normalize(self, text: &Rope) -> Rope {
        self.normalize_delta(text)
            .map(|d| d.apply(text))
            .unwrap_or_else(|| text.clone())
    }

    pub fn normalize_delta(self, text: &Rope) -> Option<RopeDelta> {
        let mut builder = DeltaBuilder::new(text.len());

        let le = if self == LineEnding::Lf {
            LF.clone()
        } else {
            CR_LF.clone()
        };

        let mut had_entries = false;
        for (range, kind) in FullLeChunkSearch::new(text.iter_chunks(..)) {
            had_entries = true;
            match kind {
                LeChunkKind::CrLf => {
                    if self == LineEnding::Lf {
                        builder.replace(range, LF.clone());
                    }
                }
                LeChunkKind::Lf => {
                    if self == LineEnding::CrLf {
                        builder.replace(range, CR_LF.clone());
                    }
                }
                LeChunkKind::Cr => {
                    builder.replace(range, le.clone());
                }
            }
        }

        if had_entries {
            let delta = builder.build();
            Some(delta)
        } else {
            None
        }
    }

    /// Only replace the carriage return line-endings.
    pub fn normalize_limited(self, text: &Rope) -> Rope {
        let mut builder = DeltaBuilder::new(text.len());

        let le = if self == LineEnding::Lf {
            LF.clone()
        } else {
            CR_LF.clone()
        };

        let mut had_entries = false;
        for offset in LoneCrChunkSearch::new(text.iter_chunks(..)) {
            had_entries = true;
            builder.replace(offset..offset + 1, le.clone());
        }

        if had_entries {
            let delta = builder.build();
            delta.apply(text)
        } else {
            text.clone()
        }
    }

    pub fn get_chars(&self) -> &'static str {
        match self {
            LineEnding::CrLf => "\r\n",
            LineEnding::Lf => "\n",
        }
    }

    /// Get the name of the line ending
    pub fn as_str(&self) -> &'static str {
        match self {
            LineEnding::CrLf => "CRLF",
            LineEnding::Lf => "LF",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LineEndingDetermination {
    CrLf,
    Lf,
    Mixed,
    Unknown,
}
impl LineEndingDetermination {
    // TODO: should we just do a simpler routine of checking the first few lines?
    // Based off of xi-rope's line-ending determination logic
    pub fn determine(text: &Rope) -> Self {
        let mut crlf = false;
        let mut lf = false;

        for chunk in text.iter_chunks(..) {
            match LineEndingDetermination::determine_str(chunk) {
                LineEndingDetermination::CrLf => crlf = true,
                LineEndingDetermination::Lf => lf = true,
                LineEndingDetermination::Mixed => {
                    return LineEndingDetermination::Mixed;
                }
                LineEndingDetermination::Unknown => {}
            }
        }

        match (crlf, lf) {
            (true, true) => LineEndingDetermination::Mixed,
            (true, false) => LineEndingDetermination::CrLf,
            (false, true) => LineEndingDetermination::Lf,
            (false, false) => LineEndingDetermination::Unknown,
        }
    }

    fn determine_str(chunk: &str) -> LineEndingDetermination {
        let bytes = chunk.as_bytes();
        let newline = memchr2(b'\n', b'\r', bytes);
        match newline {
            Some(x) if bytes[x] == b'\r' && bytes.len() > x + 1 && bytes[x + 1] == b'\n' => {
                LineEndingDetermination::CrLf
            }
            Some(x) if bytes[x] == b'\n' => LineEndingDetermination::Lf,
            Some(_) => LineEndingDetermination::Mixed,
            None => LineEndingDetermination::Unknown,
        }
    }

    pub fn unwrap_or(self, le: LineEnding) -> LineEnding {
        match self {
            LineEndingDetermination::CrLf => LineEnding::CrLf,
            LineEndingDetermination::Lf => LineEnding::Lf,
            LineEndingDetermination::Mixed | LineEndingDetermination::Unknown => le,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LeChunkKind {
    CrLf,
    Lf,
    Cr,
}

/// Line ending chunk searcher
struct FullLeChunkSearch<'a, I: Iterator<Item = &'a str>> {
    offset: usize,
    /// Offset within the chunk itself
    chunk_pos: usize,
    chunks: Peekable<I>,
}
impl<'a, I: Iterator<Item = &'a str>> FullLeChunkSearch<'a, I> {
    fn new(chunks: I) -> Self {
        Self {
            offset: 0,
            chunk_pos: 0,
            chunks: chunks.peekable(),
        }
    }

    /// Get the current chunk, updating the current chunk if needed
    fn get_chunk(&mut self) -> Option<&'a str> {
        let chunk = self.chunks.peek()?;
        if self.chunk_pos >= chunk.len() {
            self.advance_chunk();
            Some(*self.chunks.peek()?)
        } else {
            Some(chunk)
        }
    }

    fn advance_chunk(&mut self) -> Option<()> {
        let chunk = self.chunks.next()?;
        self.offset += chunk.len();
        self.chunk_pos = 0;

        Some(())
    }
}
impl<'a, I: Iterator<Item = &'a str>> Iterator for FullLeChunkSearch<'a, I> {
    type Item = (Range<usize>, LeChunkKind);

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.get_chunk()?;

        let bytes = &chunk.as_bytes()[self.chunk_pos..];

        let newline = memchr2(b'\n', b'\r', bytes);
        match newline {
            // CrLf
            Some(x) if bytes[x] == b'\r' && bytes.len() > x + 1 && bytes[x + 1] == b'\n' => {
                let start = self.offset + self.chunk_pos + x;
                let end = start + 2;

                self.chunk_pos += x + 2;
                Some((start..end, LeChunkKind::CrLf))
            }
            // Lf
            Some(x) if bytes[x] == b'\n' => {
                let start = self.offset + self.chunk_pos + x;
                let end = start + 1;

                self.chunk_pos += x + 1;
                Some((start..end, LeChunkKind::Lf))
            }
            Some(x) => {
                // Typically this only occurs for a lone `\r`.
                // However, we need to handle the case where the `\r` is the last character in the
                // chunk whilst the next chunk starts with a `\n`.
                assert_eq!(bytes[x], b'\r');

                let start = self.offset + self.chunk_pos + x;
                self.chunk_pos += x + 1;

                let v = if self.chunk_pos == chunk.len() {
                    if let Some(next_chunk) = self.get_chunk() {
                        let next_chunk = &next_chunk.as_bytes()[self.chunk_pos..];
                        if next_chunk.starts_with(b"\n") {
                            self.chunk_pos += 1;
                            Some((start..start + 2, LeChunkKind::CrLf))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                Some(v.unwrap_or_else(|| {
                    // There is no \n so it is a lone `\r`
                    // (Which is used in MacOS, or sometimes due to bugged line endings)
                    let end = start + 1;
                    (start..end, LeChunkKind::Cr)
                }))
            }
            None => {
                self.advance_chunk();
                self.next()
            }
        }
    }
}

/// Iterator that searches for lone carriage returns ('\r') in chunks of text.
struct LoneCrChunkSearch<'a, I: Iterator<Item = &'a str>> {
    /// Offset of the start of the current chunk
    offset: usize,
    chunk_pos: usize,
    chunks: Peekable<I>,
}

impl<'a, I: Iterator<Item = &'a str>> LoneCrChunkSearch<'a, I> {
    fn new(chunks: I) -> Self {
        Self {
            offset: 0,
            chunk_pos: 0,
            chunks: chunks.peekable(),
        }
    }

    /// Get the current chunk, or if chunk pos is past the end of the chunk, then
    /// advance to the next chunk and get it.
    fn get_chunk(&mut self) -> Option<&'a str> {
        let chunk = self.chunks.peek()?;
        if self.chunk_pos >= chunk.len() {
            self.advance_chunk();
            Some(*self.chunks.peek()?)
        } else {
            Some(chunk)
        }
    }

    fn advance_chunk(&mut self) -> Option<()> {
        let chunk = self.chunks.next()?;
        self.offset += chunk.len();
        self.chunk_pos = 0;

        Some(())
    }
}

impl<'a, I: Iterator<Item = &'a str>> Iterator for LoneCrChunkSearch<'a, I> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let chunk = self.get_chunk()?;

            let bytes = &chunk.as_bytes()[self.chunk_pos..];

            let newline = memchr(b'\r', bytes);
            match newline {
                Some(x) => {
                    let offset = self.offset + self.chunk_pos + x;

                    // Check if the next character is '\n' (indicating \r\n)
                    self.chunk_pos += x + 1;
                    if self.chunk_pos < chunk.len() && chunk.as_bytes()[self.chunk_pos] == b'\n' {
                        // Skip \r\n sequences
                        self.chunk_pos += 1;
                    } else if let Some(chunk_b) = self.get_chunk() {
                        let chunk_b = &chunk_b.as_bytes()[self.chunk_pos..];
                        if chunk_b.starts_with(b"\n") {
                            // Skip \r\n sequences across chunks
                            self.chunk_pos += 1;
                        } else {
                            // Lone \r
                            return Some(offset);
                        }
                    } else {
                        // Lone \r at the end
                        return Some(offset);
                    }
                }
                None => {
                    self.advance_chunk();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        let text = Rope::from("hello\r\nworld toast and jam\nthe end\nhi");
        let normalized = LineEnding::CrLf.normalize(&text);
        assert_eq!(
            normalized.slice_to_cow(..),
            "hello\r\nworld toast and jam\r\nthe end\r\nhi"
        );

        let text = Rope::from("\n");
        let normalized = LineEnding::Lf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\n");
        let normalized = LineEnding::CrLf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\r\n");

        let text = Rope::from("\r\n");
        let normalized = LineEnding::Lf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\n");
        let normalized = LineEnding::CrLf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\r\n");

        // `\r` is always normalized to the line ending of the file
        let text = Rope::from("\r");
        let normalized = LineEnding::Lf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\n");
        let normalized = LineEnding::CrLf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\r\n");
        let normalized = LineEnding::Lf.normalize_limited(&text);
        assert_eq!(normalized.slice_to_cow(..), "\n");

        let text = Rope::from("\rtest");
        let normalized = LineEnding::Lf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\ntest");
        let normalized = LineEnding::CrLf.normalize(&text);
        assert_eq!(normalized.slice_to_cow(..), "\r\ntest");
        let normalized = LineEnding::Lf.normalize_limited(&text);
        assert_eq!(normalized.slice_to_cow(..), "\ntest");
    }

    #[test]
    fn chunk_search() {
        let text = Rope::from("hello\r\nworld toast and jam\nthe end\nhi");
        let c = FullLeChunkSearch::new(text.iter_chunks(..));
        assert_eq!(
            c.collect::<Vec<_>>(),
            vec![
                (5..7, LeChunkKind::CrLf),
                (26..27, LeChunkKind::Lf),
                (34..35, LeChunkKind::Lf),
            ]
        );
        let c = LoneCrChunkSearch::new(text.iter_chunks(..));
        assert_eq!(c.collect::<Vec<_>>(), Vec::new());

        // Test searching across different chunks of text
        // (Using a non-Rope iterator to simplify creation, however it should behave the same)
        let text = ["a\n", "\n5", "\r\ne\r", "\ntest\r", "\rv"];
        let multi_chunk = FullLeChunkSearch::new(text.into_iter());
        assert_eq!(
            multi_chunk.collect::<Vec<_>>(),
            vec![
                (1..2, LeChunkKind::Lf),
                (2..3, LeChunkKind::Lf),
                (4..6, LeChunkKind::CrLf),
                (7..9, LeChunkKind::CrLf),
                (13..14, LeChunkKind::Cr),
                (14..15, LeChunkKind::Cr),
            ]
        );

        let multi_chunk = LoneCrChunkSearch::new(text.into_iter());
        assert_eq!(multi_chunk.collect::<Vec<_>>(), vec![13, 14]);

        let text = ["\n\rb"];
        let chunks = FullLeChunkSearch::new(text.into_iter());
        assert_eq!(
            chunks.collect::<Vec<_>>(),
            vec![(0..1, LeChunkKind::Lf), (1..2, LeChunkKind::Cr)]
        );

        let text = ["\n\rb"];
        let chunks = LoneCrChunkSearch::new(text.into_iter());
        assert_eq!(chunks.collect::<Vec<_>>(), vec![1]);
    }
}
