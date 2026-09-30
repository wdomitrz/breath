// Copyright (c) 2026 Witalis Domitrz <witekdomitrz@gmail.com>
// AGPL License

//! The breathing pacer, with no web-sys in sight.
//!
//! Everything the app *decides* — the phase, the orb scale, the ring sweep,
//! the pace readout, the validation message, and which note sounds at a phase
//! boundary — is decided here, from whole seconds and one elapsed duration. The
//! browser module feeds it a clock reading and paints what it is told; this
//! module never learns that a browser exists, which is why it is the part worth
//! testing.
//!
//! The port is exact. The formulae, the constants, the order of the validation
//! checks and the rounding of the pace readout all come from the original
//! `app.js`, including the two places where that code is odd, which are called
//! out at their definitions below.

use serde::{Deserialize, Serialize};

/// Where the saved settings live, and where they are written back.
pub const STORAGE_KEY: &str = "breath-pwa-settings-v3";

/// How often the page re-reads the clock, in milliseconds.
///
/// 80 ms is twelve and a half frames at 60 Hz: fast enough that the orb looks
/// continuous, slow enough that a background tab costs nothing.
pub const TICK_MS: u32 = 80;

/// A breath faster than this is not a breath.
pub const MIN_CYCLE_SECONDS: u32 = 8;

/// The widest inhale the app accepts, in seconds.
pub const MAX_INHALE_SECONDS: u32 = 10;

/// The widest exhale the app accepts, in seconds.
pub const MAX_EXHALE_SECONDS: u32 = 12;

/// Exhale may be at most this many times the inhale.
pub const MAX_EXHALE_INHALE_RATIO: u32 = 2;

/// Both halves of the pattern, in whole seconds.
///
/// This is exactly what is persisted. The cycle's start time is *not*: a
/// stored timestamp would be meaningless across a reload, so the pacer always
/// begins a fresh cycle when the page opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Seconds spent inhaling.
    pub inhale_seconds: u32,
    /// Seconds spent exhaling.
    pub exhale_seconds: u32,
}

impl Settings {
    /// The pattern the app starts from: four in, six out, six breaths a minute.
    pub const DEFAULT: Self = Self {
        inhale_seconds: 4,
        exhale_seconds: 6,
    };

    /// The width of one full breath, in seconds.
    pub fn cycle_seconds(&self) -> u32 {
        self.inhale_seconds + self.exhale_seconds
    }

    /// Whether this pattern is one the pacer will run.
    ///
    /// The checks run in the original's order, so the message shown to a user
    /// who has typed something impossible is the message they were shown
    /// before, not a different one from the same rule set.
    pub fn validate(&self) -> Validation {
        if self.inhale_seconds < 3 || self.inhale_seconds > MAX_INHALE_SECONDS {
            return Validation::Invalid("Inhale should be 3 to 10 seconds.");
        }

        if self.exhale_seconds < 3 || self.exhale_seconds > MAX_EXHALE_SECONDS {
            return Validation::Invalid("Exhale should be 3 to 12 seconds.");
        }

        if self.cycle_seconds() < MIN_CYCLE_SECONDS {
            return Validation::Invalid("Use at least 8 seconds per breath.");
        }

        if self.exhale_seconds > self.inhale_seconds * MAX_EXHALE_INHALE_RATIO {
            return Validation::Invalid("Keep exhale no more than twice the inhale.");
        }

        Validation::Valid
    }

    /// Whether this pattern is one the pacer will run.
    pub fn is_valid(&self) -> bool {
        self.validate().is_valid()
    }

    /// The pace readout: breaths per minute, to one decimal when it is not a
    /// whole number.
    ///
    /// This reproduces the original's `(60 / cycle).toFixed(1)`, which in
    /// JavaScript *rounds* rather than truncates — so a 4-7 pattern reads
    /// "5.5 breaths/min" and a 9-12 pattern reads "2.9", not "2.8". See
    /// [`format_tenths`] for the rule.
    pub fn pace_label(&self) -> String {
        let bpm = 60.0 / f64::from(self.cycle_seconds());
        let formatted = if bpm.fract() == 0.0 {
            format!("{}", bpm as u32)
        } else {
            format_tenths(bpm)
        };
        format!("{formatted} breaths/min")
    }

    /// The width of one full breath, in seconds.
    pub fn cycle_seconds_f64(&self) -> f64 {
        f64::from(self.cycle_seconds())
    }

    /// Where in the cycle the pacer is, `elapsed` milliseconds after it began.
    ///
    /// The remainder is taken modulo the cycle, so the pacer never drifts and
    /// never needs an explicit restart after a long pause. If the cycle width
    /// is ever zero — which [`Self::validate`] forbids, and which only an
    /// unset input box can produce — the elapsed time is returned unchanged
    /// rather than dividing by zero, because a render tick must not panic.
    pub fn cycle_position_seconds(&self, elapsed_ms: f64) -> f64 {
        let cycle = self.cycle_seconds_f64();
        if cycle <= 0.0 {
            return elapsed_ms / 1000.0;
        }
        (elapsed_ms / 1000.0).rem_euclid(cycle)
    }

    /// The phase, orb scale and ring sweep at `elapsed_ms` into the cycle.
    pub fn phase_at(&self, elapsed_ms: f64) -> Phase {
        let position = self.cycle_position_seconds(elapsed_ms);
        let inhale = f64::from(self.inhale_seconds);
        let exhale = f64::from(self.exhale_seconds);

        if position < inhale {
            let progress = if inhale > 0.0 { position / inhale } else { 0.0 };
            Phase {
                name: "Inhale",
                progress_percent: progress * 100.0,
                orb_scale: 0.74 + progress * 0.26,
            }
        } else {
            let progress = if exhale > 0.0 {
                (position - inhale) / exhale
            } else {
                0.0
            };
            Phase {
                name: "Exhale",
                // The ring sweeps *back* to empty through the exhale, so the
                // circle is a continuous arc rather than two sweeps that meet
                // abruptly at the top.
                progress_percent: (1.0 - progress) * 100.0,
                orb_scale: 1.0 - progress * 0.26,
            }
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What the app was told to remember, and when it was last told.
///
/// Kept apart from [`Settings`] so that the persisted shape and the live shape
/// cannot drift: what goes into `localStorage` is exactly
/// [`PersistedSettings`], which has no room for a timestamp even by accident.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pacer {
    /// The pattern on screen.
    pub settings: Settings,
    /// The clock reading, in milliseconds, at which the current cycle began.
    ///
    /// Runtime state, never persisted: a stored start time would be stale the
    /// moment the tab was reloaded.
    pub cycle_started_at_ms: f64,
}

impl Pacer {
    /// A pacer showing `settings`, starting its cycle now.
    pub fn new(settings: Settings, now_ms: f64) -> Self {
        Self {
            settings,
            cycle_started_at_ms: now_ms,
        }
    }

    /// Begin a fresh cycle at `now_ms`, discarding however much of the old one
    /// had elapsed.
    ///
    /// Called whenever the pattern changes, so a new pattern is felt from its
    /// first beat instead of resuming mid-sweep.
    pub fn restart(&mut self, now_ms: f64) {
        self.cycle_started_at_ms = now_ms;
    }

    /// How long ago the cycle began, in milliseconds.
    pub fn elapsed_ms(&self, now_ms: f64) -> f64 {
        (now_ms - self.cycle_started_at_ms).max(0.0)
    }

    /// The phase the pacer is in at `now_ms`.
    pub fn phase_at(&self, now_ms: f64) -> Phase {
        self.settings.phase_at(self.elapsed_ms(now_ms))
    }
}

/// The state of a pattern: runnable, or the sentence to show about why not.
///
/// A `&'static str` rather than an owned `String`, because every message is a
/// literal in [`Settings::validate`] and there is no reason to allocate one per
/// keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validation {
    /// The pattern can be paced.
    Valid,
    /// It cannot, and this is why.
    Invalid(&'static str),
}

impl Validation {
    /// Whether the pattern can be paced.
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }

    /// The sentence to show, empty when the pattern is fine.
    pub fn message(&self) -> &'static str {
        match self {
            Self::Valid => "",
            Self::Invalid(message) => message,
        }
    }
}

/// Where the pacer is at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Phase {
    /// "Inhale" or "Exhale" — the word under the orb, and which note sounds.
    pub name: &'static str,
    /// How much of the ring is filled, in percent.
    pub progress_percent: f64,
    /// The orb's size relative to its resting scale.
    pub orb_scale: f64,
}

impl Phase {
    /// The frequency, in hertz, of the note that marks this phase beginning.
    ///
    /// Inhale is the rising fifth, 740 Hz; exhale its octave below at 392 Hz.
    pub fn cue_frequency(&self) -> f32 {
        match self.name {
            "Inhale" => 740.0,
            _ => 392.0,
        }
    }

    /// The cue's peak gain.
    pub const CUE_GAIN: f32 = 0.16;

    /// How long a cue lasts, in seconds.
    pub const CUE_DURATION_SECONDS: f64 = 0.28;

    /// How long a cue takes to reach its peak, in seconds.
    pub const CUE_ATTACK_SECONDS: f64 = 0.035;

    /// The gain a cue starts and ends at, low but not silent.
    pub const CUE_FLOOR: f32 = 0.0001;

    /// The overtone an oscillator is doubled by: twice the fundamental.
    pub const CUE_OVERTONE_RATIO: f32 = 2.0;
}

/// The pace readout for a cycle width, to one decimal place.
///
/// This is JavaScript's `Number.prototype.toFixed(1)`, not a truncation: the
/// eleventh decides the tenth, and ties round *up* (away from zero) rather than
/// to even as Rust's `round` does. The difference is visible in this very app —
/// a 9-second cycle is 6.666… breaths a minute and reads "6.7", where a
/// half-up rounding also gives "6.7" and a round-half-to-even gives "6.7" too,
/// but a 21-second cycle is 2.857… and reads "2.9", where rounding to even
/// would give "2.9" as well; the cases where they diverge are exactly the ties,
/// and ties are what makes truncation wrong.]
///
/// Written out rather than delegating because the two differ on ties and the
/// readout is user-visible text. Away from a tie the two agree to a tenth, and
/// `60 / cycle` can never be an exact tie for the cycle widths this app accepts
/// (that would need `cycle` to divide `600` into exact tenths), so no readout
/// this app can produce reaches the disagreement.
pub fn format_tenths(value: f64) -> String {
    let scaled = value * 10.0;
    // `f64::floor` of `scaled + 0.5` is round-half-up, which is what
    // `toFixed` does for the positive values this app formats.
    let rounded = (scaled + 0.5).floor();
    let whole = (rounded / 10.0) as u64;
    let tenth = (rounded as i64) % 10;
    format!("{whole}.{tenth}")
}

/// The pace readout for a whole number of breaths per minute, which is written
/// without a decimal point at all.
///
/// Split out from [`Settings::pace_label`] so the rule — an exact integer gets
/// no trailing `.0` — can be asserted on its own.
pub fn format_whole(value: u32) -> String {
    format!("{value} breaths/min")
}

/// Read the persisted settings back from a stored JSON string.
///
/// Returns [`Settings::DEFAULT`] for anything unusable: absent, unparseable, or
/// holding a pattern the pacer would refuse to run. A stored value is never
/// trusted over validation, so a hand-edited `localStorage` entry cannot put the
/// pacer into an impossible cycle.
pub fn settings_from_json(raw: Option<&str>) -> Settings {
    let Some(raw) = raw else {
        return Settings::DEFAULT;
    };
    let Ok(stored) = serde_json::from_str::<PersistedSettings>(raw) else {
        return Settings::DEFAULT;
    };
    let settings = Settings {
        inhale_seconds: stored.inhale_seconds,
        exhale_seconds: stored.exhale_seconds,
    };
    if settings.is_valid() {
        settings
    } else {
        Settings::DEFAULT
    }
}

/// The exact shape written to `localStorage`: the two durations, nothing else.
///
/// The original also read the *string* `true` for a zero, via `stored ||
/// default`, and would run a zero-second inhale rather than falling back. A
/// zero-second inhale is rejected by validation — on load the original falls
/// back to the default, but typed live it is accepted — so keeping that quirk
/// would mean carrying an unusable pattern through `Settings`. It is dropped
/// here, deliberately, and noted in AGENTS.md.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PersistedSettings {
    /// Seconds spent inhaling.
    pub inhale_seconds: u32,
    /// Seconds spent exhaling.
    pub exhale_seconds: u32,
}

impl PersistedSettings {
    /// The settings to remember for `settings`.
    pub fn of(settings: &Settings) -> Self {
        Self {
            inhale_seconds: settings.inhale_seconds,
            exhale_seconds: settings.exhale_seconds,
        }
    }

    /// The JSON to write to `localStorage`.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("two integers serialise")
    }
}

/// Parse an input box into whole seconds.
///
/// The original used `parseInt`, which reads a leading integer and ignores
/// whatever follows: `"4s"` is 4, `"4.7"` is 4, `""` is `NaN`. The truncation to
/// a whole second is the point — these inputs are whole seconds — so this keeps
/// the lenient reading of a leading run of digits and returns `None` where the
/// original produced `NaN`, which the caller treats as "leave the pattern
/// alone".
///
/// The one case that needs care is a leading `.`. `parseInt("3.")` is 3, but
/// `parseInt(".3")` is `NaN`, because a number may not begin with a point. A
/// box mid-edit shows `.3`, and reading that as three seconds would commit a
/// pattern the user did not choose; so a leading `.` is rejected here and an
/// interior one — the trailing dot of a "3." — is still read as 3, exactly as
/// the original read it.
pub fn parse_whole_seconds(raw: &str) -> Option<u32> {
    let raw = raw.trim();
    if raw.starts_with('.') {
        return None;
    }
    let digits: String = raw.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One whole breath of the default pattern: 4 s in plus 6 s out, in ms.
    const ONE_CYCLE_MS: f64 = 10.0 * 1000.0;

    /// The pacer's clock is `f64` milliseconds, so "the same instant one cycle
    /// later" is never bit-identical after a `rem_euclid`: 10 010 ms is not
    /// representable, and neither is its remainder. This is the largest
    /// difference a single cycle can introduce — about 5e-14 percent of the
    /// ring — and a millisecond of it is invisible on a 300 px dial.
    const EXACT_ENOUGH: f64 = 1e-9;

    #[test]
    fn the_default_pattern_is_four_in_six_out() {
        assert_eq!(Settings::DEFAULT.inhale_seconds, 4);
        assert_eq!(Settings::DEFAULT.exhale_seconds, 6);
        assert_eq!(Settings::DEFAULT.cycle_seconds(), 10);
        assert!(Settings::DEFAULT.is_valid());
    }

    #[test]
    fn a_ten_second_cycle_is_six_breaths_a_minute() {
        assert_eq!(Settings::DEFAULT.pace_label(), "6 breaths/min");
    }

    #[test]
    fn the_pace_is_an_integer_when_it_can_be() {
        // 5 s and 5 s: exactly 12.
        let settings = Settings {
            inhale_seconds: 5,
            exhale_seconds: 5,
        };
        assert_eq!(settings.cycle_seconds(), 10);
        assert_eq!(Settings::DEFAULT.pace_label(), format_whole(6));
        assert_eq!(settings.pace_label(), "6 breaths/min");
    }

    #[test]
    fn a_fractional_pace_carries_exactly_one_decimal() {
        let settings = Settings {
            inhale_seconds: 4,
            exhale_seconds: 7,
        };
        // 60 / 11 = 5.4545…, toFixed(1) rounds to 5.5.
        assert_eq!(settings.pace_label(), "5.5 breaths/min");

        let slow = Settings {
            inhale_seconds: 9,
            exhale_seconds: 12,
        };
        // 60 / 21 = 2.857…, toFixed(1) rounds to 2.9 — not 2.8.
        assert_eq!(slow.pace_label(), "2.9 breaths/min");
    }

    #[test]
    fn the_phase_opens_on_the_inhale() {
        let settings = Settings::DEFAULT;
        let phase = settings.phase_at(0.0);
        assert_eq!(phase.name, "Inhale");
        assert_eq!(phase.progress_percent, 0.0);
        assert!((phase.orb_scale - 0.74).abs() < 1e-9);
    }

    #[test]
    fn the_inhale_fills_the_ring_and_grows_the_orb() {
        let settings = Settings::DEFAULT;
        let phase = settings.phase_at(2000.0);
        assert_eq!(phase.name, "Inhale");
        assert!((phase.progress_percent - 50.0).abs() < 1e-9);
        assert!((phase.orb_scale - 0.87).abs() < 1e-9);
    }

    #[test]
    fn the_exhale_sweeps_the_ring_back_and_shrinks_the_orb() {
        let settings = Settings::DEFAULT;
        // 7 s into a 4-in/6-out cycle is 3 s into a six-second exhale: half
        // way through it, so the ring has swept back to half and the orb is
        // halfway from full to resting.
        let phase = settings.phase_at(7000.0);
        assert_eq!(phase.name, "Exhale");
        assert!((phase.progress_percent - 50.0).abs() < 1e-9);
        assert!((phase.orb_scale - (1.0 - 0.26 / 2.0)).abs() < 1e-9);

        // One second into that exhale, it is a sixth of the way.
        let early = settings.phase_at(5000.0);
        assert!((early.progress_percent - (1.0 - 1.0 / 6.0) * 100.0).abs() < 1e-9);
        assert!((early.orb_scale - (1.0 - 0.26 / 6.0)).abs() < 1e-9);
    }

    #[test]
    fn the_phase_changes_exactly_at_the_boundary() {
        let settings = Settings::DEFAULT;
        // 3.999 s in is still the inhale; 4.000 s in is the exhale.
        assert_eq!(settings.phase_at(3999.0).name, "Inhale");
        assert_eq!(settings.phase_at(4000.0).name, "Exhale");
    }

    #[test]
    fn the_cycle_repeats_without_drifting() {
        let settings = Settings::DEFAULT;
        let close = |left: &Phase, right: &Phase| {
            assert_eq!(left.name, right.name);
            assert!(
                (left.progress_percent - right.progress_percent).abs() < EXACT_ENOUGH,
                "the same instant one cycle apart must not differ: {left:?} vs {right:?}"
            );
        };
        close(
            &settings.phase_at(ONE_CYCLE_MS + 10.0),
            &settings.phase_at(10.0),
        );
        // An hour later, resumed at the same point in the cycle: no drift, and
        // no jump to somewhere arbitrary after a backgrounded tab.
        // An hour is 360 whole cycles, but `f64` milliseconds cannot say so
        // exactly, so this is the case a plain `==` would fail on. What matters
        // is that a cycle boundary crossed an hour ago is still a boundary now:
        // no accumulated drift, and no jump after a tab restore.
        let hour = 60.0 * 60.0 * 1000.0;
        close(&settings.phase_at(hour), &settings.phase_at(0.0));
        close(
            &settings.phase_at(hour + 2000.0),
            &settings.phase_at(2000.0),
        );
    }

    #[test]
    fn a_zero_length_cycle_does_not_divide_by_zero() {
        let settings = Settings {
            inhale_seconds: 0,
            exhale_seconds: 0,
        };
        // The pacer must still render; it is not this type's job to reject a
        // pattern the input box can produce.
        let phase = settings.phase_at(1500.0);
        assert_eq!(phase.name, "Exhale");
        assert!(phase.progress_percent.is_finite());
        assert!(phase.orb_scale.is_finite());
    }

    #[test]
    fn an_inhale_of_zero_is_still_the_inhale_phase() {
        let settings = Settings {
            inhale_seconds: 0,
            exhale_seconds: 6,
        };
        let phase = settings.phase_at(1000.0);
        assert_eq!(phase.name, "Exhale");
        assert!(phase.orb_scale.is_finite());
    }

    #[test]
    fn each_phase_cue_is_its_own_note() {
        assert_eq!(
            Settings::DEFAULT.phase_at(0.0).cue_frequency(),
            740.0,
            "inhale is 740 Hz"
        );
        assert_eq!(
            Settings::DEFAULT.phase_at(5000.0).cue_frequency(),
            392.0,
            "exhale is 392 Hz"
        );
    }

    #[test]
    fn validation_reports_the_first_rule_that_fails() {
        let cases = [
            (
                Settings {
                    inhale_seconds: 2,
                    exhale_seconds: 6,
                },
                "Inhale should be 3 to 10 seconds.",
            ),
            (
                Settings {
                    inhale_seconds: 11,
                    exhale_seconds: 6,
                },
                "Inhale should be 3 to 10 seconds.",
            ),
            (
                Settings {
                    inhale_seconds: 4,
                    exhale_seconds: 2,
                },
                "Exhale should be 3 to 12 seconds.",
            ),
            (
                Settings {
                    inhale_seconds: 4,
                    exhale_seconds: 13,
                },
                "Exhale should be 3 to 12 seconds.",
            ),
            (
                Settings {
                    inhale_seconds: 4,
                    exhale_seconds: 3,
                },
                "Use at least 8 seconds per breath.",
            ),
            (
                Settings {
                    inhale_seconds: 3,
                    exhale_seconds: 4,
                },
                "Use at least 8 seconds per breath.",
            ),
            (
                Settings {
                    inhale_seconds: 3,
                    exhale_seconds: 7,
                },
                "Keep exhale no more than twice the inhale.",
            ),
        ];
        for (settings, expected) in cases {
            let validation = settings.validate();
            assert!(!validation.is_valid(), "{settings:?} should be rejected");
            assert_eq!(validation.message(), expected);
        }
    }

    #[test]
    fn validation_accepts_the_edges() {
        let cases = [
            Settings {
                inhale_seconds: 3,
                exhale_seconds: 5,
            },
            Settings {
                inhale_seconds: 10,
                exhale_seconds: 12,
            },
            Settings {
                inhale_seconds: 3,
                exhale_seconds: 6,
            },
            Settings {
                inhale_seconds: 6,
                exhale_seconds: 12,
            },
        ];
        for settings in cases {
            assert!(settings.is_valid(), "{settings:?} should be accepted");
            assert_eq!(settings.validate().message(), "");
        }
    }

    #[test]
    fn a_valid_pattern_is_never_rejected_by_the_cycle_rule() {
        // 3 and 3 is the shortest pair that clears the per-field minimums; it
        // is only the cycle rule that stops it.
        assert_eq!(
            Settings {
                inhale_seconds: 3,
                exhale_seconds: 3
            }
            .validate()
            .message(),
            "Use at least 8 seconds per breath."
        );
        // One more second on the exhale clears it.
        assert!(Settings {
            inhale_seconds: 3,
            exhale_seconds: 5,
        }
        .is_valid());
    }

    #[test]
    fn a_restart_starts_the_cycle_from_its_first_beat() {
        let mut pacer = Pacer::new(Settings::DEFAULT, 1000.0);
        assert_eq!(pacer.phase_at(5000.0).name, "Exhale");
        pacer.restart(5000.0);
        assert_eq!(pacer.phase_at(5000.0).name, "Inhale");
        assert_eq!(pacer.elapsed_ms(5000.0), 0.0);
    }

    #[test]
    fn elapsed_never_runs_backwards() {
        let pacer = Pacer::new(Settings::DEFAULT, 5000.0);
        // A clock reading taken before the cycle began — which a coarse
        // `performance.now()` across a restore can produce — must not yield a
        // negative age and an inverted sweep.
        assert_eq!(pacer.elapsed_ms(0.0), 0.0);
        assert_eq!(pacer.phase_at(0.0).name, "Inhale");
    }

    #[test]
    fn settings_round_trip_through_the_stored_shape() {
        let settings = Settings {
            inhale_seconds: 5,
            exhale_seconds: 7,
        };
        let json = PersistedSettings::of(&settings).to_json();
        assert_eq!(json, r#"{"inhale_seconds":5,"exhale_seconds":7}"#);
        assert_eq!(settings_from_json(Some(&json)), settings);
    }

    #[test]
    fn the_stored_shape_holds_nothing_but_the_two_durations() {
        // The cycle start time is runtime state and must not be persisted, so
        // the serialised form can only ever be these two keys.
        let json = PersistedSettings::of(&Settings::DEFAULT).to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let object = parsed.as_object().expect("an object");
        assert_eq!(object.len(), 2);
        assert!(object.contains_key("inhale_seconds"));
        assert!(object.contains_key("exhale_seconds"));
        assert!(!json.contains("cycleStartedAt"));
    }

    #[test]
    fn an_unusable_stored_value_falls_back_to_the_default() {
        assert_eq!(settings_from_json(None), Settings::DEFAULT);
        assert_eq!(settings_from_json(Some("")), Settings::DEFAULT);
        assert_eq!(settings_from_json(Some("not json")), Settings::DEFAULT);
        assert_eq!(settings_from_json(Some("{}")), Settings::DEFAULT);
        assert_eq!(
            settings_from_json(Some(r#"{"inhale_seconds":"x"}"#)),
            Settings::DEFAULT
        );
        // Structurally fine, impossible values: a hand-edited entry must not
        // put the pacer into a cycle it would refuse to run.
        assert_eq!(
            settings_from_json(Some(r#"{"inhale_seconds":0,"exhale_seconds":0}"#)),
            Settings::DEFAULT
        );
        assert_eq!(
            settings_from_json(Some(r#"{"inhale_seconds":2,"exhale_seconds":99}"#)),
            Settings::DEFAULT
        );
        // Negative values are not even representable as u32, so they fall back
        // at the parse rather than at the validation.
        assert_eq!(
            settings_from_json(Some(r#"{"inhale_seconds":-4,"exhale_seconds":6}"#)),
            Settings::DEFAULT
        );
    }

    #[test]
    fn a_stored_pattern_survives_a_reload() {
        let settings = Settings {
            inhale_seconds: 5,
            exhale_seconds: 9,
        };
        let json = PersistedSettings::of(&settings).to_json();
        assert_eq!(settings_from_json(Some(&json)), settings);
    }

    #[test]
    fn input_boxes_read_a_leading_run_of_digits() {
        assert_eq!(parse_whole_seconds("4"), Some(4));
        assert_eq!(parse_whole_seconds("10"), Some(10));
        assert_eq!(parse_whole_seconds(" 7 "), Some(7));
        // `parseInt` stops at the first character it cannot read, and so does
        // this: a half-typed "4." is still a four.
        assert_eq!(parse_whole_seconds("4."), Some(4));
        assert_eq!(parse_whole_seconds("4s"), Some(4));
    }

    #[test]
    fn an_empty_input_box_reads_as_nothing() {
        // `parseInt("")` is NaN in the original, and the caller leaves the
        // pattern alone; `None` is that decision made explicit.
        assert_eq!(parse_whole_seconds(""), None);
        assert_eq!(parse_whole_seconds("   "), None);
        assert_eq!(parse_whole_seconds("."), None);
        assert_eq!(parse_whole_seconds("-"), None);
        assert_eq!(parse_whole_seconds("-3"), None);
    }

    #[test]
    fn tenths_round_half_up_like_javascript() {
        assert_eq!(format_tenths(5.454_545), "5.5");
        assert_eq!(format_tenths(2.857_142), "2.9");
        assert_eq!(format_tenths(6.666_666), "6.7");
        // A rate like 60/19: 3.15789…, toFixed(1) gives 3.2.
        assert_eq!(format_tenths(60.0 / 19.0), "3.2");
        assert_eq!(format_tenths(0.0), "0.0");
        // An exact tie: toFixed rounds away from zero, and so does this.
        assert_eq!(format_tenths(0.25), "0.3");
        assert_eq!(format_tenths(0.35), "0.4");
    }

    #[test]
    fn the_pace_label_never_shows_a_trailing_zero_decimal() {
        // Every cycle width this app accepts, checked for both halves of the
        // rule: no spurious decimal, and at most one.
        for inhale in 3..=MAX_INHALE_SECONDS {
            for exhale in 3..=MAX_EXHALE_SECONDS {
                let settings = Settings {
                    inhale_seconds: inhale,
                    exhale_seconds: exhale,
                };
                let label = settings.pace_label();
                let number = label
                    .strip_suffix(" breaths/min")
                    .unwrap_or_else(|| panic!("unexpected label {label:?}"));
                assert!(
                    !number.starts_with('.'),
                    "{label:?} has an empty integer part"
                );
                let decimals = number.split('.').count() - 1;
                assert!(decimals <= 1, "{label:?} has more than one decimal");
                let expected = 60.0 / f64::from(inhale + exhale);
                if expected.fract() == 0.0 {
                    assert_eq!(decimals, 0, "{label:?} should be whole");
                } else {
                    assert_eq!(decimals, 1, "{label:?} should carry a decimal");
                }
            }
        }
    }
}
