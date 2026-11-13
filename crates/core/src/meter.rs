//! How a level meter moves: it rises to a new peak at once and falls back at a steady rate, so a
//! single loud sample stays visible long enough to read.

/// The quietest level the meter shows. Anything quieter sits at the bottom of the scale.
pub const FLOOR_DB: f32 = -48.0;

/// How fast the meter falls after a peak.
pub const FALL_DB_PER_SECOND: f32 = 24.0;

/// One channel of a peak meter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakMeter {
    level_db: f32,
}

impl Default for PeakMeter {
    fn default() -> Self {
        Self { level_db: FLOOR_DB }
    }
}

impl PeakMeter {
    /// Moves the meter on by `elapsed_seconds`, during which the loudest sample had the amplitude
    /// `peak`, where full scale is 1.
    pub fn update(&mut self, peak: f32, elapsed_seconds: f32) {
        let fallen = self.level_db - FALL_DB_PER_SECOND * elapsed_seconds.max(0.0);
        self.level_db = decibels(peak).max(fallen).max(FLOOR_DB);
    }

    pub fn decibels(&self) -> f32 {
        self.level_db
    }

    /// Where the meter stands, from 0 at the floor to 1 at full scale.
    pub fn position(&self) -> f32 {
        ((self.level_db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
    }
}

/// The level of an amplitude in decibels relative to full scale.
pub fn decibels(amplitude: f32) -> f32 {
    if amplitude > 0.0 {
        20.0 * amplitude.log10()
    } else {
        f32::NEG_INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn full_scale_is_zero_decibels_and_half_scale_is_about_minus_six() {
        assert!(close(decibels(1.0), 0.0));
        assert!(close(decibels(0.5), -6.0206));
        assert_eq!(decibels(0.0), f32::NEG_INFINITY);
    }

    #[test]
    fn the_meter_jumps_to_a_peak_at_once() {
        let mut meter = PeakMeter::default();

        meter.update(0.5, 0.016);

        assert!(close(meter.decibels(), -6.0206));
    }

    #[test]
    fn after_a_peak_the_meter_falls_at_its_steady_rate() {
        let mut meter = PeakMeter::default();
        meter.update(1.0, 0.0);

        meter.update(0.0, 0.5);

        assert!(close(meter.decibels(), -FALL_DB_PER_SECOND * 0.5));
    }

    #[test]
    fn silence_rests_on_the_floor_at_position_zero() {
        let mut meter = PeakMeter::default();
        meter.update(1.0, 0.0);

        meter.update(0.0, 10.0);

        assert_eq!(meter.decibels(), FLOOR_DB);
        assert_eq!(meter.position(), 0.0);
    }

    #[test]
    fn full_scale_is_the_top_of_the_scale_and_louder_stays_there() {
        let mut meter = PeakMeter::default();

        meter.update(1.0, 0.0);
        assert!(close(meter.position(), 1.0));
        meter.update(2.0, 0.0);
        assert_eq!(meter.position(), 1.0);
    }
}
