use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::{AudioDecoder as AudioDecoderTrait, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

use crate::errors::{DiavloError, Result};

pub struct AudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoderTrait>,
    track_id: u32,
    sample_rate: u32,
    channels: u16,
    total_duration: Option<f64>,
}

impl AudioDecoder {
    pub fn new(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| {
            DiavloError::FileNotFound(format!("Cannot open {}: {}", path.display(), e))
        })?;

        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            hint.with_extension(ext);
        }

        let fmt_opts = FormatOptions::default();
        let meta_opts = MetadataOptions::default();

        let format = symphonia::default::get_probe()
            .probe(&hint, mss, fmt_opts, meta_opts)
            .map_err(|e| {
                DiavloError::UnsupportedFormat(format!("Unsupported or corrupt: {}", e))
            })?;

        let track = format
            .tracks()
            .iter()
            .find(|t| matches!(t.codec_params, Some(CodecParameters::Audio(_))))
            .ok_or_else(|| DiavloError::UnsupportedFormat("No audio track".to_string()))?
            .clone();

        let track_id = track.id;

        let codec_params = track.codec_params;
        let audio_params = match &codec_params {
            Some(CodecParameters::Audio(p)) => p.clone(),
            _ => unreachable!(),
        };

        let sample_rate = audio_params.sample_rate.unwrap_or(44100);
        let channels = audio_params
            .channels
            .as_ref()
            .map(|c| c.count() as u16)
            .unwrap_or(2);

        let total_dur = track.time_base.and_then(|tb| {
            track.num_frames.map(|n| {
                let secs = tb.numer.get() as f64 / tb.denom.get() as f64;
                n as f64 * secs
            })
        });

        let dec_opts = AudioDecoderOptions::default();
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&audio_params, &dec_opts)
            .map_err(|e| DiavloError::Decode(format!("Cannot create decoder: {}", e)))?;

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            total_duration: total_dur,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn total_duration(&self) -> Option<f64> {
        self.total_duration
    }

    pub fn next_packet(&mut self) -> Result<Option<Vec<f32>>> {
        loop {
            match self.format.next_packet() {
                Ok(Some(packet)) => {
                    if packet.track_id != self.track_id {
                        continue;
                    }
                    match self.decoder.decode(&packet) {
                        Ok(buf) => {
                            let n = buf.samples_interleaved();
                            if n == 0 {
                                continue;
                            }
                            let mut samples = vec![0.0f32; n];
                            buf.copy_to_slice_interleaved(&mut samples);
                            return Ok(Some(samples));
                        }
                        Err(SymphoniaError::DecodeError(_)) => continue,
                        Err(_) => return Ok(None),
                    }
                }
                Ok(None) => return Ok(None),
                Err(SymphoniaError::ResetRequired) => continue,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(DiavloError::Decode(format!("Read error: {}", e))),
            }
        }
    }

    pub fn seek(&mut self, seconds: f64) -> Result<()> {
        let time = Time::from(seconds.max(0.0) as u32);
        self.format
            .seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| DiavloError::Decode(format!("Seek error: {}", e)))?;
        self.decoder.reset();
        Ok(())
    }
}
