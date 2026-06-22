//! Fractional delay line for ITD.
//!
//! This is a simple linear-interpolation delay. For production-quality binaural
//! rendering, replace it with an all-pass fractional delay or a higher-order Lagrange
//! interpolator if the source moves quickly.

use crate::math::clamp;

#[derive(Clone, Debug)]
pub struct FractionalDelay {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_samples: f32,
    max_delay_samples: f32,
}

impl FractionalDelay {
    pub fn new(max_delay_samples: usize) -> Self {
        let len = max_delay_samples.max(2) + 2;
        Self {
            buffer: vec![0.0; len],
            write_pos: 0,
            delay_samples: 0.0,
            max_delay_samples: max_delay_samples as f32,
        }
    }

    pub fn set_delay_samples(&mut self, delay_samples: f32) {
        self.delay_samples = clamp(delay_samples, 0.0, self.max_delay_samples);
    }

    pub fn reset(&mut self) {
        for v in &mut self.buffer {
            *v = 0.0;
        }
        self.write_pos = 0;
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let len = self.buffer.len();
        self.buffer[self.write_pos] = input;

        let mut read_pos = self.write_pos as f32 - self.delay_samples;
        while read_pos < 0.0 {
            read_pos += len as f32;
        }
        while read_pos >= len as f32 {
            read_pos -= len as f32;
        }

        let i0 = read_pos.floor() as usize;
        let i1 = (i0 + 1) % len;
        let frac = read_pos - i0 as f32;
        let out = self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac;

        self.write_pos = (self.write_pos + 1) % len;
        out
    }
}
