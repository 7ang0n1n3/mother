/// MU/TH/UR 6000 — Synthesized Sound Engine (rodio 0.22)
///
/// All sounds are generated from sine/square waves — no audio files.
/// A background thread owns the MixerDeviceSink and creates Players as needed.
use std::f32::consts::PI;
use std::num::NonZero;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, SyncSender},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rodio::{source::Source, ChannelCount, DeviceSinkBuilder, Player, SampleRate};

// Sample is f32 in rodio 0.22 (the `Sample` type alias)
type S = f32;

const SR: u32 = 48_000;

// ── DSP primitives ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Note {
    freq: f32,  // Hz; 0.0 = silence
    amp:  f32,  // 0.0–1.0
    len:  u32,  // number of samples
    sq:   bool, // square wave vs sine
}

fn ms(millis: u32) -> u32 { millis * SR / 1_000 }

fn sine(freq: f32, amp: f32, millis: u32) -> Note {
    Note { freq, amp, len: ms(millis), sq: false }
}
fn buzz(freq: f32, amp: f32, millis: u32) -> Note {
    Note { freq, amp, len: ms(millis), sq: true }
}
fn rest(millis: u32) -> Note {
    Note { freq: 0.0, amp: 0.0, len: ms(millis), sq: false }
}

// ── Sequenced tone source ────────────────────────────────────────────────────

struct ToneSeq {
    notes: Vec<Note>,
    ni:    usize, // index into notes
    si:    u32,   // sample index within current note
}

impl ToneSeq {
    fn new(notes: Vec<Note>) -> Self {
        Self { notes, ni: 0, si: 0 }
    }
}

impl Iterator for ToneSeq {
    type Item = S;

    fn next(&mut self) -> Option<S> {
        loop {
            let note = self.notes.get(self.ni)?;
            if self.si >= note.len {
                self.ni += 1;
                self.si = 0;
                continue;
            }
            let sample = if note.freq == 0.0 {
                0.0
            } else {
                let t   = self.si as f32 / SR as f32;
                let raw = (t * note.freq * 2.0 * PI).sin();
                let env = envelope(self.si, note.len);
                (if note.sq { raw.signum() * 0.6 } else { raw }) * note.amp * env
            };
            self.si += 1;
            return Some(sample);
        }
    }
}

/// Short linear attack/release to eliminate pops on note boundaries
fn envelope(si: u32, len: u32) -> f32 {
    let fade = (SR / 2_000).min(len / 4).max(1);
    let tail = len.saturating_sub(fade);
    if si < fade {
        si as f32 / fade as f32
    } else if si >= tail {
        (len - si) as f32 / fade as f32
    } else {
        1.0
    }
}

impl Source for ToneSeq {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self)        -> ChannelCount   { NonZero::new(1).unwrap() }
    fn sample_rate(&self)     -> SampleRate     { NonZero::new(SR).unwrap() }
    fn total_duration(&self)  -> Option<Duration> { None }
}

// ── Sound library ────────────────────────────────────────────────────────────

fn boot()         -> Vec<Note> { vec![
    sine(440.0, 0.22, 100), rest(35),
    sine(550.0, 0.24, 100), rest(35),
    sine(660.0, 0.26, 120), rest(35),
    sine(880.0, 0.20,  80),
]}

fn select()       -> Vec<Note> { vec![sine(800.0, 0.10, 14)] }

fn input_mode()   -> Vec<Note> { vec![
    sine(660.0, 0.16, 45), rest(12),
    sine(880.0, 0.12, 35),
]}

fn keypress()     -> Vec<Note> { vec![sine(2200.0, 0.055, 7)] }

fn scan_start()   -> Vec<Note> { vec![
    sine(440.0, 0.22,  70), rest(18),
    sine(660.0, 0.26, 100), rest(18),
    sine(880.0, 0.22,  85),
]}

fn output_tick()  -> Vec<Note> { vec![sine(1400.0, 0.038, 5)] }

fn complete()     -> Vec<Note> { vec![
    sine(660.0, 0.22,  80), rest(25),
    sine(550.0, 0.20,  80), rest(25),
    sine(440.0, 0.25, 140),
]}

fn error()        -> Vec<Note> { vec![
    buzz(185.0, 0.28,  80), rest(18),
    buzz(165.0, 0.26, 110),
]}

fn cancel()       -> Vec<Note> { vec![
    sine(550.0, 0.16, 50), rest(10),
    sine(440.0, 0.14, 60),
]}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct SoundEngine {
    tx:           Option<SyncSender<Vec<Note>>>,
    last_tick_ms: AtomicU64,
    muted:        AtomicBool,
}

impl SoundEngine {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Vec<Note>>(128);

        std::thread::Builder::new()
            .name("mu-th-ur-audio".into())
            .spawn(move || {
                // Open the default audio output — silently return if unavailable
                let mut device_sink = match DeviceSinkBuilder::open_default_sink() {
                    Ok(d) => d,
                    Err(_) => return,
                };
                // Suppress the "dropped" log message rodio 0.22 prints by default
                device_sink.log_on_drop(false);

                let mixer = device_sink.mixer().clone();

                loop {
                    match rx.recv() {
                        Ok(notes) => {
                            let player = Player::connect_new(&mixer);
                            player.append(ToneSeq::new(notes));
                            player.detach();
                        }
                        Err(_) => break, // sender dropped → quit
                    }
                }
            })
            .ok();

        Self {
            tx: Some(tx),
            last_tick_ms: AtomicU64::new(0),
            muted: AtomicBool::new(false),
        }
    }

    fn send(&self, notes: Vec<Note>) {
        if self.muted.load(Ordering::Relaxed) { return; }
        if let Some(tx) = &self.tx {
            tx.try_send(notes).ok();
        }
    }

    pub fn toggle_mute(&self) {
        let was = self.muted.load(Ordering::Relaxed);
        self.muted.store(!was, Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    // ── Events ───────────────────────────────────────────────────────────────

    pub fn boot(&self)          { self.send(boot()) }
    pub fn select(&self)        { self.send(select()) }
    pub fn input_mode(&self)    { self.send(input_mode()) }
    pub fn keypress(&self)      { self.send(keypress()) }
    pub fn scan_start(&self)    { self.send(scan_start()) }
    pub fn scan_complete(&self) { self.send(complete()) }
    pub fn error(&self)         { self.send(error()) }
    pub fn cancel(&self)        { self.send(cancel()) }

    /// Throttled tick — at most once per 60 ms
    pub fn output_tick(&self) {
        let now  = now_ms();
        let last = self.last_tick_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) < 60 { return; }
        self.last_tick_ms.store(now, Ordering::Relaxed);
        self.send(output_tick());
    }
}

impl Default for SoundEngine {
    fn default() -> Self { Self::new() }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
