//! プレイのユースケース（APP-PLY-*）。物理キーの上げ下げをフレームごとのレーンの状態に直して
//! `Lr2Play` に渡し、音を `AudioOutput` に流し、描画用のノーツ位置を計算する。
use crate::ports::AudioOutput;
use domain::chart::{Chart, ChartHash, Lane};
use domain::play::{GaugeType, KeyState, Lr2Play, PlayEvent, TimingMap};
use domain::score::ScoreRecord;
use std::collections::{BTreeMap, VecDeque};

/// 物理キーの番号（どのキーかは入力側が決める）
pub type KeyId = u32;

#[derive(Debug, Default)]
struct LaneInput {
    /// 押されている物理キーの数
    held_keys: u32,
    /// まだ渡していないレーンの変化（true = 押した）
    queue: VecDeque<bool>,
    down: bool,
}

/// 物理キーの上げ下げを、フレームごとのレーンの状態にする（APP-PLY-01, 02）
pub struct KeyInput {
    map: BTreeMap<KeyId, Lane>,
    lanes: BTreeMap<Lane, LaneInput>,
}

impl KeyInput {
    /// `map` は物理キー → レーン。1 つのレーンに複数のキーを割り当ててよい（皿の左右回転など）
    pub fn new(map: impl IntoIterator<Item = (KeyId, Lane)>) -> Self {
        let map: BTreeMap<KeyId, Lane> = map.into_iter().collect();
        let lanes = map.values().map(|l| (*l, LaneInput::default())).collect();
        Self { map, lanes }
    }

    /// 1 フレーム分の物理キーの変化（起きた順。true = 押した）から各レーンの状態を作る
    pub fn frame(&mut self, events: &[(KeyId, bool)]) -> BTreeMap<Lane, KeyState> {
        for &(key, down) in events {
            let Some(lane) = self.map.get(&key) else {
                continue;
            };
            let li = self.lanes.get_mut(lane).unwrap();
            let before = li.held_keys > 0;
            li.held_keys = if down {
                li.held_keys + 1
            } else {
                li.held_keys.saturating_sub(1)
            };
            let after = li.held_keys > 0;
            if before != after {
                li.queue.push_back(after);
            }
        }
        self.lanes
            .iter_mut()
            .map(|(lane, li)| {
                let state = match li.queue.pop_front() {
                    Some(true) => KeyState::Pressed,
                    Some(false) => KeyState::Released,
                    None if li.down => KeyState::Held,
                    None => KeyState::Off,
                };
                li.down = matches!(state, KeyState::Pressed | KeyState::Held);
                (*lane, state)
            })
            .collect()
    }
}

/// 描画するノーツ（APP-PLY-04）。y は 0 = 判定ライン、1 = 表示範囲の上端
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleNote {
    pub lane: Lane,
    pub y: f64,
    /// LN の終端
    pub y_end: Option<f64>,
    pub mine: bool,
    pub held: bool,
}

pub struct PlaySession {
    play: Lr2Play,
    timing: TimingMap,
    input: KeyInput,
    hash: ChartHash,
    /// 直前のフレームの各レーンの状態
    keys: BTreeMap<Lane, KeyState>,
}

impl PlaySession {
    /// LR2 に無いゲージ種類なら `None`
    pub fn new(chart: &Chart, gauge: GaugeType, input: KeyInput) -> Option<Self> {
        let timing = TimingMap::from_chart(chart);
        let play = Lr2Play::new(chart, &timing, gauge)?;
        Some(Self {
            play,
            timing,
            input,
            hash: chart.hash,
            keys: BTreeMap::new(),
        })
    }

    pub fn play(&self) -> &Lr2Play {
        &self.play
    }

    pub fn timing(&self) -> &TimingMap {
        &self.timing
    }

    /// 直前のフレームの各レーンの状態
    pub fn keys(&self) -> &BTreeMap<Lane, KeyState> {
        &self.keys
    }

    pub fn record(&self) -> ScoreRecord {
        self.play.record(self.hash)
    }

    /// 1 フレームを進める。キー音と BGM はフレーム時刻で `audio` に渡す（APP-PLY-03）
    pub fn frame(
        &mut self,
        t_ms: i64,
        key_events: &[(KeyId, bool)],
        audio: &mut dyn AudioOutput,
    ) -> Vec<PlayEvent> {
        let keys = self.input.frame(key_events);
        let events = self
            .play
            .tick(t_ms, |l| keys.get(&l).copied().unwrap_or_default());
        self.keys = keys;
        for e in &events {
            match e {
                PlayEvent::Keysound { sound, .. } | PlayEvent::Bgm { sound } => {
                    audio.play(*sound, t_ms * 1000)
                }
                _ => {}
            }
        }
        events
    }

    /// 描画するノーツ。y = (ノーツの拍 − 今の拍) / `beats`。y ≤ 1 のものを返し、押下中の LN の始点は 0 で止める。
    /// 拍で並べるので STOP 中はノーツも止まる（APP-PLY-04）
    pub fn visible_notes(&self, t_ms: i64, beats: f64) -> Vec<VisibleNote> {
        let now = self.timing.beat_at(t_ms * 1000).0;
        let y = |b: f64| (b - now) / beats;
        self.play
            .pending_notes()
            .into_iter()
            .filter(|n| y(n.beat.0) <= 1.0)
            .map(|n| {
                let head = y(n.beat.0);
                VisibleNote {
                    lane: n.lane,
                    y: if n.held { head.max(0.0) } else { head },
                    y_end: n.end_beat.map(|e| y(e.0)),
                    mine: n.mine,
                    held: n.held,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::chart::{Beat, ChartMeta, KeyMode, Note, NoteKind, SoundId};
    use rstest::rstest;
    use KeyState::*;

    fn input() -> KeyInput {
        // キー 1 → レーン 1、キー 10 と 11 → レーン 0（皿）
        KeyInput::new([(1, Lane(1)), (10, Lane(0)), (11, Lane(0))])
    }

    /// APP-PLY-01: レーンは割り当てたキーのどれかが押されていれば押下中。割り当ての無いキーは無視
    #[rstest]
    #[case::press(vec![vec![(1, true)]], vec![(1, Pressed)])]
    #[case::hold(vec![vec![(1, true)], vec![]], vec![(1, Held)])]
    #[case::release(vec![vec![(1, true)], vec![(1, false)]], vec![(1, Released)])]
    #[case::off_after_release(vec![vec![(1, true)], vec![(1, false)], vec![]], vec![(1, Off)])]
    #[case::second_scratch_key_keeps_held(vec![vec![(10, true)], vec![(11, true)], vec![(10, false)]], vec![(0, Held)])]
    #[case::both_scratch_keys_released(vec![vec![(10, true), (11, true)], vec![(10, false), (11, false)]], vec![(0, Released)])]
    #[case::unmapped_ignored(vec![vec![(99, true)]], vec![(0, Off), (1, Off)])]
    fn app_ply_01_lanes_from_keys(
        #[case] frames: Vec<Vec<(KeyId, bool)>>,
        #[case] want: Vec<(u8, KeyState)>,
    ) {
        let mut inp = input();
        let mut last = BTreeMap::new();
        for f in &frames {
            last = inp.frame(f);
        }
        for (lane, st) in want {
            assert_eq!(last[&Lane(lane)], st, "lane {lane}");
        }
    }

    /// APP-PLY-02: 1 フレームで渡すレーンの変化は 1 回まで。同じフレームで押して離したら、離しは次のフレームに回す
    #[rstest]
    #[case::tap_in_one_frame(vec![vec![(1, true), (1, false)], vec![], vec![]], vec![Pressed, Released, Off])]
    #[case::release_and_repress(vec![vec![(1, true)], vec![(1, false), (1, true)], vec![]], vec![Pressed, Released, Pressed])]
    fn app_ply_02_one_change_per_frame(
        #[case] frames: Vec<Vec<(KeyId, bool)>>,
        #[case] want: Vec<KeyState>,
    ) {
        let mut inp = input();
        let got: Vec<KeyState> = frames.iter().map(|f| inp.frame(f)[&Lane(1)]).collect();
        assert_eq!(got, want);
    }

    fn chart(notes: Vec<Note>, bgm: Vec<(f64, u32)>) -> Chart {
        Chart {
            hash: ChartHash {
                md5: [1; 16],
                sha256: None,
            },
            meta: ChartMeta {
                initial_bpm: Some(60.0),
                ..Default::default()
            },
            key_mode: KeyMode::K7,
            notes,
            bgm: bgm
                .into_iter()
                .map(|(b, s)| (Beat(b), SoundId(s)))
                .collect(),
            bga: vec![],
            timing: vec![],
            sounds: vec![],
            images: vec![],
        }
    }
    fn note(beat: f64, kind: NoteKind, sound: u32) -> Note {
        Note {
            beat: Beat(beat),
            lane: Lane(1),
            kind,
            sound: Some(SoundId(sound)),
        }
    }

    #[derive(Default)]
    struct Rec(Vec<(u32, i64)>);
    impl AudioOutput for Rec {
        fn play(&mut self, sound: SoundId, at_us: i64) {
            self.0.push((sound.0, at_us));
        }
    }

    /// APP-PLY-03: キー音と BGM はそのフレームの時刻（µs）で AudioOutput に渡す
    #[rstest]
    #[case::keysound(&[(1000, vec![(1, true)])], vec![(5, 1_000_000)])]
    #[case::bgm(&[(2001, vec![])], vec![(7, 2_001_000)])]
    fn app_ply_03_sounds(
        #[case] frames: &[(i64, Vec<(KeyId, bool)>)],
        #[case] want: Vec<(u32, i64)>,
    ) {
        let c = chart(vec![note(1.0, NoteKind::Normal, 5)], vec![(2.0, 7)]);
        let mut s = PlaySession::new(&c, GaugeType::Groove, input()).unwrap();
        let mut audio = Rec::default();
        for (t, ev) in frames {
            s.frame(*t, ev, &mut audio);
        }
        assert_eq!(audio.0, want);
    }

    /// APP-PLY-04: 描画位置 y = (拍 − 今の拍) / 表示拍数。y ≤ 1 だけ返し、押下中の LN の始点は 0 で止める
    #[rstest]
    #[case::ahead(vec![note(3.0, NoteKind::Normal, 1)], &[], 2000, vec![(0.25, None, false)])]
    #[case::beyond_range(vec![note(9.0, NoteKind::Normal, 1)], &[], 2000, vec![])]
    #[case::at_top(vec![note(6.0, NoteKind::Normal, 1)], &[], 2000, vec![(1.0, None, false)])]
    #[case::ln_end(vec![note(3.0, NoteKind::Long { end: Beat(5.0), end_sound: None }, 1)], &[], 2000, vec![(0.25, Some(0.75), false)])]
    #[case::held_ln_pinned(vec![note(3.0, NoteKind::Long { end: Beat(5.0), end_sound: None }, 1)], &[(3000, true)], 3500, vec![(0.0, Some(0.375), true)])]
    fn app_ply_04_visible(
        #[case] notes: Vec<Note>,
        #[case] press: &[(i64, bool)],
        #[case] at: i64,
        #[case] want: Vec<(f64, Option<f64>, bool)>,
    ) {
        let c = chart(notes, vec![]);
        let mut s = PlaySession::new(&c, GaugeType::Groove, input()).unwrap();
        let mut audio = Rec::default();
        for &(t, _) in press {
            s.frame(t, &[(1, true)], &mut audio);
        }
        let got: Vec<(f64, Option<f64>, bool)> = s
            .visible_notes(at, 4.0)
            .iter()
            .map(|v| (v.y, v.y_end, v.held))
            .collect();
        assert_eq!(got, want);
    }
}
