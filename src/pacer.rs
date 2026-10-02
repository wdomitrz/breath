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
//! A cycle is up to four phases: **inhale, hold, exhale, hold**. The holds are
//! retentions — the box-breathing and coherent-breathing rests — and either may
//! be zero, which is what a two-phase pattern is. Setting both to zero
//! reproduces the original app exactly: same phases in the same order, same
//! formulae, same ring sweep, same pace.
//!
//! What changed from the two-phase original, and why, is recorded at each of the
//! four places it could have been done differently: [`Settings::validate`],
//! [`Settings::pace_label`], [`Phase`] and [`PersistedSettings`].

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

/// The shortest hold that is a hold rather than nothing: 1 second.
///
/// Zero is always allowed and means "no hold at all" — that is the two-phase
/// pattern the app shipped for years, and it must stay reachable without
/// clearing a box. One second is the smallest value a person can actually hold
/// for on purpose, so there is nothing between.
pub const MIN_HOLD_SECONDS: u32 = 1;

/// The longest hold the app accepts, in seconds.
///
/// Twenty seconds is the working ceiling of breath-hold practice and is also
/// about two and a half times the widest inhale the app allows, which is the
/// ratio that keeps a hold from dominating its own cycle — see
/// [`MAX_HOLD_INHALE_RATIO`].
pub const MAX_HOLD_SECONDS: u32 = 20;

/// A hold may be at most this many times the inhale it follows.
///
/// This is the rule that needed the most thought, because there are two
/// plausible versions and they disagree.
///
/// The generous reading is that a hold is *only* worth having when it is
/// shorter than the inhale that filled the lungs — hold longer than you inhaled
/// and you are not doing a breath exercise, you are holding your breath. That
/// gives `hold <= inhale`, which is what box breathing actually prescribes
/// (4-4-4-4) and what every coherent-breathing variant does.
///
/// The permissive reading is `hold <= inhale * 2`, keeping the exhale rule's
/// own headroom and so treating a hold as just another exhale-shaped stretch.
///
/// Two to one is chosen. Box breathing is the pattern people are trying to
/// reach when they add a hold, and a hold capped at the inhale length makes
/// that unreachable: 4-4-4-4 would be rejected, and it is the single most
/// widely prescribed retention pattern there is. A user who wants a hold longer
/// than the inhale is doing something deliberate, and the cap that matters to
/// them is the absolute one. The consequence is that a 3-second inhale admits a
/// 6-second hold — deliberately allowed, and still short enough that the hold
/// never exceeds a third of a 15-second cycle.
pub const MAX_HOLD_INHALE_RATIO: u32 = 2;

/// The labels the four phases are shown and announced under.
///
/// Fixed strings, not an enum's `Debug`, because these are words a person
/// reads and a screen reader speaks.
pub const PHASE_INHALE: &str = "Inhale";
/// The hold after an inhale.
pub const PHASE_HOLD: &str = "Hold";
/// The label for the exhale.
pub const PHASE_EXHALE: &str = "Exhale";

/// The pattern, in whole seconds: four durations, two of them optional.
///
/// This is exactly what is persisted. The cycle's start time is *not*: a stored
/// timestamp would be meaningless across a reload, so the pacer always begins a
/// fresh cycle when the page opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Seconds spent inhaling.
    pub inhale_seconds: u32,
    /// Seconds held at the top of the inhale, breathing in but not out. Zero
    /// for none.
    #[serde(default)]
    pub hold_after_inhale_seconds: u32,
    /// Seconds spent exhaling.
    pub exhale_seconds: u32,
    /// Seconds held at the bottom of the exhale. Zero for none.
    #[serde(default)]
    pub hold_after_exhale_seconds: u32,
}

impl Settings {
    /// The pattern the app starts from: 4-7-8-1.
    ///
    /// Four in, seven held at the top, eight out, one held at the bottom —
    /// twenty seconds, three breaths a minute. This is the longest exhale the
    /// app accepts, so it is the one pattern that both uses the whole range and
    /// is the recognisable "calm down" pattern rather than a box-breathing or a
    /// sleep pattern. Choosing it as the default is a deliberate departure from
    /// the two-phase 4-in/6-out the app shipped for years; the two-phase
    /// pattern is still one keystroke away and is still what
    /// [`PersistedSettings`] reads a stored two-key entry back as, so no
    /// existing user's pattern changes — only what a first-time visitor sees.
    pub const DEFAULT: Self = Self {
        inhale_seconds: 4,
        hold_after_inhale_seconds: 7,
        exhale_seconds: 8,
        hold_after_exhale_seconds: 1,
    };

    /// The width of one full breath, in seconds: all four phases.
    pub fn cycle_seconds(&self) -> u32 {
        self.inhale_seconds
            + self.hold_after_inhale_seconds
            + self.exhale_seconds
            + self.hold_after_exhale_seconds
    }

    /// The width of the moving parts alone — inhale plus exhale, holds
    /// excluded.
    ///
    /// The two rules the original app was built around are about this, not about
    /// [`Self::cycle_seconds`]: a hold makes the cycle longer, but it does not
    /// make the breath itself longer or more lopsided. See [`Self::validate`].
    pub fn moving_seconds(&self) -> u32 {
        self.inhale_seconds + self.exhale_seconds
    }

    /// A two-phase pattern: an inhale and an exhale, no holds.
    ///
    /// Not `Default` — this is how the tests, and any caller thinking in terms
    /// of the original app, spell "4 in, 6 out" without four fields of noise.
    pub const fn moving(inhale_seconds: u32, exhale_seconds: u32) -> Self {
        Self {
            inhale_seconds,
            hold_after_inhale_seconds: 0,
            exhale_seconds,
            hold_after_exhale_seconds: 0,
        }
    }

    /// Whether either hold is set.
    pub fn has_holds(&self) -> bool {
        self.hold_after_inhale_seconds > 0 || self.hold_after_exhale_seconds > 0
    }

    /// Whether this pattern is one the pacer will run.
    ///
    /// The checks run in the original's order, with the two hold checks after
    /// both existing ones, so a user who has typed something impossible sees the
    /// message they saw before rather than a different one from the same rule
    /// set.
    ///
    /// **The two existing rules are unchanged in what they mean, and that was
    /// the decision worth getting right.** "At least 8 seconds per breath" and
    /// "exhale no more than twice the inhale" were both written for a cycle that
    /// *was* the breath. Now a cycle can be longer than the breath, because a
    /// hold sits inside it. Two readings were possible:
    ///
    /// * Read both rules against [`Self::cycle_seconds`]. The 8-second floor
    ///   then stops being a floor on the breath and becomes a floor on the whole
    ///   pattern, and the ratio rule starts comparing an exhale against a number
    ///   that includes two holds — so 3/3 with a 20-second hold "passes" the
    ///   ratio rule for the wrong reason, and a 3/6 pattern fails the 8-second
    ///   rule whenever the user happens to have typed a hold.
    /// * Read both against [`Self::moving_seconds`] — inhale plus exhale — and
    ///   let holds count only where they are actually judged.
    ///
    /// The second is chosen. Both rules are about the *breath*, and the breath
    /// is still inhale plus exhale; a hold does not make a 3-second inhale
    /// gentler, so it should not be able to satisfy a floor on how long one is.
    /// It also means every two-phase pattern validates exactly as it did before,
    /// and the three genuinely new rules — the two hold ranges and the
    /// hold-against-inhale cap — are the only ones a hold can newly fail.
    pub fn validate(&self) -> Validation {
        if self.inhale_seconds < 3 || self.inhale_seconds > MAX_INHALE_SECONDS {
            return Validation::Invalid("Inhale should be 3 to 10 seconds.");
        }

        if self.exhale_seconds < 3 || self.exhale_seconds > MAX_EXHALE_SECONDS {
            return Validation::Invalid("Exhale should be 3 to 12 seconds.");
        }

        if self.moving_seconds() < MIN_CYCLE_SECONDS {
            return Validation::Invalid("Use at least 8 seconds per breath.");
        }

        if self.exhale_seconds > self.inhale_seconds * MAX_EXHALE_INHALE_RATIO {
            return Validation::Invalid("Keep exhale no more than twice the inhale.");
        }

        // Zero is a hold that is switched off, never an error. Anything else has
        // to be a hold a person can actually hold for, and short enough that it
        // does not become the cycle.
        if self.hold_after_inhale_seconds > MAX_HOLD_SECONDS {
            return Validation::Invalid("Hold should be off or 1 to 20 seconds.");
        }

        if self.hold_after_exhale_seconds > MAX_HOLD_SECONDS {
            return Validation::Invalid("Hold should be off or 1 to 20 seconds.");
        }

        if self.hold_after_inhale_seconds > self.inhale_seconds * MAX_HOLD_INHALE_RATIO {
            return Validation::Invalid("Keep hold no more than twice the inhale.");
        }

        if self.hold_after_exhale_seconds > self.inhale_seconds * MAX_HOLD_INHALE_RATIO {
            return Validation::Invalid("Keep hold no more than twice the inhale.");
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
    /// The phase, orb scale and ring sweep at `elapsed_ms` into the cycle.
    ///
    /// The four phases are walked in order, each consuming its own duration, and
    /// a zero-length phase is stepped over rather than special-cased — which is
    /// what makes "both holds off" collapse to the original two-phase behaviour
    /// without a branch anywhere: a zero-length phase has an empty window, so a
    /// position can never fall inside it.
    ///
    /// The orb's scale is the one thing that must *not* be a per-phase formula.
    /// In the original, the inhale grows the orb from 0.74 to 1.0 and the exhale
    /// shrinks it back, so size alone already encodes "which way am I going".
    /// With a hold in the pattern, the size at the start of a hold is whatever
    /// the movement before it left: full after an inhale, at rest after an
    /// exhale. A hold is the absence of movement, so the orb stands still and
    /// only its colour changes — which is legible at a glance in a way a slowly
    /// drifting size is not.
    ///
    /// The ring follows the same rule, and both holds take the state the
    /// movement before them ended on: full through the top hold, empty through
    /// the bottom one. So during either hold the entire figure is motionless and
    /// only the colour says a hold is happening. Between an inhale and an exhale
    /// the ring is still one continuous arc — the exhale sweeps it back to
    /// empty rather than filling a second time — and the only discontinuity
    /// left is the empty ring meeting the first sliver of the next inhale.
    pub fn phase_at(&self, elapsed_ms: f64) -> Phase {
        let position = self.cycle_position_seconds(elapsed_ms);
        let mut remaining = position;

        // Inhale: the ring fills and the orb grows.
        let inhale = f64::from(self.inhale_seconds);
        if position < inhale {
            let progress = ratio(position, inhale);
            return Phase {
                progress_percent: progress * 100.0,
                orb_scale: Phase::ORB_MIN + progress * (Phase::ORB_MAX - Phase::ORB_MIN),
                ..Phase::INHALE
            };
        }
        remaining -= inhale;

        // Hold after the inhale: the orb is steady and full.
        let hold_in = f64::from(self.hold_after_inhale_seconds);
        if hold_in > 0.0 && remaining < hold_in {
            return Phase {
                progress_percent: 100.0,
                orb_scale: Phase::ORB_MAX,
                ..Phase::HOLD_IN
            };
        }
        remaining -= hold_in;

        // Exhale: the ring empties and the orb shrinks.
        let exhale = f64::from(self.exhale_seconds);
        if remaining < exhale {
            let progress = ratio(remaining, exhale);
            return Phase {
                // The ring sweeps *back* to empty through the exhale, so the
                // circle is one continuous arc rather than two sweeps that meet
                // abruptly at the top.
                progress_percent: (1.0 - progress) * 100.0,
                orb_scale: Phase::ORB_MAX - progress * (Phase::ORB_MAX - Phase::ORB_MIN),
                ..Phase::EXHALE
            };
        }
        remaining -= exhale;

        // Hold after the exhale: the orb is steady and at rest, and the ring
        // stays empty — which is what the exhale left it.
        //
        // It used to close the arc here, sweeping from full back to empty
        // across the hold, on the reasoning that a four-phase cycle should read
        // as one continuous sweep. That was the one place the ring moved during
        // a hold, and it made the bottom hold the only phase in which the user
        // watches a bar drain while being told to hold still — a hold that
        // empties the ring is a hold the eye reads as a slow exhale. Holding
        // still now means the whole figure is still: the orb at rest and the
        // ring at empty, both of which is what the exhale ends with.
        //
        // The cost is honest and is paid at the top of the next inhale, where
        // the ring goes from empty to its first sliver of fill. That snap was
        // previously paid by the hold instead, spread over its whole length, and
        // it is the lesser one: it happens once per cycle, at the boundary the
        // user is already watching for the next breath to start, and a sliver
        // appearing is a far smaller visual event than a bar quietly draining
        // for four seconds while the label says "Hold".
        let hold_out = f64::from(self.hold_after_exhale_seconds);
        if remaining < hold_out {
            return Phase {
                progress_percent: 0.0,
                orb_scale: Phase::ORB_MIN,
                ..Phase::HOLD_OUT
            };
        }

        // Past the last phase. Reachable only if every duration is zero, which
        // validation forbids and which the pacer must still render rather than
        // divide by zero on.
        Phase {
            progress_percent: 0.0,
            orb_scale: Phase::ORB_MIN,
            ..Phase::HOLD_OUT
        }
    }
}

/// A fraction of `whole`, guarded against a zero-length phase.
///
/// A hold of zero is legal and normal; dividing by it is not. `phase_at` steps
/// over zero-length phases before calling this, so the guard is belt-and-braces
/// for the all-zero pattern the inputs can briefly produce.
fn ratio(part: f64, whole: f64) -> f64 {
    if whole > 0.0 {
        part / whole
    } else {
        0.0
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
///
/// `name` is the only thing that distinguishes a hold after the inhale from a
/// hold after the exhale, because a hold is a hold — the orb and the ring behave
/// identically. Distinguishing them in the label would mean inventing a fourth
/// and fifth word for the same visual state, and the extra vocabulary buys
/// nothing a user can see.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Phase {
    /// "Inhale", "Hold" or "Exhale" — the word under the orb, and which note
    /// sounds.
    pub name: &'static str,
    /// How much of the ring is filled, in percent.
    pub progress_percent: f64,
    /// The orb's size relative to its resting scale.
    pub orb_scale: f64,
    /// Which of the four phases this is.
    pub kind: PhaseKind,
}

/// Which of the four phases a [`Phase`] is.
///
/// Split from `name` because the ring and the orb are driven by *behaviour* and
/// the label is only ever shown, and conflating the two makes every paint site
/// match on a string. The two holds share one label but keep their own identity,
/// so the ring can tell "hold after the inhale" from "hold after the exhale"
/// even though both are called "Hold".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseKind {
    /// Lungs filling.
    Inhale,
    /// Held after the inhale, at the top.
    HoldIn,
    /// Lungs emptying.
    Exhale,
    /// Held after the exhale, at the bottom.
    HoldOut,
}

impl Phase {
    /// A hold in progress at the top of the inhale.
    const INHALE: Self = Self {
        name: PHASE_INHALE,
        progress_percent: 0.0,
        orb_scale: Self::ORB_MIN,
        kind: PhaseKind::Inhale,
    };

    /// The resting shape of a hold after the inhale.
    const HOLD_IN: Self = Self {
        name: PHASE_HOLD,
        progress_percent: 0.0,
        orb_scale: Self::ORB_MIN,
        kind: PhaseKind::HoldIn,
    };

    /// The resting shape of a hold after the exhale.
    const HOLD_OUT: Self = Self {
        name: PHASE_HOLD,
        progress_percent: 0.0,
        orb_scale: Self::ORB_MIN,
        kind: PhaseKind::HoldOut,
    };

    /// An exhale in progress.
    const EXHALE: Self = Self {
        name: PHASE_EXHALE,
        progress_percent: 0.0,
        orb_scale: Self::ORB_MAX,
        kind: PhaseKind::Exhale,
    };

    /// The orb's scale when the lungs are full.
    pub const ORB_MAX: f64 = 1.0;

    /// The orb's scale when the lungs are at rest.
    pub const ORB_MIN: f64 = 0.74;

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

    /// The frequency, in hertz, of the note that marks this phase beginning.
    ///
    /// Three distinct notes for four phases, because the two holds share one:
    ///
    /// * **Inhale** is the rising fifth, 740 Hz.
    /// * **Exhale** is its octave below, 392 Hz.
    /// * **Hold** is 587 Hz — a major third above the exhale and a fourth below
    ///   the inhale, so it sits between the two rather than repeating either.
    ///
    /// Reusing the inhale note for a hold was the obvious cheap option and it is
    /// wrong: a hold after the exhale would then sound exactly like the inhale
    /// that will follow it, and the two cues would be indistinguishable in the
    /// one situation where the user most needs to know which is which. One note
    /// for both holds is right — from the inside they are the same instruction,
    /// which is to wait — and it is not any of the other two.
    pub fn cue_frequency(&self) -> f32 {
        match self.kind {
            PhaseKind::Inhale => 740.0,
            PhaseKind::Exhale => 392.0,
            PhaseKind::HoldIn | PhaseKind::HoldOut => 587.0,
        }
    }
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
    let settings = stored.settings();
    if settings.is_valid() {
        settings
    } else {
        Settings::DEFAULT
    }
}

/// The exact shape written to `localStorage`: the four durations, nothing else.
///
/// **Both hold fields carry `#[serde(default)]`, and that is the whole reason
/// this type exists separately from [`Settings`].** Settings written by the
/// two-phase version of this app are two keys long. Deserialising that into a
/// four-field struct without a default per field fails outright, and a failed
/// read means falling back to the default pattern — so every existing user's
/// carefully chosen 4/6 would silently become the default the first time they
/// opened the app after this update. With the defaults, a two-key entry reads
/// as two-key-plus-two-zeros, which is exactly the pattern it describes, and
/// nothing about anyone's setup changes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PersistedSettings {
    /// Seconds spent inhaling.
    pub inhale_seconds: u32,
    /// Seconds held after the inhale. Absent in settings written before holds
    /// existed, which means none.
    #[serde(default)]
    pub hold_after_inhale_seconds: u32,
    /// Seconds spent exhaling.
    pub exhale_seconds: u32,
    /// Seconds held after the exhale. Absent means none, for the same reason.
    #[serde(default)]
    pub hold_after_exhale_seconds: u32,
}

impl PersistedSettings {
    /// The settings to remember for `settings`.
    pub fn of(settings: &Settings) -> Self {
        Self {
            inhale_seconds: settings.inhale_seconds,
            hold_after_inhale_seconds: settings.hold_after_inhale_seconds,
            exhale_seconds: settings.exhale_seconds,
            hold_after_exhale_seconds: settings.hold_after_exhale_seconds,
        }
    }

    /// The live settings this describes.
    pub fn settings(&self) -> Settings {
        Settings {
            inhale_seconds: self.inhale_seconds,
            hold_after_inhale_seconds: self.hold_after_inhale_seconds,
            exhale_seconds: self.exhale_seconds,
            hold_after_exhale_seconds: self.hold_after_exhale_seconds,
        }
    }

    /// The JSON to write to `localStorage`.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("four integers serialise")
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

    /// 4 in, 6 out: the two-phase pattern, written out rather than reached
    /// through `Settings::DEFAULT`.
    ///
    /// It used to be the default, so a lot of tests reached for `DEFAULT` when
    /// they wanted any simple valid pattern. Changing the default then broke
    /// eleven of them at once, none of which were about the default — a test
    /// that fails because the default moved is a test whose subject is
    /// ambiguous. Tests about the default say `DEFAULT`; every other test says
    /// which pattern it means.
    fn plain() -> Settings {
        Settings::moving(4, 6)
    }

    #[test]
    fn the_default_pattern_is_four_seven_eight_one() {
        // 4-7-8-1: the calm-down pattern, and the only one that uses the
        // widest exhale the app accepts.
        assert_eq!(Settings::DEFAULT.inhale_seconds, 4);
        assert_eq!(Settings::DEFAULT.hold_after_inhale_seconds, 7);
        assert_eq!(Settings::DEFAULT.exhale_seconds, 8);
        assert_eq!(Settings::DEFAULT.hold_after_exhale_seconds, 1);
        assert_eq!(Settings::DEFAULT.cycle_seconds(), 20);
        assert!(Settings::DEFAULT.is_valid());
    }

    #[test]
    fn a_twenty_second_cycle_is_three_breaths_a_minute() {
        // The default is a 20 s cycle, so it opens at three breaths a minute —
        // and that is a whole number, so the readout carries no decimal.
        assert_eq!(Settings::DEFAULT.pace_label(), "3 breaths/min");
    }

    #[test]
    fn the_pace_is_an_integer_when_it_can_be() {
        // 5 s and 5 s: exactly 12.
        let settings = Settings::moving(5, 5);
        assert_eq!(settings.cycle_seconds(), 10);
        // The default happens to be a whole rate too, and is asserted here so
        // the two "no trailing .0" cases are covered: an exact integer written
        // plainly, and the same value reached from a different pattern.
        assert_eq!(Settings::DEFAULT.pace_label(), format_whole(3));
        assert_eq!(settings.pace_label(), "6 breaths/min");
    }

    #[test]
    fn a_fractional_pace_carries_exactly_one_decimal() {
        let settings = Settings::moving(4, 7);
        // 60 / 11 = 5.4545…, toFixed(1) rounds to 5.5.
        assert_eq!(settings.pace_label(), "5.5 breaths/min");

        let slow = Settings::moving(9, 12);
        // 60 / 21 = 2.857…, toFixed(1) rounds to 2.9 — not 2.8.
        assert_eq!(slow.pace_label(), "2.9 breaths/min");
    }

    #[test]
    fn the_phase_opens_on_the_inhale() {
        let settings = plain();
        let phase = settings.phase_at(0.0);
        assert_eq!(phase.name, "Inhale");
        assert_eq!(phase.progress_percent, 0.0);
        assert!((phase.orb_scale - 0.74).abs() < 1e-9);
    }

    #[test]
    fn the_inhale_fills_the_ring_and_grows_the_orb() {
        let settings = plain();
        let phase = settings.phase_at(2000.0);
        assert_eq!(phase.name, "Inhale");
        assert!((phase.progress_percent - 50.0).abs() < 1e-9);
        assert!((phase.orb_scale - 0.87).abs() < 1e-9);
    }

    #[test]
    fn the_exhale_sweeps_the_ring_back_and_shrinks_the_orb() {
        let settings = plain();
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
        let settings = plain();
        // 3.999 s in is still the inhale; 4.000 s in is the exhale.
        assert_eq!(settings.phase_at(3999.0).name, "Inhale");
        assert_eq!(settings.phase_at(4000.0).name, "Exhale");
    }

    #[test]
    fn the_cycle_repeats_without_drifting() {
        let settings = plain();
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
        let settings = Settings::moving(0, 0);
        // The pacer must still render; it is not this type's job to reject a
        // pattern the input box can produce.
        let phase = settings.phase_at(1500.0);
        assert_eq!(phase.name, "Hold");
        assert!(phase.progress_percent.is_finite());
        assert!(phase.orb_scale.is_finite());
    }

    #[test]
    fn an_inhale_of_zero_is_still_the_inhale_phase() {
        let settings = Settings::moving(0, 6);
        let phase = settings.phase_at(1000.0);
        assert_eq!(phase.name, "Exhale");
        assert!(phase.orb_scale.is_finite());
    }

    #[test]
    fn each_phase_cue_is_its_own_note() {
        assert_eq!(
            plain().phase_at(0.0).cue_frequency(),
            740.0,
            "inhale is 740 Hz"
        );
        assert_eq!(
            plain().phase_at(5000.0).cue_frequency(),
            392.0,
            "exhale is 392 Hz"
        );
    }

    #[test]
    fn validation_reports_the_first_rule_that_fails() {
        let cases = [
            (Settings::moving(2, 6), "Inhale should be 3 to 10 seconds."),
            (Settings::moving(11, 6), "Inhale should be 3 to 10 seconds."),
            (Settings::moving(4, 2), "Exhale should be 3 to 12 seconds."),
            (Settings::moving(4, 13), "Exhale should be 3 to 12 seconds."),
            (Settings::moving(4, 3), "Use at least 8 seconds per breath."),
            (Settings::moving(3, 4), "Use at least 8 seconds per breath."),
            (
                Settings::moving(3, 7),
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
            Settings::moving(3, 5),
            Settings::moving(10, 12),
            Settings::moving(3, 6),
            Settings::moving(6, 12),
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
            Settings::moving(3, 3).validate().message(),
            "Use at least 8 seconds per breath."
        );
        // One more second on the exhale clears it.
        assert!(Settings::moving(3, 5).is_valid());
    }

    #[test]
    fn a_restart_starts_the_cycle_from_its_first_beat() {
        let mut pacer = Pacer::new(plain(), 1000.0);
        assert_eq!(pacer.phase_at(5000.0).name, "Exhale");
        pacer.restart(5000.0);
        assert_eq!(pacer.phase_at(5000.0).name, "Inhale");
        assert_eq!(pacer.elapsed_ms(5000.0), 0.0);
    }

    #[test]
    fn elapsed_never_runs_backwards() {
        let pacer = Pacer::new(plain(), 5000.0);
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
            hold_after_inhale_seconds: 4,
            exhale_seconds: 7,
            hold_after_exhale_seconds: 0,
        };
        let json = PersistedSettings::of(&settings).to_json();
        assert_eq!(
            json,
            r#"{"inhale_seconds":5,"hold_after_inhale_seconds":4,"exhale_seconds":7,"hold_after_exhale_seconds":0}"#
        );
        assert_eq!(settings_from_json(Some(&json)), settings);
    }

    #[test]
    fn the_stored_shape_holds_nothing_but_the_durations() {
        // The cycle start time is runtime state and must not be persisted, so
        // the serialised form can only ever be these four keys.
        let json = PersistedSettings::of(&Settings::DEFAULT).to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let object = parsed.as_object().expect("an object");
        assert_eq!(object.len(), 4);
        for key in [
            "inhale_seconds",
            "hold_after_inhale_seconds",
            "exhale_seconds",
            "hold_after_exhale_seconds",
        ] {
            assert!(object.contains_key(key), "{key} is not persisted");
        }
        assert!(!json.contains("cycleStartedAt"));
    }

    #[test]
    fn settings_written_before_holds_existed_are_read_as_no_holds() {
        // The whole reason `PersistedSettings` exists. An entry written by the
        // two-phase version of this app is two keys long; if that failed to
        // parse, every existing user's pattern would silently become the default
        // the first time they opened the app after this update.
        let two_phase = r#"{"inhale_seconds":5,"exhale_seconds":9}"#;
        assert_eq!(
            settings_from_json(Some(two_phase)),
            Settings::moving(5, 9),
            "a two-key entry must read as that pattern with no holds"
        );

        // Including one that is *not* the default, so a fallback would show up
        // rather than coincide.
        let slow = r#"{"inhale_seconds":7,"exhale_seconds":11}"#;
        assert_eq!(settings_from_json(Some(slow)), Settings::moving(7, 11));

        // A missing non-default field is still a missing non-default field, and
        // falls back exactly as it always did.
        assert_eq!(
            settings_from_json(Some(r#"{"inhale_seconds":4}"#)),
            Settings::DEFAULT,
            "an incomplete entry falls back, as it always did"
        );
    }

    #[test]
    fn a_hold_survives_a_reload() {
        let settings = Settings {
            inhale_seconds: 4,
            hold_after_inhale_seconds: 4,
            exhale_seconds: 4,
            hold_after_exhale_seconds: 4,
        };
        let json = PersistedSettings::of(&settings).to_json();
        assert_eq!(settings_from_json(Some(&json)), settings);
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
        let settings = Settings::moving(5, 9);
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

    /// Box breathing: 4 in, 4 hold, 4 out, 4 hold. The pattern holds were added
    /// for, and the one that is most often got wrong.
    fn boxed() -> Settings {
        Settings {
            inhale_seconds: 4,
            hold_after_inhale_seconds: 4,
            exhale_seconds: 4,
            hold_after_exhale_seconds: 4,
        }
    }

    /// Coherent breathing with a bottom hold: 5 in, 7 out, 4 s held at the
    /// bottom and no hold at the top — the commonest real pattern where the
    /// bottom hold is the only one, and the one that must not sweep either.
    fn bottom_hold_only() -> Settings {
        Settings {
            inhale_seconds: 5,
            hold_after_inhale_seconds: 0,
            exhale_seconds: 7,
            hold_after_exhale_seconds: 4,
        }
    }

    #[test]
    fn both_holds_off_reproduces_the_two_phase_app_exactly() {
        // The backwards-compatibility guarantee, asserted rather than assumed.
        // Every observable of the original at every instant of a 4/6 cycle.
        //
        // The default is no longer 4/6, and that is exactly why this test can no
        // longer be written as "the default equals the two-phase pattern": that
        // equality was the bug, not the guarantee. The guarantee is that a
        // pattern with both holds off behaves identically whether it came from
        // the default, from `moving()`, or from typed input — all three are the
        // same four numbers, so they are the same pattern.
        let by_default = plain();
        let by_moving_helper = Settings::moving(4, 6);
        let by_holds_off = Settings {
            inhale_seconds: 4,
            hold_after_inhale_seconds: 0,
            exhale_seconds: 6,
            hold_after_exhale_seconds: 0,
        };
        assert_eq!(by_default, by_holds_off);
        assert_eq!(by_moving_helper, by_holds_off);
        assert!(!by_default.has_holds());
        assert_eq!(by_default.cycle_seconds(), 10);
        assert_eq!(by_default.pace_label(), "6 breaths/min");
        assert!(by_default.is_valid());

        for tenth in 0..1000 {
            let at = f64::from(tenth) * 10.0;
            let (a, b) = (by_default.phase_at(at), by_holds_off.phase_at(at));
            assert_eq!(a, b, "at {at} ms");
        }
    }

    #[test]
    fn a_hold_of_zero_is_never_a_phase() {
        // The four-phase walk steps over an empty window, so a hold that is off
        // contributes no instant at all — there is no flash of "Hold".
        for (at, expected) in [
            (0.0, "Inhale"),
            (3999.0, "Inhale"),
            (4000.0, "Exhale"),
            (9999.0, "Exhale"),
        ] {
            assert_eq!(plain().phase_at(at).name, expected, "at {at} ms");
        }

        // Only the second hold set, so the first is empty and the second is not.
        let second_only = Settings {
            inhale_seconds: 4,
            hold_after_inhale_seconds: 0,
            exhale_seconds: 4,
            hold_after_exhale_seconds: 4,
        };
        assert_eq!(second_only.phase_at(4000.0).name, "Exhale");
        assert_eq!(second_only.phase_at(8000.0).name, "Hold");
        assert_eq!(second_only.cycle_seconds(), 12);
    }

    #[test]
    fn a_cycle_walks_all_four_phases_in_order() {
        // Box breathing, one second at a time: 4 in, 4 hold, 4 out, 4 hold.
        let settings = boxed();
        assert_eq!(settings.cycle_seconds(), 16);
        assert!(settings.has_holds());

        let expected = [
            (0, "Inhale"),
            (1000, "Inhale"),
            (4000, "Hold"),
            (7000, "Hold"),
            (8000, "Exhale"),
            (11000, "Exhale"),
            (12000, "Hold"),
            (15000, "Hold"),
            // And straight back round to the first phase again.
            (16000, "Inhale"),
        ];
        for (ms, name) in expected {
            assert_eq!(
                settings.phase_at(f64::from(ms)).name,
                name,
                "at {ms} ms of a 16 s box breath"
            );
        }
    }

    #[test]
    fn the_phase_boundaries_are_exact() {
        // One millisecond either side of each of the three boundaries. A hold
        // that started a millisecond early, or ended a millisecond late, would
        // be a visible stutter in the orb.
        let settings = boxed();
        let boundaries = [
            (4000, "Inhale", "Hold"),
            (8000, "Hold", "Exhale"),
            (12000, "Exhale", "Hold"),
        ];
        for (at, before, after) in boundaries {
            assert_eq!(
                settings.phase_at(f64::from(at) - 1.0).name,
                before,
                "before {at}"
            );
            assert_eq!(settings.phase_at(f64::from(at)).name, after, "at {at}");
        }
    }

    #[test]
    fn only_the_first_hold_set_is_enough_to_have_a_hold() {
        let only_first = Settings {
            inhale_seconds: 4,
            hold_after_inhale_seconds: 7,
            exhale_seconds: 6,
            hold_after_exhale_seconds: 0,
        };
        assert!(only_first.has_holds());
        assert_eq!(only_first.cycle_seconds(), 17);
        assert_eq!(only_first.phase_at(5000.0).name, "Hold");
        // Past the hold and into the exhale.
        assert_eq!(only_first.phase_at(11_000.0).name, "Exhale");
        // And no second hold at the bottom.
        assert_ne!(only_first.phase_at(16_000.0).name, "Hold");
    }

    #[test]
    fn a_hold_steady_the_orb_wherever_it_stopped() {
        // The design decision, asserted: a hold is the absence of movement, so
        // the orb keeps the size the movement before it left and only its colour
        // changes. A hold that drifted would read as a slow motion the user
        // cannot control.
        let settings = boxed();

        // Hold after the inhale: full, and still full a moment later.
        let early = settings.phase_at(5000.0);
        let late = settings.phase_at(7900.0);
        assert_eq!(early.kind, PhaseKind::HoldIn);
        assert!((early.orb_scale - Phase::ORB_MAX).abs() < 1e-9, "held full");
        assert!(
            (early.orb_scale - late.orb_scale).abs() < 1e-9,
            "and it stays full"
        );
        assert!(
            (early.progress_percent - 100.0).abs() < 1e-9,
            "the ring is full through the first hold"
        );
        assert!(
            (late.progress_percent - 100.0).abs() < 1e-9,
            "and does not drift during it"
        );

        // Hold after the exhale: at rest, and still at rest. The ring is empty
        // for the whole of it, which is what the exhale ended on — so during
        // this hold nothing on the dial moves, and the only thing that changes
        // is the colour.
        let bottom_early = settings.phase_at(12_500.0);
        let bottom_late = settings.phase_at(15_500.0);
        assert_eq!(bottom_early.kind, PhaseKind::HoldOut);
        assert!(
            (bottom_early.orb_scale - Phase::ORB_MIN).abs() < 1e-9,
            "held at rest"
        );
        assert!((bottom_early.orb_scale - bottom_late.orb_scale).abs() < 1e-9);
        assert!(
            (bottom_early.progress_percent - 0.0).abs() < 1e-9,
            "the ring is empty through the second hold"
        );
        assert!(
            (bottom_late.progress_percent - 0.0).abs() < 1e-9,
            "and does not drift during it"
        );
    }

    /// Every part of the dial a hold touches must be frozen for the whole hold.
    ///
    /// This is the test the bottom hold failed when it was still closing the
    /// ring's arc: the orb held, and the ring drained from full to empty over
    /// the four seconds the user was being told to hold still. The property is
    /// stated over the whole window rather than at two instants on purpose — a
    /// sweep is a thing that is *nearly* static at its endpoints, so sampling
    /// the boundaries is what let it through.
    #[test]
    fn no_hold_moves_anything_on_the_dial() {
        for settings in [boxed(), bottom_hold_only()] {
            // `phase_at` takes milliseconds, so every window below is computed
            // in seconds and converted once here.
            let ms_per_second = 1000.0;
            let holds = [
                (
                    PhaseKind::HoldIn,
                    f64::from(settings.inhale_seconds),
                    f64::from(settings.hold_after_inhale_seconds),
                ),
                (
                    PhaseKind::HoldOut,
                    f64::from(
                        settings.inhale_seconds
                            + settings.hold_after_inhale_seconds
                            + settings.exhale_seconds,
                    ),
                    f64::from(settings.hold_after_exhale_seconds),
                ),
            ];
            for (kind, start, len) in holds {
                if len == 0.0 {
                    continue; // a hold that is off has no window to freeze
                }
                let start = start * ms_per_second;
                let len = len * ms_per_second;
                // Sample strictly inside the window: the far end belongs to
                // whatever follows the hold. The near end is kept, because the
                // boundary is the instant the movement before it ended, and that
                // is precisely the state the hold is required to hold.
                let held = settings.phase_at(start + len / 2.0 + 1.0);
                assert_eq!(held.kind, kind, "start={start} len={len}");
                for tenth in 0..(len / 100.0) as u32 {
                    let now = settings.phase_at(start + f64::from(tenth) * 100.0);
                    assert_eq!(
                        now.kind, kind,
                        "wrong phase at +{tenth} tenth(s) of a {len} ms hold"
                    );
                    assert!(
                        (now.orb_scale - held.orb_scale).abs() < 1e-9,
                        "the orb moved during {kind:?} at +{tenth} tenth(s)"
                    );
                    assert!(
                        (now.progress_percent - held.progress_percent).abs() < 1e-9,
                        "the ring moved during {kind:?} at +{tenth} tenth(s)"
                    );
                }
            }
        }
    }

    #[test]
    fn a_hold_that_does_not_divide_evenly_still_fills_its_share() {
        // 3 in, 5 hold, 7 out, 1 hold: a 16 s cycle where nothing divides 16.
        let settings = Settings {
            inhale_seconds: 3,
            hold_after_inhale_seconds: 5,
            exhale_seconds: 7,
            hold_after_exhale_seconds: 1,
        };
        assert_eq!(settings.cycle_seconds(), 16);

        // The boundaries land where the durations say, not on round seconds.
        assert_eq!(settings.phase_at(2999.0).name, "Inhale");
        assert_eq!(settings.phase_at(3000.0).name, "Hold");
        assert_eq!(settings.phase_at(7999.0).name, "Hold");
        assert_eq!(settings.phase_at(8000.0).name, "Exhale");
        assert_eq!(settings.phase_at(14_999.0).name, "Exhale");
        assert_eq!(settings.phase_at(15_000.0).name, "Hold");
        assert_eq!(settings.phase_at(15_999.0).name, "Hold");
        assert_eq!(settings.phase_at(16_000.0).name, "Inhale");

        // The one-second bottom hold holds the ring where the exhale left it,
        // which is empty — the hold does not close the arc, so the next inhale
        // does not start at a full ring.
        assert!(
            (settings.phase_at(15_000.0).progress_percent - 0.0).abs() < 1e-9,
            "the last hold begins with the ring empty"
        );
        assert!(
            (settings.phase_at(15_999.0).progress_percent - 0.0).abs() < 1e-9,
            "and ends with it empty"
        );

        // The orb is full for the whole of the five-second hold and at rest for
        // the whole of the one-second one.
        for ms in [3000, 5000, 7000, 7999] {
            assert!(
                (settings.phase_at(f64::from(ms)).orb_scale - Phase::ORB_MAX).abs() < 1e-9,
                "full at {ms} ms"
            );
        }
        assert!(
            (settings.phase_at(15_500.0).orb_scale - Phase::ORB_MIN).abs() < 1e-9,
            "at rest at the bottom"
        );
    }

    #[test]
    fn the_ring_is_one_continuous_arc_across_a_full_cycle() {
        // Sampled densely, the ring must never jump backwards while it is
        // filling, and must return to where it started exactly where the next
        // inhale begins. This is the property that makes the moving halves of a
        // four-phase cycle read as one breath.
        //
        // The ring's *fall* now happens only in the exhale; both holds park it.
        // The test used to exempt the bottom hold from the no-backwards rule
        // precisely because that hold was sweeping, so this is where the change
        // shows: the exemption is gone and the exhale is the only falling phase.
        let settings = boxed();
        // The comparison is made *within* a phase rather than between the sample
        // before and the sample now, and that is the whole point. A between-
        // sample comparison has to tolerate one step's worth of the exhale's
        // rate, because a sample ten milliseconds before the boundary is in the
        // exhale and the next is in the hold — and a tolerance that size will
        // quietly swallow a slow drain inside a hold, which is precisely the
        // regression this change is about. (It did: a hold draining a fifth of
        // the ring passed a version of this test that allowed the slack.)
        // Within a phase there is no boundary to straddle, so the tolerance is
        // exact and nothing can hide in it.
        //
        // The exhale is the only phase that may fall, and both holds park the
        // ring: the test used to exempt the bottom hold from the no-backwards
        // rule because that hold was sweeping, and the exemption is now gone.
        let mut previous = settings.phase_at(0.0);
        for step in 1..1600 {
            let now = settings.phase_at(f64::from(step) * 10.0);
            if now.kind == previous.kind && now.kind != PhaseKind::Exhale {
                assert!(
                    now.progress_percent >= previous.progress_percent - 1e-9,
                    "the ring fell inside {:?}: {} after {} at {} ms",
                    now.kind,
                    now.progress_percent,
                    previous.progress_percent,
                    f64::from(step) * 10.0
                );
            }
            previous = now;
        }
        assert!(
            (settings.phase_at(16_000.0).progress_percent
                - settings.phase_at(0.0).progress_percent)
                .abs()
                < 1e-9,
            "one cycle returns the ring to where it started"
        );
    }

    #[test]
    fn a_hold_gets_its_own_note_that_is_neither_other_one() {
        // Reusing the inhale tone would make "hold after the exhale" sound
        // exactly like the inhale that follows it.
        let inhale = plain().phase_at(0.0).cue_frequency();
        let exhale = plain().phase_at(5000.0).cue_frequency();
        let hold_top = boxed().phase_at(5000.0).cue_frequency();
        let hold_bottom = boxed().phase_at(13_000.0).cue_frequency();

        assert_eq!(hold_top, hold_bottom, "one note for both holds");
        assert_ne!(hold_top, inhale, "a hold is not the inhale");
        assert_ne!(hold_top, exhale, "a hold is not the exhale");
        // And it sits between the two rather than above or below both, so the
        // three are heard as one scale.
        assert!(
            hold_top > exhale && hold_top < inhale,
            "587 Hz is between the exhale and the inhale"
        );
    }

    #[test]
    fn holds_are_validated_by_their_own_rules() {
        // Zero is off, never an error.
        assert!(Settings::DEFAULT.is_valid());
        assert!(boxed().is_valid());

        // Over the absolute ceiling, either hold.
        for hold in [21, 40] {
            let top = Settings {
                hold_after_inhale_seconds: hold,
                ..boxed()
            };
            assert_eq!(
                top.validate().message(),
                "Hold should be off or 1 to 20 seconds."
            );
            let bottom = Settings {
                hold_after_exhale_seconds: hold,
                ..boxed()
            };
            assert_eq!(
                bottom.validate().message(),
                "Hold should be off or 1 to 20 seconds."
            );
        }

        // Over the inhale ratio: a 4 s inhale admits 8 s, not 9.
        let too_long = Settings {
            inhale_seconds: 4,
            hold_after_inhale_seconds: 9,
            exhale_seconds: 6,
            hold_after_exhale_seconds: 0,
        };
        assert_eq!(
            too_long.validate().message(),
            "Keep hold no more than twice the inhale."
        );
        assert!(
            Settings {
                inhale_seconds: 4,
                hold_after_inhale_seconds: 8,
                exhale_seconds: 6,
                hold_after_exhale_seconds: 0,
            }
            .is_valid(),
            "exactly twice the inhale is allowed"
        );

        // The bottom hold is measured against the inhale too: with nothing to
        // compare against, the exhale would be the only candidate, and a hold
        // is not a kind of exhale. The exhale here is itself legal, so the hold
        // rule is the only one this pattern can fail -- which is the point: the
        // two rules are independent and the hold rule is not just a restatement
        // of the exhale one.
        let bottom_too_long = Settings {
            inhale_seconds: 5,
            hold_after_inhale_seconds: 0,
            exhale_seconds: 9,
            hold_after_exhale_seconds: 11,
        };
        assert_eq!(
            bottom_too_long.validate().message(),
            "Keep hold no more than twice the inhale."
        );
        assert!(
            Settings {
                inhale_seconds: 5,
                hold_after_inhale_seconds: 0,
                exhale_seconds: 9,
                hold_after_exhale_seconds: 10,
            }
            .is_valid(),
            "exactly twice the inhale is allowed at the bottom too"
        );
    }

    #[test]
    fn the_two_original_rules_measure_the_breath_not_the_pattern() {
        // A 3+3 pattern fails the 8-second rule on its own account — and a hold
        // cannot rescue it, because a hold does not make the breath longer.
        let short_breath = Settings {
            inhale_seconds: 3,
            hold_after_inhale_seconds: 0,
            exhale_seconds: 3,
            hold_after_exhale_seconds: 0,
        };
        assert_eq!(
            short_breath.validate().message(),
            "Use at least 8 seconds per breath."
        );
        let rescued_by_a_hold = Settings {
            inhale_seconds: 3,
            hold_after_inhale_seconds: 4,
            exhale_seconds: 3,
            hold_after_exhale_seconds: 0,
        };
        assert_eq!(
            rescued_by_a_hold.validate().message(),
            "Use at least 8 seconds per breath.",
            "a 6-second breath is still a 6-second breath, however long the pattern"
        );

        // And the ratio rule is about the exhale against the inhale, not against
        // a total that now happens to include two holds. A 3/6 pattern is exactly
        // as valid as it was, and stays so with holds on either end.
        let lopsided = Settings::moving(3, 6);
        assert!(lopsided.is_valid());
        assert!(Settings {
            inhale_seconds: 3,
            hold_after_inhale_seconds: 6,
            exhale_seconds: 6,
            hold_after_exhale_seconds: 0,
        }
        .is_valid());
    }

    #[test]
    fn the_pace_counts_held_breaths_as_breaths() {
        // 4/4/4/4 is sixteen seconds, so three and a half breaths a minute — and
        // that is correct: a held breath is not a breath.
        assert_eq!(boxed().cycle_seconds(), 16);
        assert_eq!(boxed().moving_seconds(), 8);
        assert_eq!(boxed().pace_label(), "3.8 breaths/min");

        // The same holds off is six breaths a minute, unchanged from before.
        assert_eq!(Settings::moving(4, 6).pace_label(), "6 breaths/min");

        // Box breathing at 4/4/4/4 with no bottom hold is a clean integer.
        let top_only = Settings {
            inhale_seconds: 4,
            hold_after_inhale_seconds: 4,
            exhale_seconds: 4,
            hold_after_exhale_seconds: 0,
        };
        assert_eq!(top_only.cycle_seconds(), 12);
        assert_eq!(top_only.pace_label(), "5 breaths/min");
    }

    #[test]
    fn the_pace_label_never_shows_a_trailing_zero_decimal() {
        // Every cycle width this app accepts, checked for both halves of the
        // rule: no spurious decimal, and at most one.
        for inhale in 3..=MAX_INHALE_SECONDS {
            for exhale in 3..=MAX_EXHALE_SECONDS {
                let settings = Settings::moving(inhale, exhale);
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
