//! Replay files: a seed plus one input byte per tick.
//!
//! Because the simulation is deterministic, that is all you need to re-run a
//! whole session exactly. A replay also carries the final state hash, so
//! playback can say "this diverged" instead of silently showing something
//! different.
//!
//! Format (little-endian): `"IANR"`, version `u8`, seed `u64`, tick count
//! `u32`, one input byte per tick, then `u8` has-hash flag and `u64` hash.

use crate::MoveInput;

const MAGIC: &[u8; 4] = b"IANR";
const VERSION: u8 = 1;
/// Sanity cap so a corrupt length cannot ask for gigabytes (about 5 hours of
/// ticks at 60 Hz).
const MAX_TICKS: u32 = 1_000_000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replay {
    pub seed: u64,
    inputs: Vec<u8>,
    pub final_hash: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    /// Does not start with the replay magic bytes.
    BadMagic,
    /// Written by a version this build does not understand.
    UnsupportedVersion(u8),
    /// File ends before the data it promises.
    Truncated,
    /// Claims more ticks than [`MAX_TICKS`].
    TooLong,
}

impl core::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not an Ianus replay file"),
            Self::UnsupportedVersion(v) => write!(f, "replay version {v} is not supported"),
            Self::Truncated => write!(f, "replay file is cut short"),
            Self::TooLong => write!(f, "replay claims an implausible length"),
        }
    }
}

impl core::error::Error for ReplayError {}

impl Replay {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            inputs: Vec::new(),
            final_hash: None,
        }
    }

    /// Record the input used for the next tick.
    pub fn push(&mut self, input: MoveInput) {
        if self.inputs.len() < MAX_TICKS as usize {
            self.inputs.push(input.bits());
        }
    }

    /// Input for `tick`, or `None` once the recording has ended.
    pub fn input_at(&self, tick: u64) -> Option<MoveInput> {
        let i = usize::try_from(tick).ok()?;
        self.inputs.get(i).map(|&b| MoveInput::from_bits(b))
    }

    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 1 + 8 + 4 + self.inputs.len() + 9);
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&self.seed.to_le_bytes());
        out.extend_from_slice(&(self.inputs.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.inputs);
        match self.final_hash {
            Some(h) => {
                out.push(1);
                out.extend_from_slice(&h.to_le_bytes());
            }
            None => out.push(0),
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReplayError> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err(ReplayError::BadMagic);
        }
        let version = r.take(1)?[0];
        if version != VERSION {
            return Err(ReplayError::UnsupportedVersion(version));
        }
        let seed = u64::from_le_bytes(r.take(8)?.try_into().unwrap());
        let count = u32::from_le_bytes(r.take(4)?.try_into().unwrap());
        if count > MAX_TICKS {
            return Err(ReplayError::TooLong);
        }
        let inputs = r.take(count as usize)?.iter().map(|b| b & 0b1111).collect();
        let final_hash = match r.take(1)?[0] {
            0 => None,
            _ => Some(u64::from_le_bytes(r.take(8)?.try_into().unwrap())),
        };
        Ok(Self {
            seed,
            inputs,
            final_hash,
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ReplayError> {
        let end = self.at.checked_add(n).ok_or(ReplayError::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(ReplayError::Truncated)?;
        self.at = end;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Replay {
        let mut r = Replay::new(0xDEAD_BEEF);
        for t in 0..100u8 {
            r.push(MoveInput::from_bits(t));
        }
        r.final_hash = Some(0x1234_5678_9abc_def0);
        r
    }

    #[test]
    fn roundtrip() {
        let r = sample();
        assert_eq!(Replay::from_bytes(&r.to_bytes()).unwrap(), r);
    }

    #[test]
    fn roundtrip_without_hash_and_when_empty() {
        let r = Replay::new(1);
        assert_eq!(Replay::from_bytes(&r.to_bytes()).unwrap(), r);
    }

    #[test]
    fn input_at_reads_back_and_ends() {
        let r = sample();
        assert_eq!(r.input_at(3), Some(MoveInput::from_bits(3)));
        assert_eq!(r.input_at(99), Some(MoveInput::from_bits(99)));
        assert_eq!(r.input_at(100), None);
        assert_eq!(r.input_at(u64::MAX), None);
    }

    #[test]
    fn rejects_wrong_magic_and_version() {
        let mut bytes = sample().to_bytes();
        bytes[0] = b'X';
        assert_eq!(Replay::from_bytes(&bytes), Err(ReplayError::BadMagic));
        let mut bytes = sample().to_bytes();
        bytes[4] = 99;
        assert_eq!(
            Replay::from_bytes(&bytes),
            Err(ReplayError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn every_truncation_is_an_error_not_a_panic() {
        let bytes = sample().to_bytes();
        for cut in 0..bytes.len() {
            assert!(Replay::from_bytes(&bytes[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn absurd_length_is_rejected_before_allocating() {
        let mut bytes = sample().to_bytes();
        bytes[13..17].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Replay::from_bytes(&bytes), Err(ReplayError::TooLong));
    }

    #[test]
    fn garbage_never_panics() {
        let mut x = 0x2545_f491_4f6c_dd1du64;
        for len in 0..200 {
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                })
                .collect();
            let _ = Replay::from_bytes(&bytes);
        }
    }
}
