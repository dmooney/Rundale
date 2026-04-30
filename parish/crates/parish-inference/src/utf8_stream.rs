//! Incremental UTF-8 decoder for byte streams from the network.
//!
//! HTTP chunk boundaries do not align with UTF-8 character boundaries, so
//! converting each raw chunk with [`String::from_utf8_lossy`] mangles any
//! multi-byte character that straddles two chunks (replacing it with
//! `U+FFFD`). In this game that includes Irish-language accents (`é`, `ó`,
//! `í`, `á`, `ú`), em dashes, smart quotes and NPC emoji reactions —
//! visible garbage in dialogue and, worse, corrupt JSON in the metadata
//! tail of an NPC stream response (issue #223).
//!
//! [`Utf8StreamDecoder`] accumulates bytes and emits only the longest
//! valid UTF-8 prefix on each call to [`Utf8StreamDecoder::push`],
//! retaining any incomplete trailing sequence for the next chunk. Bytes
//! that are genuinely invalid (not just partial) are replaced with the
//! Unicode replacement character `U+FFFD`, matching the conservative
//! behaviour of `from_utf8_lossy` on unambiguous errors.

/// Stateful decoder that converts a stream of raw byte chunks into
/// well-formed UTF-8 strings without splitting multi-byte sequences.
#[derive(Default)]
pub(crate) struct Utf8StreamDecoder {
    buf: Vec<u8>,
}

impl Utf8StreamDecoder {
    /// Creates a new empty decoder.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Appends `bytes` to the internal buffer and returns every complete
    /// UTF-8 sequence accumulated so far. Incomplete trailing bytes are
    /// retained for the next call. Genuinely invalid bytes are replaced
    /// with `U+FFFD`.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        self.buf.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.buf) {
                Ok(valid) => {
                    out.push_str(valid);
                    self.buf.clear();
                    return out;
                }
                Err(e) => {
                    let valid_up_to = e.valid_up_to();
                    // SAFETY: `valid_up_to()` is documented to return the
                    // length of the longest valid UTF-8 prefix, so slicing
                    // there yields definitely-valid UTF-8 bytes.
                    out.push_str(unsafe {
                        std::str::from_utf8_unchecked(&self.buf[..valid_up_to])
                    });
                    match e.error_len() {
                        None => {
                            // Incomplete trailing sequence — wait for more
                            // bytes in the next chunk.
                            self.buf.drain(..valid_up_to);
                            return out;
                        }
                        Some(invalid_len) => {
                            // Genuinely invalid bytes — emit a single
                            // replacement char and skip past them.
                            out.push('\u{FFFD}');
                            self.buf.drain(..valid_up_to + invalid_len);
                            // Loop to process any remaining bytes.
                        }
                    }
                }
            }
        }
    }

    /// Flushes any bytes still buffered after the stream has ended. An
    /// incomplete trailing sequence is converted lossily (each invalid
    /// byte becomes `U+FFFD`), matching `from_utf8_lossy`'s behaviour.
    pub(crate) fn flush(&mut self) -> String {
        if self.buf.is_empty() {
            return String::new();
        }
        let s = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_passes_through_unchanged() {
        let mut d = Utf8StreamDecoder::new();
        assert_eq!(d.push(b"hello world"), "hello world");
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn two_byte_char_split_across_chunks_is_reassembled() {
        // 'é' is 0xC3 0xA9 in UTF-8.
        let mut d = Utf8StreamDecoder::new();
        assert_eq!(d.push(b"caf\xC3"), "caf"); // partial — no 'é' yet
        assert_eq!(d.push(b"\xA9"), "é"); // completes the sequence
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn three_byte_char_split_across_three_chunks() {
        // '—' (em dash U+2014) is 0xE2 0x80 0x94 in UTF-8.
        let mut d = Utf8StreamDecoder::new();
        assert_eq!(d.push(b"oh \xE2"), "oh ");
        assert_eq!(d.push(b"\x80"), "");
        assert_eq!(d.push(b"\x94 yes"), "— yes");
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn four_byte_emoji_split_across_chunks() {
        // '🙂' (U+1F642) is 0xF0 0x9F 0x99 0x82 in UTF-8.
        let mut d = Utf8StreamDecoder::new();
        assert_eq!(d.push(b"smile \xF0\x9F"), "smile ");
        assert_eq!(d.push(b"\x99\x82!"), "🙂!");
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn multiple_multibyte_chars_in_single_chunk() {
        let mut d = Utf8StreamDecoder::new();
        assert_eq!(d.push("Siobhán — faith".as_bytes()), "Siobhán — faith");
    }

    #[test]
    fn invalid_bytes_become_replacement_char() {
        let mut d = Utf8StreamDecoder::new();
        // 0xFF is never valid as the first byte of a UTF-8 sequence.
        let out = d.push(b"a\xFFb");
        assert_eq!(out, "a\u{FFFD}b");
    }

    #[test]
    fn flush_reports_incomplete_trailing_as_replacement_char() {
        let mut d = Utf8StreamDecoder::new();
        // Feed only the first byte of a two-byte sequence, then end the stream.
        assert_eq!(d.push(b"good \xC3"), "good ");
        assert_eq!(d.flush(), "\u{FFFD}");
        // Decoder is now clean.
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn byte_by_byte_feed_reassembles_correctly() {
        // Feed each byte of "café" individually.
        let bytes: &[u8] = "café".as_bytes();
        let mut d = Utf8StreamDecoder::new();
        let mut combined = String::new();
        for b in bytes {
            combined.push_str(&d.push(&[*b]));
        }
        combined.push_str(&d.flush());
        assert_eq!(combined, "café");
    }

    #[test]
    fn json_with_accent_is_preserved_across_chunk_boundary() {
        // Regression for #223: the metadata tail of an NPC response would
        // fail JSON parsing if a multi-byte char inside it got mangled.
        let payload = r#"{"speaker":"Siobh\u00e1n","line":"Dia dhuit — fáilte"}"#.as_bytes();
        // Arbitrary split point chosen to land mid-sequence for the em dash
        // (0xE2 0x80 0x94) inside the JSON string.
        let dash_byte_idx = payload
            .windows(3)
            .position(|w| w == [0xE2, 0x80, 0x94])
            .unwrap();
        // Split between the first and second byte of the em dash.
        let (a, b) = payload.split_at(dash_byte_idx + 1);
        let mut d = Utf8StreamDecoder::new();
        let mut combined = String::new();
        combined.push_str(&d.push(a));
        combined.push_str(&d.push(b));
        combined.push_str(&d.flush());
        assert_eq!(
            combined,
            r#"{"speaker":"Siobh\u00e1n","line":"Dia dhuit — fáilte"}"#
        );
    }
}
