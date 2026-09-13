//! Decoded source samples to the device's rate and channel count.
//!
//! Separate from the engine so a track change can be tested without an output
//! device.

use super::decode::Spec;
use super::output::{remap_channels, Plan};
use super::resample::Resample;

pub struct Converter {
    src: Spec,
    plan: Plan,
    speed: f64,
    resampler: Option<Resample>,
    scratch: Vec<f32>,
    finished: bool,
}

impl Converter {
    /// Converts `src` for output per `plan`, playing at `speed`.
    ///
    /// A resampler is built when the device cannot take the source rate, and at
    /// any speed but normal, since varispeed is a change of resampling ratio.
    pub fn new(src: Spec, plan: Plan, speed: f64) -> Self {
        let resampler = if plan.needs_resample(src) || speed != 1.0 {
            // In the source's channel count: channels are remapped afterwards.
            Resample::new(src.rate, plan.rate, src.channels, speed)
        } else {
            None
        };
        Converter {
            src,
            plan,
            speed,
            resampler,
            scratch: Vec::new(),
            finished: false,
        }
    }

    pub fn src(&self) -> Spec {
        self.src
    }

    pub fn resampling(&self) -> bool {
        self.resampler.is_some()
    }

    /// Whether the next track can be pushed through this converter unchanged.
    ///
    /// Continuing keeps the resampler's filter state across the join, which is
    /// what resampling both tracks as one stream would produce.
    pub fn continues(&self, src: Spec, plan: Plan, speed: f64) -> bool {
        !self.finished && self.src == src && self.plan == plan && self.speed == speed
    }

    /// Converts interleaved source samples and appends them to `sink`.
    pub fn push(&mut self, decoded: &[f32], sink: &mut Vec<f32>) {
        let (src_ch, dst_ch) = (self.src.channels as usize, self.plan.channels as usize);
        match self.resampler.as_mut() {
            Some(r) => {
                self.scratch.clear();
                r.push(decoded, &mut self.scratch);
                remap_channels(&self.scratch, src_ch, dst_ch, sink);
            }
            None => remap_channels(decoded, src_ch, dst_ch, sink),
        }
    }

    /// Ends the stream, appending what the resampler still holds. Later calls do nothing.
    pub fn finish(&mut self, sink: &mut Vec<f32>) {
        if std::mem::replace(&mut self.finished, true) {
            return;
        }
        if let Some(r) = self.resampler.as_mut() {
            self.scratch.clear();
            r.flush(&mut self.scratch);
            let (src_ch, dst_ch) = (self.src.channels as usize, self.plan.channels as usize);
            remap_channels(&self.scratch, src_ch, dst_ch, sink);
        }
    }
}
