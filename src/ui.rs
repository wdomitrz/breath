// Copyright (c) 2026 Witalis Domitrz <witekdomitrz@gmail.com>
// AGPL License

//! The browser half: read the page, paint what [`crate::pacer`] decides, sound
//! the cue, and register the service worker.
//!
//! Nothing in here decides anything. The phase, the orb scale, the ring sweep,
//! the pace readout and the validation sentence all arrive already computed by
//! [`crate::pacer`]; this module's whole job is to move them into the DOM on an
//! 80 ms tick and to turn a phase *change* into a pair of oscillators.
//!
//! Compiled for `wasm32` only, so the host build and `cargo test` never pull in
//! `web-sys` and the domain tests run anywhere.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use web_sys::{
    window, AudioContext, AudioContextState, AudioNode, Element, EventTarget, HtmlButtonElement,
    HtmlElement, HtmlInputElement, OscillatorType, Performance,
};

use crate::pacer::{
    parse_whole_seconds, settings_from_json, Pacer, PersistedSettings, Phase, PhaseKind, Settings,
    Validation, STORAGE_KEY, TICK_MS,
};

/// The custom properties the shell's CSS reads, and what each one means.
const PROP_PROGRESS: &str = "--phase-progress";
/// The ring's own colour, set per phase so the arc itself says which phase.
const PROP_RING_COLOUR: &str = "--phase-colour";
/// The attribute the stylesheet keys off to drop the orb's size easing during a
/// hold.
const ATTR_HOLDING: &str = "data-holding";
/// Marks a hold box as in use rather than switched off.
const ATTR_ACTIVE: &str = "data-active";
const PROP_PHASE_COLOR: &str = "--phase-color";
const PROP_ORB_SCALE: &str = "--orb-scale";

/// The scope this app's worker is registered for.
///
/// Stated rather than inherited, and it is the one string that has to agree
/// with `src/service-worker.js`: the worker resolves its own directory the same
/// way, from `self.location`. See [`register_service_worker`] for why this is
/// not optional.
const SCOPE: &str = "./";

/// The colour a phase is drawn in, named by the CSS custom property rather than
/// by value.
///
/// This is the original's choice and it is the better one: the orb picks up the
/// light and dark variants of the palette from the stylesheet rather than from a
/// hex literal in the script, so `prefers-color-scheme` keeps working without
/// anything here knowing the dark values exist.
///
/// **A hold gets its own colour, and this is what makes it readable without a
/// label.** With only the original two, a hold would have to borrow one of them,
/// and a hold after the exhale would look exactly like the inhale about to
/// follow — the one moment where telling them apart is the entire point. The
/// hold colour is a desaturated slate between the two: unmistakably *neither*
/// moving colour, so "I am not breathing right now" is visible at a glance and
/// does not depend on reading the word under the orb.
fn phase_colour(kind: PhaseKind) -> &'static str {
    match kind {
        PhaseKind::Inhale => "var(--inhale)",
        PhaseKind::Exhale => "var(--exhale)",
        PhaseKind::HoldIn | PhaseKind::HoldOut => "var(--hold)",
    }
}

/// The live application, shared by the two duration boxes, the sound button and
/// the render timer.
///
/// `Rc<RefCell<_>>` because those four callbacks are `Fn` closures with no
/// argument to thread a handle through, and because three handles that could
/// each mutate the state need *one* handle, not three. Every borrow here is
/// taken and released inside one callback with no DOM work and no re-entry in
/// between, so the borrow flag never has anything to catch.
type Shared = Rc<RefCell<App>>;

/// Everything the page hands the pacer, held for the life of the document.
struct App {
    pacer: Pacer,
    /// The element the conic gradient reads its sweep from.
    ring: HtmlElement,
    /// The element the orb reads its colour and scale from.
    orb: HtmlElement,
    /// The word under the orb.
    phase_label: Element,
    /// The "6 breaths/min" readout.
    pace_label: Element,
    /// The validation sentence. It is never hidden or emptied out of the DOM —
    /// a live region that is removed stops announcing, and one that is emptied
    /// in place still clears.
    validation_message: Element,
    /// The inhale duration input.
    inhale_input: HtmlInputElement,
    /// The label wrapping the hold-after-inhale box, so its "off" state can be
    /// marked. Zero is a deliberate setting, not an empty field, and a user has
    /// to be able to tell that without reading the help text.
    hold_in_field: Element,
    /// The hold-after-inhale duration input.
    hold_in_input: HtmlInputElement,
    /// The exhale duration input.
    exhale_input: HtmlInputElement,
    /// The label wrapping the hold-after-exhale box.
    hold_out_field: Element,
    /// The hold-after-exhale duration input.
    hold_out_input: HtmlInputElement,
    /// The "Enable sound" button.
    sound_button: HtmlButtonElement,
    /// The document, for the active element a tick compares against.
    document: web_sys::Document,
    /// The clock, read once so no tick pays for the global lookup.
    performance: Performance,
    /// The audio context, once a gesture has created one.
    audio_context: Option<AudioContext>,
    /// Whether the button has been pressed. Sound cannot be armed any other
    /// way, because browsers require a gesture to start an audio context.
    audio_ready: bool,
    /// The phase the last cue was for. `None` until the first render, so the
    /// page does not sound a note merely for starting.
    last_phase: Option<&'static str>,
}

impl App {
    /// Build the app against the live document.
    fn start() -> Shared {
        let window = window().expect("the page has a window");
        let document = window.document().expect("the page has a document");
        let performance = window.performance().expect("the page has a clock");

        let settings = load_settings();
        let app = Rc::new(RefCell::new(App {
            pacer: Pacer::new(settings, performance.now()),
            ring: styled(&document, "breath-ring"),
            orb: styled(&document, "breath-orb"),
            phase_label: element(&document, "phase-label"),
            pace_label: element(&document, "pace-label"),
            validation_message: element(&document, "validation-message"),
            inhale_input: input(&document, "inhale-input"),
            hold_in_field: element(&document, "hold-in-field"),
            hold_in_input: input(&document, "hold-in-input"),
            exhale_input: input(&document, "exhale-input"),
            hold_out_field: element(&document, "hold-out-field"),
            hold_out_input: input(&document, "hold-out-input"),
            sound_button: button(&document, "sound-button"),
            document,
            performance,
            audio_context: None,
            audio_ready: false,
            last_phase: None,
        }));

        // The listeners are wired against this exact handle, not a copy of the
        // state that existed while they were being attached.
        app.borrow().bind_events(&app);
        app.borrow_mut().render();
        app
    }

    /// Wire the two inputs and the button.
    ///
    /// Three handlers for the life of the document, so each closure is dropped
    /// into the page's listener table and forgotten: nothing accumulates as the
    /// pacer runs. The tick below is the same trick for one timer.
    fn bind_events(&self, shared: &Shared) {
        // Both duration boxes and the button listen against this one handle,
        // which is the same state the render timer draws from. Three separate
        // copies of the app would each drift; one cannot.
        for field in [Field::Inhale, Field::HoldIn, Field::Exhale, Field::HoldOut] {
            listen_change(field.element(self).as_ref(), Rc::clone(shared), field);
        }
        listen_sound(self.sound_button.as_ref(), Rc::clone(shared));
    }

    /// Begin ticking, for as long as the page lives.
    ///
    /// The interval is deliberately never cleared: a breathing pacer has no end
    /// state, and a timer dropped when the page goes away is one that silently
    /// stopped breathing.
    fn tick_forever(shared: &Shared) {
        let app = Rc::clone(shared);
        let timer = Closure::wrap(Box::new(move || {
            // `borrow_mut`, held across the body and never across the paint
            // calls themselves, which do no re-entry.
            if let Ok(mut app) = app.try_borrow_mut() {
                app.render();
            }
        }) as Box<dyn Fn()>);
        if window()
            .expect("the page has a window")
            .set_interval_with_callback_and_timeout_and_arguments_0(
                timer.as_ref().unchecked_ref(),
                i32::try_from(TICK_MS).unwrap_or(i32::MAX),
            )
            .is_err()
        {
            // Nothing was scheduled, so the handle has to go back to being a
            // plain value or it is dropped and cancelled. Without this the
            // pacer would show one frame and then stop, silently.
            return;
        }
        timer.forget();
    }

    /// Read both duration inputs, adopt them, and begin a fresh cycle.
    ///
    /// An input the user has emptied reads as `None`, and the pattern is then
    /// left exactly as it was. That is the original's behaviour — its `NaN`
    /// propagates into a pattern that fails validation, and the last good cycle
    /// keeps running underneath the message — and it is the better of the two
    /// readings: a box being typed into should not freeze the orb.
    fn on_pattern_change(&mut self, field: Field) {
        let mut settings = self.pacer.settings;
        if let Some(seconds) = field.read(self) {
            field.write(&mut settings, seconds);
        }

        self.pacer.settings = settings;
        self.pacer.restart(self.performance.now());
        // A cue marks a phase the pacer actually ran, so the last one is
        // forgotten with the rest of the cycle. The next valid pattern re-arms
        // silently rather than striking a note for a boundary that never was.
        self.last_phase = None;

        if settings.validate().is_valid() {
            save_settings(&settings);
        }
        self.render();
    }

    /// Arm the audio, which only a gesture can do.
    fn on_sound_click(&mut self) {
        self.audio_ready = true;
        self.update_sound_button();

        if let Some(context) = self.audio_context.clone() {
            // A context the browser suspended — a backgrounded tab, an audio
            // device change — comes back silently until it is resumed.
            if context.state() == AudioContextState::Suspended {
                let _ = context.resume();
            }
            return;
        }

        match AudioContext::new() {
            Ok(context) => self.audio_context = Some(context),
            Err(error) => {
                // No Web Audio at all. The button stays live so the message can
                // be seen, and the pacer runs silently, which is all the user
                // loses.
                web_sys::console::warn_1(&JsValue::from_str(
                    "This browser has no Web Audio support.",
                ));
                web_sys::console::warn_1(&error);
            }
        }
    }

    /// The button reads "Sound enabled" once the button has been pressed, and
    /// is then disabled: there is nothing left to arm, and a second press would
    /// only resume a context that is already running.
    fn update_sound_button(&self) {
        self.sound_button
            .set_text_content(Some(if self.audio_ready {
                "Sound enabled"
            } else {
                "Enable sound"
            }));
        self.sound_button.set_disabled(self.audio_ready);
    }

    /// Paint one frame.
    fn render(&mut self) {
        let now = self.performance.now();
        let settings = self.pacer.settings;
        let validation = settings.validate();
        let phase = self.pacer.phase_at(now);

        self.paint_phase(&phase);
        self.paint_validation(validation);
        self.pace_label
            .set_text_content(Some(&settings.pace_label()));
        self.phase_label.set_text_content(Some(phase.name));
        self.sync_inputs(settings);
        self.sync_hold_fields(settings);
        self.cue_if_phase_changed(&phase, validation);
    }

    /// The ring's sweep and colour, and the orb's colour and size.
    ///
    /// The ring is also told whether the pacer is holding, because the CSS uses
    /// it to drop the orb's size transition: a hold is the absence of movement,
    /// and an orb that eases between two sizes it is not visiting reads as a
    /// slow drift rather than as stillness. Colouring the ring per phase is
    /// what lets "I am holding" be seen without reading the label.
    ///
    /// The orb's size is a CSS custom property, exactly as in the original, and
    /// under `prefers-reduced-motion: reduce` the stylesheet's transition is
    /// switched off. So no branch is needed here for that: the shell's
    /// `@media (prefers-reduced-motion: reduce) { .breath-orb { transition: none } }`
    /// is honoured by the browser, and Rust sets the same property either way.
    fn paint_phase(&self, phase: &Phase) {
        self.ring
            .style()
            .set_property(PROP_PROGRESS, &format!("{}%", phase.progress_percent))
            .ok();
        self.ring
            .style()
            .set_property(PROP_RING_COLOUR, phase_colour(phase.kind))
            .ok();
        let holding = matches!(phase.kind, PhaseKind::HoldIn | PhaseKind::HoldOut);
        self.ring
            .set_attribute(ATTR_HOLDING, if holding { "true" } else { "false" })
            .ok();
        self.orb
            .style()
            .set_property(PROP_PHASE_COLOR, phase_colour(phase.kind))
            .ok();
        self.orb
            .style()
            .set_property(PROP_ORB_SCALE, &phase.orb_scale.to_string())
            .ok();
    }

    /// Mark each hold box as in use or switched off.
    ///
    /// Purely a presentation detail, and one that earns its place: a `0` in a
    /// box looks the same whether it is a deliberate "no hold here" or a value
    /// nobody has touched yet. The dimmed state and the word "off" in the label
    /// make the difference visible, which is what lets the boxes sit on screen
    /// at all rather than hiding behind a "show advanced" toggle — this app is
    /// meant to be calm, and a toggle that hides the pattern is not.
    fn sync_hold_fields(&self, settings: Settings) {
        for (field, seconds) in [
            (&self.hold_in_field, settings.hold_after_inhale_seconds),
            (&self.hold_out_field, settings.hold_after_exhale_seconds),
        ] {
            field
                .set_attribute(ATTR_ACTIVE, if seconds > 0 { "true" } else { "false" })
                .ok();
        }
    }

    /// The validation sentence for the current pattern.
    fn paint_validation(&self, validation: Validation) {
        self.validation_message
            .set_text_content(Some(validation.message()));
    }

    /// Show each duration in its box, unless the user is typing in it.
    ///
    /// Writing to the box a caret is sitting in moves the caret, so a tick that
    /// did this unconditionally would fight every keystroke. The original skips
    /// the focused element and so does this.
    fn sync_inputs(&self, settings: Settings) {
        let focused = self.document.active_element();
        let pairs = [
            (&self.inhale_input, settings.inhale_seconds),
            (&self.hold_in_input, settings.hold_after_inhale_seconds),
            (&self.exhale_input, settings.exhale_seconds),
            (&self.hold_out_input, settings.hold_after_exhale_seconds),
        ];
        for (input, seconds) in pairs {
            if focused.as_ref() != Some(input.as_ref()) {
                input.set_value(&seconds.to_string());
            }
        }
    }

    /// Sound the cue when the phase has just changed.
    ///
    /// The first phase of a cycle is deliberately silent: no boundary has been
    /// crossed yet, and a note on load would be a click rather than a cue. Nor
    /// is one sounded while the pattern is invalid — a pacer the user has
    /// misconfigured should say so in words and wait.
    fn cue_if_phase_changed(&mut self, phase: &Phase, validation: Validation) {
        if !validation.is_valid() {
            return;
        }
        if let Some(previous) = self.last_phase {
            if previous != phase.name {
                self.play_cue(phase);
            }
        }
        self.last_phase = Some(phase.name);
    }

    /// One cue: a sine at the phase's note, doubled by a triangle an octave
    /// above it, both through one gain envelope.
    ///
    /// The envelope is exponential, which is why its ends are [`Phase::CUE_FLOOR`]
    /// rather than zero: an exponential ramp cannot reach zero, and starting
    /// there throws in the browser. It rises to [`Phase::CUE_GAIN`] in
    /// [`Phase::CUE_ATTACK_SECONDS`] and falls back over the rest of the cue, so
    /// the note fades out instead of cutting.
    fn play_cue(&self, phase: &Phase) {
        if !self.audio_ready {
            return;
        }
        let Some(context) = self.audio_context.as_ref() else {
            return;
        };
        let (Ok(oscillator), Ok(overtone), Ok(gain)) = (
            context.create_oscillator(),
            context.create_oscillator(),
            context.create_gain(),
        ) else {
            web_sys::console::warn_1(&JsValue::from_str("The audio cue could not be built."));
            return;
        };

        let frequency = phase.cue_frequency();
        let now = context.current_time();
        let end = now + Phase::CUE_DURATION_SECONDS;

        // `set_type` takes `&self` and throws on an argument a browser will not
        // accept; sine and triangle are both universally valid, so the
        // exception cannot fire here and is not worth a branch.
        oscillator.set_type(OscillatorType::Sine);
        oscillator
            .frequency()
            .set_value_at_time(frequency, now)
            .ok();
        overtone.set_type(OscillatorType::Triangle);
        overtone
            .frequency()
            .set_value_at_time(frequency * Phase::CUE_OVERTONE_RATIO, now)
            .ok();

        let envelope = gain.gain();
        envelope.set_value_at_time(Phase::CUE_FLOOR, now).ok();
        let _ = envelope
            .exponential_ramp_to_value_at_time(Phase::CUE_GAIN, now + Phase::CUE_ATTACK_SECONDS);
        let _ = envelope.exponential_ramp_to_value_at_time(Phase::CUE_FLOOR, end);

        // `OscillatorNode` and `GainNode` both extend `AudioNode`, and
        // `connect` is one method on that base interface -- exposed in
        // `web-sys` under its disambiguated name, because `AudioNode` has five
        // overloads of it and the crate generates one Rust name each.
        join(oscillator.as_ref(), gain.as_ref());
        join(overtone.as_ref(), gain.as_ref());
        join(gain.as_ref(), context.destination().as_ref());

        if oscillator.start_with_when(now).is_err() || overtone.start_with_when(now).is_err() {
            web_sys::console::warn_1(&JsValue::from_str("The audio cue could not start."));
            return;
        }
        // Stopping releases the nodes; the browser drops a stopped source's
        // connections, so nothing is kept alive by them.
        let _ = oscillator.stop_with_when(end);
        let _ = overtone.stop_with_when(end);
    }
}

/// Which duration box changed, so one handler serves both.
///
/// The original bound a single `handleInputChange` to each box that read both.
/// That is a source of a bug this is written to avoid: if it is ever called
/// once per box, the half-typed value in the *other* box is read at the same
/// moment and adopted, so a partially-edited pattern is committed and
/// persisted without the user finishing. Reading both is correct; reading the
/// changed one is what the boxes actually mean, and the render below fills in
/// the other anyway.
#[derive(Clone, Copy)]
enum Field {
    /// The inhale box.
    Inhale,
    /// The hold-after-inhale box.
    HoldIn,
    /// The exhale box.
    Exhale,
    /// The hold-after-exhale box.
    HoldOut,
}

impl Field {
    /// The element this field is bound to.
    fn element<'a>(&self, app: &'a App) -> &'a HtmlInputElement {
        match self {
            Self::Inhale => &app.inhale_input,
            Self::HoldIn => &app.hold_in_input,
            Self::Exhale => &app.exhale_input,
            Self::HoldOut => &app.hold_out_input,
        }
    }

    /// The duration this field sets, read from its own box.
    ///
    /// `None` means the box was empty, and the caller leaves that duration
    /// alone: a box being typed into must not freeze the pattern, which is the
    /// original's behaviour and the friendlier one.
    fn read(&self, app: &App) -> Option<u32> {
        parse_whole_seconds(&self.element(app).value())
    }

    /// Adopt a duration read from this field.
    fn write(&self, app: &mut Settings, seconds: u32) {
        match self {
            Self::Inhale => app.inhale_seconds = seconds,
            Self::HoldIn => app.hold_after_inhale_seconds = seconds,
            Self::Exhale => app.exhale_seconds = seconds,
            Self::HoldOut => app.hold_after_exhale_seconds = seconds,
        }
    }
}

/// Fetch one element by id, or fail the whole start with a clear message.
///
/// The shell is committed beside this code and `tests/shell.rs` asserts these
/// ids exist, so a missing one is a broken build rather than a runtime
/// accident — but failing loudly beats a page that half works.
fn element(document: &web_sys::Document, id: &str) -> Element {
    document
        .get_element_by_id(id)
        .unwrap_or_else(|| panic!("the shell has no element with id {id:?}"))
}

/// Fetch one element by id as an [`HtmlElement`], for its style declaration.
///
/// `Element::style` does not exist in `web-sys` — `style` is on `HtmlElement`
/// and there is no `Deref` from `Element`, so the cast is explicit and is the
/// only way to reach a custom property.
fn styled(document: &web_sys::Document, id: &str) -> HtmlElement {
    element(document, id)
        .dyn_into::<HtmlElement>()
        .unwrap_or_else(|_| panic!("#{id} is not an HTML element"))
}

/// Fetch one element by id as a typed input, or fail the whole start.
fn input(document: &web_sys::Document, id: &str) -> HtmlInputElement {
    element(document, id)
        .dyn_into::<HtmlInputElement>()
        .unwrap_or_else(|_| panic!("#{id} is not an input"))
}

/// Fetch one element by id as a typed button, or fail the whole start.
fn button(document: &web_sys::Document, id: &str) -> HtmlButtonElement {
    element(document, id)
        .dyn_into::<HtmlButtonElement>()
        .unwrap_or_else(|_| panic!("#{id} is not a button"))
}

/// Join two audio nodes, ignoring the exception a browser raises for a
/// connection it will not accept.
fn join(from: &AudioNode, to: &AudioNode) {
    let _ = from.connect_with_audio_node(to);
}

/// Attach a handler to a target for the life of the page.
///
/// The handler is moved into JavaScript and forgotten, so it cannot be dropped
/// while the listener that references it is still live. There are three of
/// these for the whole document, and nothing here runs per frame.
fn listen(target: &EventTarget, event: &str, handler: Closure<dyn Fn()>) {
    target
        .add_event_listener_with_callback(event, handler.as_ref().unchecked_ref())
        .expect("a valid event name");
    handler.forget();
}

/// Listen for a change to one duration box.
///
/// `try_borrow_mut` rather than `borrow_mut` in both handlers: a re-entrant
/// event would otherwise panic the page outright, and the only correct response
/// to "the state is already being updated, and by definition therefore correct"
/// is to skip this event and let the one in progress finish.
fn listen_change(target: &EventTarget, shared: Shared, field: Field) {
    let handler = Closure::wrap(Box::new(move || {
        if let Ok(mut app) = shared.try_borrow_mut() {
            app.on_pattern_change(field);
        }
    }) as Box<dyn Fn()>);
    listen(target, "change", handler);
}

/// Listen for the sound button. Same re-entrancy rule as [`listen_change`].
fn listen_sound(target: &EventTarget, shared: Shared) {
    let handler = Closure::wrap(Box::new(move || {
        if let Ok(mut app) = shared.try_borrow_mut() {
            app.on_sound_click();
        }
    }) as Box<dyn Fn()>);
    listen(target, "click", handler);
}

/// Read the stored settings, falling back to the default for anything unusable.
fn load_settings() -> Settings {
    let stored = window()
        .expect("the page has a window")
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item(STORAGE_KEY).ok().flatten());
    settings_from_json(stored.as_deref())
}

/// Remember the pattern, if the browser will let us.
///
/// Storage can be full, or disabled outright in a private window; failing to
/// save a preference is no reason to stop breathing.
fn save_settings(settings: &Settings) {
    let stored = PersistedSettings::of(settings).to_json();
    let Ok(Some(storage)) = window().expect("the page has a window").local_storage() else {
        return;
    };
    if let Err(error) = storage.set_item(STORAGE_KEY, &stored) {
        web_sys::console::warn_1(&JsValue::from_str("Breathing settings could not be saved."));
        web_sys::console::warn_1(&error);
    }
}

/// Register the service worker, so the pacer opens without a network.
///
/// A failure here costs offline support and nothing else — the page is already
/// running out of `dist/` — so it is a console warning, not a visible error.
/// Registration and the stale-registration sweep below are sequenced by
/// awaiting the registration promise: the sweep must run *after* this app's own
/// registration exists, so it never mistakes the registration being made right
/// now for a stale one.
async fn register_service_worker() {
    // Both accessors are infallible in `web-sys`: `window.navigator` is on
    // every window and `navigator.serviceWorker` on every navigator. (A
    // non-secure context does make the *property* absent, where JavaScript
    // would hand back `undefined` -- but then `register` throws, which is the
    // case below, and is the only case there is.)
    let container = window()
        .expect("the page has a window")
        .navigator()
        .service_worker();

    // The scope is stated rather than inherited. Left to itself, a
    // registration's scope is the directory of the page that registered it,
    // which is right today and silently wrong the moment the app is published
    // somewhere else, or opened through a path that resolves higher up the
    // origin. A worker registered for the whole origin does not serve just its
    // own pages -- it answers for every page on that origin, including the
    // ones that have nothing to do with it. Naming the scope keeps that claim
    // as small as the app.
    let options = web_sys::RegistrationOptions::new();
    options.set_scope(SCOPE);

    // `register_with_options` returns a `Promise` and does not throw for the
    // failure that matters here — a non-secure context, where
    // `navigator.serviceWorker` is absent — so the rejection is awaited and
    // handled rather than caught.
    if let Err(error) =
        JsFuture::from(container.register_with_options("./service-worker.js", &options)).await
    {
        web_sys::console::warn_1(&JsValue::from_str("Service worker registration failed."));
        web_sys::console::warn_1(&error);
        return;
    }

    release_stale_registrations().await;
}

/// Hand this app's own URLs back to the current worker.
///
/// A service worker is a registration, and a registration outlives the page
/// that made it: it is kept by the browser, not by the tab, and it keeps
/// answering for its scope until something explicitly unregisters it. That is
/// how a page on this origin can come to be served by a worker installed for a
/// *different* page, long after the app that installed it was closed. A stale
/// registration is not corrected by a reload, by a newer version of the app, or
/// by a newer worker installing itself -- the newer worker only takes control
/// where its own scope reaches, and a wider stale one is still in the way.
///
/// So the repair is explicit: find any registration whose scope covers this
/// app's directory but is not this app's directory, and unregister it. This
/// app's own registration is left alone, and so is every other app on the
/// origin -- each is scoped to its own directory, and a sibling that never
/// covered us is not ours to remove.
///
/// Failures are ignored on purpose. This is best-effort cleanup of state this
/// app did not create, and a browser that refuses leaves the user no worse
/// off: the app still runs and still caches its own assets.
async fn release_stale_registrations() {
    let container = window()
        .expect("the page has a window")
        .navigator()
        .service_worker();
    let Ok(registrations) = JsFuture::from(container.get_registrations()).await else {
        return;
    };
    let Ok(array) = registrations.dyn_into::<js_sys::Array>() else {
        return;
    };

    // This app's own directory, as an absolute URL with a trailing slash. The
    // app is served from a subdirectory and every URL of ours is inside it.
    let Ok(home) = window().expect("the page has a window").location().href() else {
        return;
    };
    let Ok(ours) = web_sys::Url::new_with_base(&home, "./") else {
        return;
    };
    let ours = ours.href();

    for entry in array.iter() {
        let Ok(registration) = entry.dyn_into::<web_sys::ServiceWorkerRegistration>() else {
            continue;
        };
        let scope = registration.scope();
        // Leave alone any scope that is this app's own, or narrower: a sibling
        // app mounted inside this directory is legitimate and separate, and
        // nothing there can intercept us. One test, because the two cases are
        // the same one: `ours` begins with `scope`.
        if ours.starts_with(&scope) {
            continue;
        }
        // What is left is a scope that is a *strict* prefix of ours: a worker
        // that would be consulted for this app's URLs while being registered
        // for more than this app. A worker is consulted for a URL exactly when
        // its scope is a prefix of that URL, which is the test above inverted.
        //
        // Of those, only our own worker qualifies: a different app's worker
        // lives in a different directory, so unregistering it would break the
        // app it belongs to.
        let script = registration
            .active()
            .map(|worker| worker.script_url())
            .unwrap_or_default();
        if script_belongs_to_app(&script, &ours) {
            match registration.unregister() {
                Ok(promise) => {
                    let _ = JsFuture::from(promise).await;
                }
                Err(error) => {
                    web_sys::console::warn_1(&JsValue::from_str(
                        "Stale service worker could not be released.",
                    ));
                    web_sys::console::warn_1(&error);
                }
            }
        }
    }
}

/// Whether a worker script at `script` is this app's own worker, registered for
/// more of the origin than this app's directory.
///
/// A wider scope means the script sits at the root of this app's own directory
/// rather than anywhere below it: a sibling app's worker is in a sibling
/// directory and does not match. `strip_suffix`, not `trim_end_matches` — the
/// latter strips a *set of characters*, so it would happily eat a directory
/// named `...e-worker.js` and call it ours.
fn script_belongs_to_app(script: &str, ours: &str) -> bool {
    match web_sys::Url::new(script) {
        Ok(url) => url.href().strip_suffix("service-worker.js") == Some(ours),
        Err(_) => false,
    }
}

/// Start the pacer.
///
/// The page's only entry point, and the reason there is a `<script>` at all:
/// `build.rs` copies the shell that calls it byte for byte, and `tests/shell.rs`
/// asserts the call is a dynamic import of the generated bindings rather than a
/// hand-written wasm ABI.
#[wasm_bindgen(start)]
pub fn start() {
    let app = App::start();
    App::tick_forever(&app);
    // Spawned, not awaited: the pacer is already running on its own tick, and
    // offline support is not worth a page that waits on the network to breathe.
    spawn_local(register_service_worker());
}
