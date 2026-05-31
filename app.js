/* global localStorage, performance, window */

(() => {
  const STORAGE_KEY = "breath-pwa-settings-v3";
  const TICK_MS = 80;
  const MIN_CYCLE_SECONDS = 8;
  const CUE_GAIN = 0.045;
  const CUE_DURATION_SECONDS = 0.14;
  const CUE_FREQUENCIES = {
    Inhale: 660,
    Exhale: 440,
  };

  const DEFAULT_STATE = {
    inhaleSeconds: 4,
    exhaleSeconds: 6,
    cycleStartedAt: performance.now(),
  };

  const elements = {
    breathRing: document.querySelector("#breath-ring"),
    breathOrb: document.querySelector("#breath-orb"),
    phaseLabel: document.querySelector("#phase-label"),
    paceLabel: document.querySelector("#pace-label"),
    inhaleInput: document.querySelector("#inhale-input"),
    exhaleInput: document.querySelector("#exhale-input"),
    validationMessage: document.querySelector("#validation-message"),
  };

  let state = loadState();
  let audioContext = null;
  let audioReady = false;
  let lastPhaseName = null;

  function loadState() {
    try {
      const storedSettings = JSON.parse(localStorage.getItem(STORAGE_KEY));
      if (!storedSettings) {
        return { ...DEFAULT_STATE };
      }

      const nextState = {
        ...DEFAULT_STATE,
        inhaleSeconds:
          storedSettings.inhaleSeconds || DEFAULT_STATE.inhaleSeconds,
        exhaleSeconds:
          storedSettings.exhaleSeconds || DEFAULT_STATE.exhaleSeconds,
      };

      return validateSettings(nextState).valid
        ? nextState
        : { ...DEFAULT_STATE };
    } catch (error) {
      console.warn("Saved breathing settings could not be loaded.", error);
      return { ...DEFAULT_STATE };
    }
  }

  function saveSettings() {
    const settings = {
      inhaleSeconds: state.inhaleSeconds,
      exhaleSeconds: state.exhaleSeconds,
    };

    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  }

  function validateSettings(settings) {
    if (
      !Number.isInteger(settings.inhaleSeconds) ||
      !Number.isInteger(settings.exhaleSeconds)
    ) {
      return { valid: false, message: "Breath timing must use whole seconds." };
    }

    if (settings.inhaleSeconds < 3 || settings.inhaleSeconds > 10) {
      return { valid: false, message: "Inhale should be 3 to 10 seconds." };
    }

    if (settings.exhaleSeconds < 3 || settings.exhaleSeconds > 12) {
      return { valid: false, message: "Exhale should be 3 to 12 seconds." };
    }

    if (getCycleSeconds(settings) < MIN_CYCLE_SECONDS) {
      return { valid: false, message: "Use at least 8 seconds per breath." };
    }

    if (settings.exhaleSeconds > settings.inhaleSeconds * 2) {
      return {
        valid: false,
        message: "Keep exhale no more than twice the inhale.",
      };
    }

    return { valid: true, message: "" };
  }

  function getCycleSeconds(settings = state) {
    return settings.inhaleSeconds + settings.exhaleSeconds;
  }

  function getElapsedInCycle() {
    const elapsedSeconds = (performance.now() - state.cycleStartedAt) / 1000;
    return elapsedSeconds % getCycleSeconds();
  }

  function getPhase() {
    const cyclePosition = getElapsedInCycle();

    if (cyclePosition < state.inhaleSeconds) {
      const progress = cyclePosition / state.inhaleSeconds;
      return {
        name: "Inhale",
        progress: progress * 100,
        color: "var(--inhale)",
        orbScale: 0.74 + progress * 0.26,
      };
    }

    const exhalePosition = cyclePosition - state.inhaleSeconds;
    const progress = exhalePosition / state.exhaleSeconds;

    return {
      name: "Exhale",
      progress: (1 - progress) * 100,
      color: "var(--exhale)",
      orbScale: 1 - progress * 0.26,
    };
  }

  function getPaceLabel() {
    const breathsPerMinute = 60 / getCycleSeconds();
    const formattedPace = Number.isInteger(breathsPerMinute)
      ? breathsPerMinute.toString()
      : breathsPerMinute.toFixed(1);

    return `${formattedPace} breaths/min`;
  }

  function render() {
    const phase = getPhase();
    const validation = validateSettings(state);

    handlePhaseCue(phase, validation.valid);
    elements.phaseLabel.textContent = phase.name;
    elements.paceLabel.textContent = getPaceLabel();
    elements.breathRing.style.setProperty(
      "--phase-progress",
      `${phase.progress}%`,
    );
    elements.breathOrb.style.setProperty("--phase-color", phase.color);
    elements.breathOrb.style.setProperty("--orb-scale", phase.orbScale);
    syncInputValue(elements.inhaleInput, state.inhaleSeconds);
    syncInputValue(elements.exhaleInput, state.exhaleSeconds);
    elements.validationMessage.textContent = validation.message;
  }

  function syncInputValue(input, value) {
    if (document.activeElement !== input) {
      input.value = value.toString();
    }
  }

  function restartCycle() {
    state.cycleStartedAt = performance.now();
    lastPhaseName = null;
  }

  function readIntegerInput(input) {
    return Number.parseInt(input.value, 10);
  }

  function handleInputChange() {
    const nextState = {
      ...state,
      inhaleSeconds: readIntegerInput(elements.inhaleInput),
      exhaleSeconds: readIntegerInput(elements.exhaleInput),
    };
    const validation = validateSettings(nextState);

    state = nextState;
    restartCycle();

    if (validation.valid) {
      saveSettings();
    }

    render();
  }

  function bindEvents() {
    [elements.inhaleInput, elements.exhaleInput].forEach((input) => {
      input.addEventListener("change", handleInputChange);
    });

    ["pointerdown", "keydown", "touchstart"].forEach((eventName) => {
      window.addEventListener(eventName, unlockAudio, { once: true });
    });
  }

  function handlePhaseCue(phase, settingsAreValid) {
    if (!settingsAreValid) {
      return;
    }

    if (lastPhaseName === null) {
      lastPhaseName = phase.name;
      return;
    }

    if (phase.name !== lastPhaseName) {
      playCue(phase.name);
      lastPhaseName = phase.name;
    }
  }

  function unlockAudio() {
    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    if (!AudioContextClass) {
      return;
    }

    if (audioContext === null) {
      audioContext = new AudioContextClass();
    }

    audioContext
      .resume()
      .then(() => {
        audioReady = true;
      })
      .catch((error) => {
        console.warn("Audio cue setup failed.", error);
      });
  }

  function playCue(phaseName) {
    if (!audioReady || audioContext === null) {
      return;
    }

    const oscillator = audioContext.createOscillator();
    const gain = audioContext.createGain();
    const now = audioContext.currentTime;

    oscillator.type = "sine";
    oscillator.frequency.setValueAtTime(CUE_FREQUENCIES[phaseName], now);
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(CUE_GAIN, now + 0.025);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + CUE_DURATION_SECONDS);

    oscillator.connect(gain);
    gain.connect(audioContext.destination);
    oscillator.start(now);
    oscillator.stop(now + CUE_DURATION_SECONDS);
  }

  function registerServiceWorker() {
    if ("serviceWorker" in navigator) {
      navigator.serviceWorker.register("./sw.js").catch((error) => {
        console.warn("Service worker registration failed.", error);
      });
    }
  }

  bindEvents();
  render();
  window.setInterval(render, TICK_MS);
  registerServiceWorker();
})();
