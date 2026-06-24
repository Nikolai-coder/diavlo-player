#![allow(dead_code)]
use rubato::audioadapter_buffers::owned::InterleavedOwned;
use rubato::{Fft, FixedSync, Resampler, ResamplerConstructionError};

pub struct AudioResampler {
    resampler: Option<Fft<f32>>,
    source_rate: u32,
    target_rate: u32,
    source_channels: u16,
    target_channels: u16,
    interleaved_output: Vec<f32>,
}

impl AudioResampler {
    pub fn new(
        source_rate: u32,
        target_rate: u32,
        source_channels: u16,
        target_channels: u16,
    ) -> Result<Self, ResamplerConstructionError> {
        let resampler = if source_rate != target_rate || source_channels != target_channels {
            let chunk_size = 1024;
            let nbr_channels = source_channels as usize;

            let r = Fft::new(
                source_rate as usize,
                target_rate as usize,
                chunk_size,
                1,
                nbr_channels,
                FixedSync::Output,
            )?;
            Some(r)
        } else {
            None
        };

        Ok(Self {
            resampler,
            source_rate,
            target_rate,
            source_channels,
            target_channels,
            interleaved_output: Vec::with_capacity(8192),
        })
    }

    pub fn process(&mut self, input: &[f32]) -> &[f32] {
        self.interleaved_output.clear();

        if input.is_empty() {
            return &[];
        }

        let r = match self.resampler.as_mut() {
            Some(r) => r,
            None => {
                self.interleaved_output.extend_from_slice(input);
                return &self.interleaved_output;
            }
        };

        let in_ch = self.source_channels as usize;
        let out_ch = self.target_channels as usize;
        let input_frames = input.len() / in_ch;

        let buf_in = match InterleavedOwned::<f32>::new_from(
            input.to_vec(),
            in_ch,
            input_frames,
        ) {
            Ok(b) => b,
            Err(_) => return &[],
        };

        match r.process(&buf_in, 0, None) {
            Ok(output) => {
                let out_vec = output.take_data();
                self.append_output(&out_vec, in_ch, out_ch);
            }
            Err(_) => {}
        }

        if self.interleaved_output.is_empty() {
            &[]
        } else {
            &self.interleaved_output
        }
    }

    fn append_output(&mut self, out_vec: &[f32], in_ch: usize, out_ch: usize) {
        if in_ch == out_ch {
            self.interleaved_output.extend_from_slice(out_vec);
        } else {
            let out_frames = out_vec.len() / in_ch;
            for frame in 0..out_frames {
                let base = frame * in_ch;
                match (in_ch, out_ch) {
                    (1, 2) => {
                        let v = out_vec[base];
                        self.interleaved_output.push(v);
                        self.interleaved_output.push(v);
                    }
                    (2, 1) => {
                        let v = (out_vec[base] + out_vec[base + 1]) * 0.5;
                        self.interleaved_output.push(v);
                    }
                    _ => {
                        for ch in 0..out_ch {
                            let v = if ch < in_ch { out_vec[base + ch] } else { 0.0 };
                            self.interleaved_output.push(v);
                        }
                    }
                }
            }
        }
    }

    pub fn flush(&mut self) -> &[f32] {
        self.interleaved_output.clear();
        if let Some(ref mut r) = self.resampler {
            let empty =
                match InterleavedOwned::<f32>::new_from(vec![], self.source_channels as usize, 0) {
                    Ok(b) => b,
                    Err(_) => InterleavedOwned::new(0.0f32, self.source_channels as usize, 0),
                };
            match r.process(&empty, 0, None) {
                Ok(output) => {
                    let out_vec = output.take_data();
                    if !out_vec.is_empty() {
                        let out_ch = self.source_channels as usize;
                        let out_tgt = self.target_channels as usize;
                        let out_frames = out_vec.len() / out_ch;
                        for frame in 0..out_frames {
                            let base = frame * out_ch;
                            match (self.source_channels, self.target_channels) {
                                (1, 2) => {
                                    let v = out_vec[base];
                                    self.interleaved_output.push(v);
                                    self.interleaved_output.push(v);
                                }
                                (2, 1) => {
                                    let v = (out_vec[base] + out_vec[base + 1]) * 0.5;
                                    self.interleaved_output.push(v);
                                }
                                _ => {
                                    for ch in 0..out_tgt {
                                        let v =
                                            if ch < out_ch { out_vec[base + ch] } else { 0.0 };
                                        self.interleaved_output.push(v);
                                    }
                                }
                            }
                        }
                    }
                    &self.interleaved_output
                }
                Err(_) => &[],
            }
        } else {
            &[]
        }
    }

    pub fn reset(&mut self) {
        if let Some(ref mut r) = self.resampler {
            r.reset();
        }
    }
}