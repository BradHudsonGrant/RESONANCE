use std::fs;
use std::io;
use std::path::Path;

use crate::signal::Signal;

const BITS: u16 = 16;

/// The 16-bit value a sample is written as.
pub fn pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

fn size_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "the signal is too long for this file format",
    )
}

/// The 80-bit float AIFF stores its sample rate in.
fn extended(rate: u32) -> [u8; 10] {
    let mut bytes = [0; 10];
    if rate == 0 {
        return bytes;
    }
    let shift = rate.leading_zeros();
    let exponent = 16_383 + 31 - shift as u16;
    let mantissa = u64::from(rate << shift) << 32;
    bytes[..2].copy_from_slice(&exponent.to_be_bytes());
    bytes[2..].copy_from_slice(&mantissa.to_be_bytes());
    bytes
}

impl Signal {
    /// Writes 16-bit PCM in a WAV file.
    pub fn write_wav(&self, path: &Path) -> io::Result<()> {
        let data: Vec<u8> = self
            .samples
            .iter()
            .flat_map(|&sample| pcm16(sample).to_le_bytes())
            .collect();
        let data_size = u32::try_from(data.len()).map_err(|_| size_error())?;
        let block_align = self.channels * (BITS / 8);

        let mut file = Vec::with_capacity(data.len() + 44);
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(
            &data_size
                .checked_add(36)
                .ok_or_else(size_error)?
                .to_le_bytes(),
        );
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&self.channels.to_le_bytes());
        file.extend_from_slice(&self.sample_rate.to_le_bytes());
        file.extend_from_slice(&(self.sample_rate * u32::from(block_align)).to_le_bytes());
        file.extend_from_slice(&block_align.to_le_bytes());
        file.extend_from_slice(&BITS.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&data_size.to_le_bytes());
        file.extend_from_slice(&data);
        fs::write(path, file)
    }

    /// Writes 16-bit PCM in an AIFF file.
    pub fn write_aiff(&self, path: &Path) -> io::Result<()> {
        let data: Vec<u8> = self
            .samples
            .iter()
            .flat_map(|&sample| pcm16(sample).to_be_bytes())
            .collect();
        let data_size = u32::try_from(data.len()).map_err(|_| size_error())?;
        let frames = u32::try_from(self.frames()).map_err(|_| size_error())?;
        let sound_size = data_size.checked_add(8).ok_or_else(size_error)?;
        let form_size = sound_size
            .checked_add(4 + 8 + 18 + 8)
            .ok_or_else(size_error)?;

        let mut file = Vec::with_capacity(data.len() + 54);
        file.extend_from_slice(b"FORM");
        file.extend_from_slice(&form_size.to_be_bytes());
        file.extend_from_slice(b"AIFFCOMM");
        file.extend_from_slice(&18u32.to_be_bytes());
        file.extend_from_slice(&self.channels.to_be_bytes());
        file.extend_from_slice(&frames.to_be_bytes());
        file.extend_from_slice(&BITS.to_be_bytes());
        file.extend_from_slice(&extended(self.sample_rate));
        file.extend_from_slice(b"SSND");
        file.extend_from_slice(&sound_size.to_be_bytes());
        file.extend_from_slice(&[0; 8]);
        file.extend_from_slice(&data);
        fs::write(path, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scale_maps_to_the_ends_of_the_16_bit_range() {
        assert_eq!(pcm16(1.0), 32_767);
        assert_eq!(pcm16(-1.0), -32_767);
        assert_eq!(pcm16(0.0), 0);
        assert_eq!(pcm16(1.5), 32_767);
    }

    #[test]
    fn the_sample_rate_is_stored_as_an_80_bit_float() {
        assert_eq!(extended(44_100), [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(48_000), [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]);
    }
}
