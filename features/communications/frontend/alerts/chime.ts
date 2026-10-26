// The chime of a new message, made as it plays (Web Audio): two soft bell notes a fifth apart.
// A browser plays sound only once the page has been used, so the sound starts on the member's
// first click or key in it, and rests between chimes.

let context: AudioContext | undefined;

/** The notes, each from when the chime starts, in seconds, and its pitch. */
const NOTES = [
  { at: 0, hz: 880 },
  { at: 0.13, hz: 1318.5 },
];
/** How long a note rings, in seconds. */
const RING = 0.9;
/** How long the sound may take to start before the chime is skipped, in milliseconds. */
const START_MS = 1_000;

/** The sound, ready to play once the page is first used. */
export function readyOnUse() {
  const used = () => {
    context ??= new AudioContext();
    void context.resume().then(() => context?.suspend());
    stop();
  };
  const stop = () => {
    removeEventListener('pointerdown', used, true);
    removeEventListener('keydown', used, true);
  };
  addEventListener('pointerdown', used, true);
  addEventListener('keydown', used, true);
  return stop;
}

/** Plays the chime, unless the browser won't play sound in the page yet. */
export async function chime() {
  if (!context) {
    if (navigator.userActivation && !navigator.userActivation.hasBeenActive) return;
    context = new AudioContext();
  }
  const sound = context;
  await Promise.race([sound.resume(), new Promise((settle) => setTimeout(settle, START_MS))]).catch(
    () => undefined,
  );
  if (sound.state !== 'running') return;
  const start = sound.currentTime + 0.02;
  const volume = sound.createGain();
  volume.gain.value = 0.16;
  volume.connect(sound.destination);
  // Each note with a faint overtone above it, as a bell has.
  const tones = NOTES.flatMap(({ at, hz }) =>
    [
      [hz, 1],
      [hz * 2.76, 0.12],
    ].map(([pitch, loud]) => {
      const tone = sound.createOscillator();
      tone.frequency.value = pitch!;
      const ring = sound.createGain();
      ring.gain.setValueAtTime(0.0001, start + at);
      ring.gain.exponentialRampToValueAtTime(loud!, start + at + 0.01);
      ring.gain.exponentialRampToValueAtTime(0.0001, start + at + RING);
      tone.connect(ring).connect(volume);
      tone.start(start + at);
      tone.stop(start + at + RING);
      return tone;
    }),
  );
  // At rest again once the last has rung.
  tones.at(-1)!.onended = () => void sound.suspend();
}
