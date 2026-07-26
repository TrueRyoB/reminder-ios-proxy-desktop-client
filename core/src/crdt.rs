//! Apple's "topotext" CRDT document format, used to encode Reminders'
//! title/description text (CloudKit `TitleDocument`/`NotesDocument`
//! fields). Ported from pyicloud's `_protocol.py`, which itself extracted
//! the wire format from iCloud.com Reminders' own main.js.
//!
//! Every create/update of a reminder's title or description must go
//! through this encoding -- there is no plain-string fallback.

pub mod topotext {
    include!(concat!(env!("OUT_DIR"), "/topotext.rs"));
}
pub mod versioned_document {
    include!(concat!(env!("OUT_DIR"), "/versioned_document.rs"));
}

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use prost::Message;
use thiserror::Error;

/// Fixed replica identity pyicloud/Apple's JS client uses when minting a
/// brand new document. Not meaningful cryptographically -- just needs to be
/// *a* stable 16-byte value for the vector clock.
const REPLICA_UUID_HEX: &str = "d46bcae41b8766c18d75efe35c9145c3";
const CLOCK_MAX: u32 = 0xFFFF_FFFF;

#[derive(Debug, Error)]
pub enum CrdtError {
    #[error("invalid base64 in CRDT document")]
    InvalidBase64,
    #[error("unable to decode CRDT document (no known shape matched)")]
    UnableToDecode,
}

fn zlib_compress(data: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("in-memory write cannot fail");
    encoder.finish().expect("in-memory finish cannot fail")
}

fn zlib_decompress(data: &[u8]) -> std::io::Result<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

fn gzip_decompress(data: &[u8]) -> std::io::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// Build a fresh topotext CRDT document for `text` and return it
/// base64-encoded, ready to store in a Title/Notes CloudKit field.
pub fn encode_crdt_document(text: &str) -> String {
    // Matches pyicloud's `len(text)`: a count of Unicode scalar values, not
    // UTF-16 code units as the .proto comment claims. Kept bit-identical to
    // the reference implementation (and therefore to what Apple's own
    // servers currently accept from it) rather than "corrected".
    let text_length = text.chars().count() as u32;
    let replica_uuid = hex::decode(REPLICA_UUID_HEX).expect("valid hex constant");

    let mut substrings = vec![topotext::Substring {
        char_id: Some(topotext::CharId {
            replica_id: Some(0),
            clock: Some(0),
        }),
        length: Some(0),
        timestamp: Some(topotext::CharId {
            replica_id: Some(0),
            clock: Some(0),
        }),
        tombstone: None,
        child: vec![1],
    }];

    if text_length > 0 {
        substrings.push(topotext::Substring {
            char_id: Some(topotext::CharId {
                replica_id: Some(1),
                clock: Some(0),
            }),
            length: Some(text_length),
            timestamp: Some(topotext::CharId {
                replica_id: Some(1),
                clock: Some(0),
            }),
            tombstone: None,
            child: vec![2],
        });
    }

    substrings.push(topotext::Substring {
        char_id: Some(topotext::CharId {
            replica_id: Some(0),
            clock: Some(CLOCK_MAX),
        }),
        length: Some(0),
        timestamp: Some(topotext::CharId {
            replica_id: Some(0),
            clock: Some(CLOCK_MAX),
        }),
        tombstone: None,
        child: vec![],
    });

    let clock = topotext::vector_timestamp::Clock {
        replica_uuid: Some(replica_uuid),
        replica_clock: vec![
            topotext::vector_timestamp::clock::ReplicaClock {
                clock: Some(text_length),
                subclock: None,
            },
            topotext::vector_timestamp::clock::ReplicaClock {
                clock: Some(1),
                subclock: None,
            },
        ],
    };

    let mut attribute_run = Vec::new();
    if text_length > 0 {
        attribute_run.push(topotext::AttributeRun {
            length: Some(text_length),
            ..Default::default()
        });
    }

    let value = topotext::String {
        string: Some(text.to_string()),
        substring: substrings,
        timestamp: Some(topotext::VectorTimestamp { clock: vec![clock] }),
        attribute_run,
        attachment: vec![],
    };

    let string_bytes = value.encode_to_vec();

    let version = versioned_document::Version {
        serialization_version: Some(0),
        minimum_supported_version: Some(0),
        data: Some(string_bytes),
    };

    let document = versioned_document::Document {
        serialization_version: Some(0),
        version: vec![version],
    };

    let compressed = zlib_compress(&document.encode_to_vec());
    B64.encode(compressed)
}

/// Decode a Title/Notes CloudKit field value back into plain text.
/// Accepts the current `Document{version:[Version{data:..}]}` shape as well
/// as two legacy fallback shapes pyicloud also handles.
pub fn decode_crdt_document(encoded: &str) -> Result<String, CrdtError> {
    let mut padded = encoded.to_string();
    let rem = padded.len() % 4;
    if rem != 0 {
        padded.push_str(&"=".repeat(4 - rem));
    }
    let compressed = B64.decode(&padded).map_err(|_| CrdtError::InvalidBase64)?;

    let data = zlib_decompress(&compressed)
        .or_else(|_| gzip_decompress(&compressed))
        .unwrap_or(compressed);

    if let Ok(document) = versioned_document::Document::decode(data.as_slice())
        && let Some(version) = document.version.first()
        && let Some(bytes) = &version.data
        && let Ok(value) = topotext::String::decode(bytes.as_slice())
    {
        return Ok(value.string.unwrap_or_default());
    }

    if let Ok(version) = versioned_document::Version::decode(data.as_slice())
        && let Some(bytes) = &version.data
        && let Ok(value) = topotext::String::decode(bytes.as_slice())
    {
        return Ok(value.string.unwrap_or_default());
    }

    if let Ok(value) = topotext::String::decode(data.as_slice())
        && let Some(s) = value.string
        && !s.is_empty()
    {
        return Ok(s);
    }

    Err(CrdtError::UnableToDecode)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vectors generated by running the real pyicloud `_encode_crdt_document`
    /// / `_decode_crdt_document` (same protobuf schema, same wire format).
    const VECTORS: [(&str, &str); 4] = [
        ("", "eJzjYBAK4GAQYJDyEmKQEuBiAbGBPDCtwSglxsUBZP0HAn6gKJytJMMlxSVwJfvUE+n2tIO9pe8fx0x0PSzExMEAxIwAhRkUpg=="),
        ("a", "eJzjYBBK52AQYJBKFGJMlBLgYgFxgFwwrcEIFmEEijBKgWkNJikxLg6g3H8g4Aeqg7OVZLikuASuZJ96It2edrC39P3jmImuh4WYOBhBWAuIAWJ9Fi8="),
        ("hello", "eJzjYBDK5mAQYJBKFWLNSM3JyZcS4GIBCQCFwLQGI1iEESjCKgWmNZikxLg4gHL/gYAfqA7OVpLhkuISuJJ96ol0e9rB3tL3j2Mmuh4WYuJgBWJGLSANABK4F/o="),
        ("Buy milk", "eJzjYBDK42AQYJDKEOJwKq1UyM3MyZYS4GIBiQFFwbQGI1iEESjCIQWmNZikxLg4gHL/gYAfqA7OVpLhkuISuJJ96ol0e9rB3tL3j2Mmuh4WYuLgAGJGLSANAHdKGPU="),
    ];

    /// zlib is not required to produce byte-identical compressed output
    /// across implementations for the same input -- only the *decompressed*
    /// protobuf payload needs to match pyicloud's reference bit-for-bit.
    /// (Verified manually: decompressing both sides and parsing with
    /// Python's own protobuf library showed byte-identical `Document`
    /// messages; only the outer zlib framing differed.)
    #[test]
    fn encode_matches_pyicloud_reference() {
        for (text, expected) in VECTORS.iter() {
            let mine = decompress_b64(&encode_crdt_document(text));
            let theirs = decompress_b64(expected);
            assert_eq!(mine, theirs, "protobuf payload mismatch for {text:?}");
        }
    }

    fn decompress_b64(encoded: &str) -> Vec<u8> {
        let mut padded = encoded.to_string();
        let rem = padded.len() % 4;
        if rem != 0 {
            padded.push_str(&"=".repeat(4 - rem));
        }
        let compressed = B64.decode(&padded).unwrap();
        zlib_decompress(&compressed).unwrap()
    }

    #[test]
    fn decode_matches_pyicloud_reference() {
        for (text, encoded) in VECTORS.iter() {
            assert_eq!(decode_crdt_document(encoded).unwrap(), *text, "mismatch for {encoded:?}");
        }
    }

    #[test]
    fn round_trip_arbitrary_text() {
        for text in ["日本語のタイトル", "emoji 🎉 test", "a very long reminder title with spaces"] {
            let encoded = encode_crdt_document(text);
            assert_eq!(decode_crdt_document(&encoded).unwrap(), text);
        }
    }
}
