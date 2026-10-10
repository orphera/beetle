//! The judge of a play, run on the input thread (see `raw_input`).
//!
//! During a play the input thread owns a `LaneSession`: it judges each lane
//! key the moment it arrives and starts its key sound at once, and checks for
//! misses every few milliseconds, none of it waiting for a frame. Each judge
//! call goes to the game as a `LaneEvent`, and the game makes the same call on
//! its own copy of the engine (`JudgeEngine` is deterministic), which is what
//! it draws and scores. The game never judges these plays itself.
//!
//! Auto play and replays are not judged here; they stay on the game's frame
//! tick, as does any play when the input thread could not start.

use beetle_audio::{AudioClock, SampleTrigger};
#[cfg(test)]
use beetle_core::{parse_bms, GaugeType, JudgeWindow, Ruleset, TimingModel};
use beetle_core::{JudgeEngine, Lane, NoteType, WavId};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::calibration::judged_time;
use crate::handlers::gameplay::is_gameplay_hotkey;
use crate::input::{lane_transition, InputConfig};

/// How often the input thread checks for misses during a play.
pub const TICK_MILLIS: u32 = 4;

/// One judge call the input thread made, for the game to make on its copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JudgeCall {
    /// `handle_key_down` / `handle_key_up` on `lane` at `judge_time`; the key
    /// arrived at `audio_time`.
    Key {
        lane: Lane,
        down: bool,
        audio_time: f64,
        judge_time: f64,
    },
    /// `update_misses` at `judge_time` (`audio_time` on the clock).
    Misses { audio_time: f64, judge_time: f64 },
}

/// A judge call of the play numbered `session`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaneEvent {
    pub session: u32,
    pub call: JudgeCall,
}

/// From the game to the input thread.
pub enum LogicCommand {
    /// Judge this play from now on (in place of any other).
    Start(Box<LaneSession>),
    /// The play is over.
    End,
    Pause(bool),
    /// The judge offset changed (F8 / F9).
    Offset(f64),
}

/// Everything the input thread needs to judge one play.
pub struct LaneSession {
    id: u32,
    judge: JudgeEngine,
    bindings: InputConfig,
    clock: AudioClock,
    trigger: Option<SampleTrigger>,
    offset_ms: f64,
    paused: bool,
    held: Vec<(KeyCode, Lane)>,
    /// Hidden (freezone) samples by time, for a press with no note near.
    freezone: Vec<(f64, WavId)>,
    poor_ms: f64,
}

impl LaneSession {
    /// `judge` is a copy of the game's engine as the play starts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u32,
        judge: JudgeEngine,
        bindings: InputConfig,
        clock: AudioClock,
        trigger: Option<SampleTrigger>,
        offset_ms: f64,
        freezone: Vec<(f64, WavId)>,
        poor_ms: f64,
    ) -> Self {
        Self {
            id,
            judge,
            bindings,
            clock,
            trigger,
            offset_ms,
            paused: false,
            held: Vec::new(),
            freezone,
            poor_ms,
        }
    }

    pub fn clock(&self) -> &AudioClock {
        &self.clock
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn set_offset(&mut self, offset_ms: f64) {
        self.offset_ms = offset_ms;
    }

    /// A key that arrived at `audio_time`: judged, its sound started.
    pub fn key(&mut self, code: KeyCode, down: bool, audio_time: f64) -> Option<LaneEvent> {
        let (event, sound) = self.judge_key(code, down, audio_time)?;
        if let (Some(id), Some(trigger)) = (sound, &mut self.trigger) {
            trigger.play(id);
        }
        Some(event)
    }

    /// The miss check at `audio_time`. None while paused.
    pub fn tick(&mut self, audio_time: f64) -> Option<LaneEvent> {
        if self.paused {
            return None;
        }
        let judge_time = judged_time(audio_time, self.offset_ms);
        self.judge.update_misses(judge_time);
        Some(self.event(JudgeCall::Misses {
            audio_time,
            judge_time,
        }))
    }

    /// The judgment of a key and the sound it starts, without playing it.
    fn judge_key(
        &mut self,
        code: KeyCode,
        down: bool,
        audio_time: f64,
    ) -> Option<(LaneEvent, Option<WavId>)> {
        if is_gameplay_hotkey(code) {
            return None;
        }
        // Paused: no presses, but a release still lets go of the key.
        if self.paused {
            if !down {
                self.held.retain(|&(k, _)| k != code);
            }
            return None;
        }
        let lane = self.bindings.map_key(PhysicalKey::Code(code))?;
        lane_transition(&mut self.held, code, lane, down)?;
        let judge_time = judged_time(audio_time, self.offset_ms);
        let sound = if down {
            match self.judge.handle_key_down(lane, judge_time) {
                Some((_, wav)) => wav,
                None => transparent_sound(&self.judge, &self.freezone, self.poor_ms, judge_time),
            }
        } else {
            self.judge.handle_key_up(lane, judge_time);
            None
        };
        let call = JudgeCall::Key {
            lane,
            down,
            audio_time,
            judge_time,
        };
        Some((self.event(call), sound))
    }

    fn event(&self, call: JudgeCall) -> LaneEvent {
        LaneEvent {
            session: self.id,
            call,
        }
    }
}

#[cfg(test)]
impl LaneSession {
    /// A 7K chart at 120 BPM (2 s a measure): key 1 taps at 2.0 s and 2.5 s,
    /// a hidden sample at 3.0 s.
    pub(crate) fn for_test(id: u32) -> LaneSession {
        let chart = parse_bms(
            "#BPM 120\n#WAV01 a.wav\n#WAV02 b.wav\n#WAV09 h.wav\n#00111:01020000\n#00131:00000900\n",
        )
        .unwrap();
        let timing = TimingModel::from_chart(&chart);
        let judge = JudgeEngine::new(&chart, &timing, GaugeType::Groove, Ruleset::CN);
        let freezone = chart
            .freezone_notes
            .iter()
            .map(|&(m, f, w)| (timing.beat_to_time_seconds(m, f), w))
            .collect();
        let poor_ms = JudgeWindow::from_rank(chart.header.rank).poor_ms;
        LaneSession::new(
            id,
            judge,
            InputConfig::default(),
            AudioClock::new(48000),
            None,
            0.0,
            freezone,
            poor_ms,
        )
    }
}

/// The sound of a press that judged no note: when no note is within the
/// POOR window of `judge_time`, the hidden (freezone) sample nearest to it.
pub fn transparent_sound(
    judge: &JudgeEngine,
    freezone: &[(f64, WavId)],
    poor_ms: f64,
    judge_time: f64,
) -> Option<WavId> {
    let visible_near = judge.notes().iter().any(|n| {
        !n.is_judged
            && n.note_event.note_type != NoteType::Landmine
            && (judge_time - n.target_time_seconds).abs() * 1000.0 <= poor_ms
    });
    if visible_near {
        return None;
    }
    freezone
        .iter()
        .min_by(|a, b| {
            let (da, db) = ((judge_time - a.0).abs(), (judge_time - b.0).abs());
            da.total_cmp(&db)
        })
        .map(|&(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> LaneSession {
        LaneSession::for_test(7)
    }

    /// The first key bound to key 1.
    fn key1(s: &LaneSession) -> KeyCode {
        s.bindings.keys_for_lane(Lane::Key1)[0]
    }

    #[test]
    fn a_press_is_judged_and_sounds_its_note() {
        let mut s = session();
        let k = key1(&s);
        let (event, sound) = s.judge_key(k, true, 2.0).unwrap();
        assert_eq!(event.session, 7);
        assert_eq!(
            event.call,
            JudgeCall::Key {
                lane: Lane::Key1,
                down: true,
                audio_time: 2.0,
                judge_time: 2.0,
            }
        );
        assert_eq!(sound, Some(WavId(1)));
        assert_eq!(s.judge.score().pgreat_count, 1);
    }

    #[test]
    fn the_offset_moves_the_judge_time() {
        let mut s = session();
        s.set_offset(-20.0);
        let k = key1(&s);
        let (event, _) = s.judge_key(k, true, 2.02).unwrap();
        let JudgeCall::Key { judge_time, .. } = event.call else {
            panic!("not a key");
        };
        assert!((judge_time - 2.0).abs() < 1e-9);
    }

    #[test]
    fn auto_repeat_hotkeys_and_unbound_keys_make_no_calls() {
        let mut s = session();
        let k = key1(&s);
        assert!(s.judge_key(k, true, 1.0).is_some());
        assert!(s.judge_key(k, true, 1.1).is_none());
        assert!(s.judge_key(KeyCode::Escape, true, 1.2).is_none());
        assert!(s.judge_key(KeyCode::F12, true, 1.3).is_none());
    }

    #[test]
    fn paused_presses_are_dropped_but_releases_let_go() {
        let mut s = session();
        let k = key1(&s);
        s.judge_key(k, true, 1.0);
        s.set_paused(true);
        assert!(s.judge_key(k, false, 1.1).is_none());
        assert!(s.tick(1.2).is_none());
        s.set_paused(false);
        // Not held any more, so this is a fresh press.
        assert!(s.judge_key(k, true, 2.0).is_some());
    }

    #[test]
    fn a_press_far_from_any_note_plays_the_nearest_hidden_sample() {
        let mut s = session();
        let k = key1(&s);
        let (_, sound) = s.judge_key(k, true, 3.05).unwrap();
        assert_eq!(sound, Some(WavId(9)));
    }

    #[test]
    fn a_copy_fed_the_calls_ends_where_the_session_did() {
        let mut s = session();
        let mut mirror = s.judge.clone();
        let k = key1(&s);
        let mut calls = Vec::new();
        calls.extend(s.judge_key(k, true, 2.01).map(|(e, _)| e.call));
        calls.extend(s.judge_key(k, false, 2.1).map(|(e, _)| e.call));
        calls.extend(s.tick(3.5).map(|e| e.call));
        for call in calls {
            match call {
                JudgeCall::Key {
                    lane,
                    down: true,
                    judge_time,
                    ..
                } => {
                    mirror.handle_key_down(lane, judge_time);
                }
                JudgeCall::Key {
                    lane, judge_time, ..
                } => {
                    mirror.handle_key_up(lane, judge_time);
                }
                JudgeCall::Misses { judge_time, .. } => {
                    mirror.update_misses(judge_time);
                }
            }
        }
        assert_eq!(
            format!("{:?}", mirror.score()),
            format!("{:?}", s.judge.score())
        );
        assert_eq!(s.judge.score().miss_count, 1);
    }
}
