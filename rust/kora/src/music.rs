//! The soundtrack, rendered from the game's own MIDI.
//!
//! The game ships exactly one sound: `sounds/theme.mid`, a 16-track General
//! MIDI file - 480 ticks a quarter, 515 ms a quarter, and 1273 notes over about
//! 84 seconds.  macroquad's audio plays WAV, OGG and MP3, not MIDI, and there
//! is no synthesiser to hand, so this reads the file and renders it: parse,
//! mix a simple voice per part, encode a WAV in memory and hand that over.
//!
//! It is a rendition, not the original phone's audio.  A General MIDI file says
//! which instrument to use, not what it should sound like, so what comes out is
//! this port's idea of a French horn.  The note data, the tempo and the
//! arrangement are the file's own.

/// A General MIDI file is usually 44.1 kHz, but a 2007 handset was not, and
/// half the rate halves the mixing.
pub const SAMPLE_RATE: u32 = 22050;

struct Note {
    start: f64,
    end: f64,
    key: u8,
    velocity: u8,
    program: u8,
}

enum Event {
    NoteOn { key: u8, velocity: u8 },
    NoteOff { key: u8 },
    Program(u8),
}

/// How a part is voiced: three harmonics, and an envelope.
struct Voice {
    harmonics: [f32; 3],
    /// Seconds for the note to fall to its sustain level.
    decay: f32,
    /// Level held after the decay, as a fraction of the peak.
    sustain: f32,
    /// Seconds of fade in.
    attack: f32,
}

/// The General MIDI program numbers in the file are 0 (piano), 28 (muted
/// guitar), 37 and 38 (slap and synth bass) and 61 (French horn).  Grouping by
/// family is enough to tell the parts apart.
fn voice(program: u8) -> Voice {
    match program {
        0..=7 => Voice {
            harmonics: [1.0, 0.40, 0.18],
            decay: 2.2,
            sustain: 0.05,
            attack: 0.005,
        },
        24..=31 => Voice {
            harmonics: [1.0, 0.55, 0.28],
            decay: 2.6,
            sustain: 0.04,
            attack: 0.004,
        },
        32..=39 => Voice {
            harmonics: [1.0, 0.30, 0.12],
            decay: 0.7,
            sustain: 0.60,
            attack: 0.008,
        },
        56..=63 => Voice {
            harmonics: [1.0, 0.22, 0.09],
            decay: 0.6,
            sustain: 0.80,
            attack: 0.070,
        },
        _ => Voice {
            harmonics: [1.0, 0.25, 0.08],
            decay: 1.4,
            sustain: 0.30,
            attack: 0.010,
        },
    }
}

fn u16_at(data: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([data[at], data[at + 1]])
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// A variable length quantity, midi's seven-bits-per-byte integer.
fn read_vlq(data: &[u8], mut at: usize) -> Option<(u32, usize)> {
    let mut value = 0u32;
    for _ in 0..4 {
        let byte = *data.get(at)?;
        at += 1;
        value = (value << 7) | (byte & 0x7f) as u32;
        if byte & 0x80 == 0 {
            return Some((value, at));
        }
    }
    None
}

/// Parse the file into notes, with ticks already turned into seconds.
///
/// Format 0 and 1 only: format 2 has independent sequences, which nothing here
/// uses.  Running status is honoured, since the file relies on it.
fn parse(data: &[u8]) -> Option<(Vec<Note>, f64)> {
    if data.len() < 14 || &data[0..4] != b"MThd" {
        return None;
    }
    let format = u16_at(data, 8);
    let track_count = u16_at(data, 10) as usize;
    let division = u16_at(data, 12);
    if format > 1 || division & 0x8000 != 0 || division == 0 {
        return None;
    }
    let ticks_per_quarter = division as f64;

    // Collect every track's events first: the tempo map is shared, and track 0
    // usually holds it while the notes live in the others.
    let mut tracks: Vec<Vec<(u32, Event)>> = Vec::with_capacity(track_count);
    let mut tempos: Vec<(u32, u32)> = Vec::new();
    let mut at = 14usize;

    for _ in 0..track_count {
        if data.get(at..at + 4)? != b"MTrk" {
            return None;
        }
        let length = u32_at(data, at + 4) as usize;
        let end = at.checked_add(8 + length)?;
        let mut cursor = at + 8;
        let mut tick = 0u32;
        let mut status = 0u8;
        let mut events = Vec::new();

        while cursor < end {
            let (delta, next) = read_vlq(data, cursor)?;
            cursor = next;
            tick = tick.wrapping_add(delta);

            let byte = *data.get(cursor)?;
            if byte & 0x80 != 0 {
                status = byte;
                cursor += 1;
            } else if status == 0 {
                return None;
            }

            match status {
                0xFF => {
                    let kind = *data.get(cursor)?;
                    let (len, next) = read_vlq(data, cursor + 1)?;
                    cursor = next;
                    let len = len as usize;
                    let body = data.get(cursor..cursor + len)?;
                    if kind == 0x51 && len == 3 {
                        tempos.push((tick, u32::from_be_bytes([0, body[0], body[1], body[2]])));
                    }
                    cursor += len;
                }
                0xF0 | 0xF7 => {
                    let (len, next) = read_vlq(data, cursor)?;
                    cursor = next + len as usize;
                }
                _ => {
                    match status & 0xF0 {
                        0x80 => {
                            let key = *data.get(cursor)?;
                            cursor += 2;
                            events.push((tick, Event::NoteOff { key }));
                        }
                        0x90 => {
                            let key = *data.get(cursor)?;
                            let velocity = *data.get(cursor + 1)?;
                            cursor += 2;
                            // A note on with no velocity is a note off.
                            events.push((
                                tick,
                                if velocity > 0 {
                                    Event::NoteOn { key, velocity }
                                } else {
                                    Event::NoteOff { key }
                                },
                            ));
                        }
                        0xA0 | 0xB0 | 0xE0 => cursor += 2,
                        0xC0 => {
                            let program = *data.get(cursor)?;
                            cursor += 1;
                            events.push((tick, Event::Program(program)));
                        }
                        0xD0 => cursor += 1,
                        _ => return None,
                    }
                }
            }
        }

        tracks.push(events);
        at = end;
    }

    // Default tempo is 120 bpm if the file never says.
    tempos.sort_by_key(|(tick, _)| *tick);
    let seconds_at = |tick: u32| -> f64 {
        let (mut seconds, mut last, mut rate) = (0.0f64, 0u32, 500_000u32);
        for &(at, tempo) in &tempos {
            if at >= tick {
                break;
            }
            seconds += (at - last) as f64 / ticks_per_quarter * rate as f64 / 1_000_000.0;
            last = at;
            rate = tempo;
        }
        seconds + (tick - last) as f64 / ticks_per_quarter * rate as f64 / 1_000_000.0
    };

    let mut notes = Vec::new();
    let mut end = 0.0f64;
    for events in &tracks {
        let mut program = 0u8;
        // A key can be held more than once on a part; keep the stack.
        let mut held: std::collections::HashMap<u8, Vec<(u32, u8)>> =
            std::collections::HashMap::new();
        for (tick, event) in events {
            match *event {
                Event::Program(value) => program = value,
                Event::NoteOn { key, velocity } => {
                    held.entry(key).or_default().push((*tick, velocity));
                }
                Event::NoteOff { key } => {
                    if let Some(stack) = held.get_mut(&key) {
                        if let Some((start, velocity)) = stack.pop() {
                            let note = Note {
                                start: seconds_at(start),
                                end: seconds_at(*tick),
                                key,
                                velocity,
                                program,
                            };
                            end = end.max(note.end);
                            notes.push(note);
                        }
                    }
                }
            }
        }
    }

    Some((notes, end))
}

/// The number of notes in a file, for checking a render made sense.
pub fn note_count(data: &[u8]) -> usize {
    parse(data).map_or(0, |(notes, _)| notes.len())
}

/// How long the file plays for, in seconds.
pub fn duration(data: &[u8]) -> Option<f64> {
    parse(data).map(|(_, end)| end)
}

fn envelope(voice: &Voice, elapsed: f32, total: f32) -> f32 {
    let attack = if voice.attack > 0.0 {
        (elapsed / voice.attack).min(1.0)
    } else {
        1.0
    };
    let release = ((total - elapsed) / 0.08).clamp(0.0, 1.0);
    let decay = voice.sustain + (1.0 - voice.sustain) * (-elapsed / voice.decay).exp();
    attack * release * decay
}

/// Render the whole file to 16-bit mono PCM at [`SAMPLE_RATE`].
pub fn render(data: &[u8]) -> Option<Vec<u8>> {
    let (notes, length) = parse(data)?;
    if notes.is_empty() || length <= 0.0 {
        return None;
    }

    let rate = SAMPLE_RATE as f32;
    let mut samples = vec![0.0f32; (length * rate as f64) as usize + SAMPLE_RATE as usize];

    for note in &notes {
        let voice = voice(note.program);
        let frequency = 440.0 * 2f32.powf((note.key as f32 - 69.0) / 12.0);
        let start = (note.start * rate as f64) as usize;
        let end = (note.end * rate as f64) as usize;
        let total = (end.saturating_sub(start)) as f32 / rate;
        if total <= 0.0 {
            continue;
        }
        let amplitude = note.velocity as f32 / 127.0 * 0.20;
        for offset in 0..(end - start) {
            let elapsed = offset as f32 / rate;
            let phase = std::f32::consts::TAU * frequency * elapsed;
            let mut sample = 0.0;
            for (harmonic, weight) in voice.harmonics.iter().enumerate() {
                sample += weight * (phase * (harmonic as f32 + 1.0)).sin();
            }
            let index = start + offset;
            if let Some(slot) = samples.get_mut(index) {
                *slot += sample * envelope(&voice, elapsed, total) * amplitude;
            }
        }
    }

    // Leave some headroom rather than clipping a dense passage.
    let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
    if peak > 0.0 {
        let gain = (0.85 / peak).min(8.0);
        for sample in &mut samples {
            *sample *= gain;
        }
    }

    Some(encode_wav(&samples))
}

fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_length = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_length).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_length.to_le_bytes());
    for &sample in samples {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}
