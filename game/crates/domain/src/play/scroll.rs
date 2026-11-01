//! ノーツの流れる位置（DOM-SPD-*）。LR2 の HS-FIX（`speedmultiplier`）と位置（`renderTiming`）。
//! OpenLR2 の `LR2_bmsload.cpp`（`speedmultiplier`・`renderTiming` の計算）と `Scene04_Play.cpp`（`DrawNotes`）を
//! 仕様として参照した（コードは転載していない）。表示は 位置の差 × ハイスピード × 倍率 / 600 ピクセル。
use super::timing::DEFAULT_BPM;
use super::TimingMap;
use crate::chart::{Beat, Chart, NoteKind};

/// HS-FIX（LR2 の `hstype`）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HsFix {
    #[default]
    Off,
    MaxBpm,
    MinBpm,
    Average,
    /// BPM に関係なく時間で流す
    Constant,
    MainBpm,
}

/// 1 拍の位置の長さ（LR2 の `renderTiming` は 1 小節 = 1920）
pub const UNITS_PER_BEAT: f64 = 480.0;
/// `Constant` の 1 ms の位置の長さ
const UNITS_PER_MS: f64 = 1.2;

/// DOM-SPD-01: HS-FIX の倍率。OFF・CONSTANT は 1、それ以外は 150 / BPM。BPM はノーツ（通常・LN の始点）の位置の BPM で、
/// MAX / MIN はその最大 / 最小（初期 BPM から始める）、AVERAGE はノーツごとの平均、MAIN はノーツ数の最も多い BPM
/// （同数なら先に現れたもの）。ノーツが無ければ AVERAGE・MAIN は 1
pub fn speed_multiplier(chart: &Chart, timing: &TimingMap, fix: HsFix) -> f64 {
    let initial = chart
        .meta
        .initial_bpm
        .filter(|b| *b > 0.0)
        .unwrap_or(DEFAULT_BPM);
    let mut notes: Vec<(i64, f64)> = chart
        .notes
        .iter()
        .filter(|n| matches!(n.kind, NoteKind::Normal | NoteKind::Long { .. }))
        .map(|n| {
            let t = timing.time_of(n.beat);
            (t, timing.bpm_at(t))
        })
        .collect();
    notes.sort_by_key(|n| n.0);
    let bpms: Vec<f64> = notes.into_iter().map(|n| n.1).collect();
    let bpm = match fix {
        HsFix::Off | HsFix::Constant => return 1.0,
        HsFix::MaxBpm => bpms.iter().fold(initial, |a, b| a.max(*b)),
        HsFix::MinBpm => bpms.iter().fold(initial, |a, b| a.min(*b)),
        HsFix::Average if bpms.is_empty() => return 1.0,
        HsFix::Average => bpms.iter().sum::<f64>() / bpms.len() as f64,
        HsFix::MainBpm => {
            let mut counts: Vec<(f64, usize)> = vec![];
            for b in &bpms {
                match counts.iter_mut().find(|c| c.0 == *b) {
                    Some(c) => c.1 += 1,
                    None => counts.push((*b, 1)),
                }
            }
            let mut best: Option<(f64, usize)> = None;
            for c in counts {
                if best.is_none_or(|b| c.1 > b.1) {
                    best = Some(c);
                }
            }
            match best {
                Some((b, _)) => b,
                None => return 1.0,
            }
        }
    };
    150.0 / bpm
}

/// DOM-SPD-02: 時刻（µs）の位置。CONSTANT 以外は 拍 × 480、CONSTANT は 止まっていない経過時間（ms）× 1.2
pub fn position_at(timing: &TimingMap, fix: HsFix, t_us: i64) -> f64 {
    match fix {
        HsFix::Constant => timing.unstopped_us(t_us) / 1000.0 * UNITS_PER_MS,
        _ => timing.beat_at(t_us).0 * UNITS_PER_BEAT,
    }
}

/// 拍の位置（[`position_at`] と同じ物差し。STOP のある拍は停止を始めたところ）
pub fn position_of(timing: &TimingMap, fix: HsFix, b: Beat) -> f64 {
    match fix {
        HsFix::Constant => position_at(timing, fix, timing.time_of(b)),
        _ => b.0 * UNITS_PER_BEAT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::{ChartHash, ChartMeta, KeyMode, Lane, Note, TimingEvent};
    use rstest::rstest;

    /// 初期 BPM 120、4 拍目から 240
    fn chart(beats: &[f64]) -> Chart {
        Chart {
            hash: ChartHash {
                md5: [0; 16],
                sha256: None,
            },
            meta: ChartMeta {
                initial_bpm: Some(120.0),
                ..Default::default()
            },
            key_mode: KeyMode::K7,
            notes: beats
                .iter()
                .map(|b| Note {
                    beat: Beat(*b),
                    lane: Lane(1),
                    kind: NoteKind::Normal,
                    sound: None,
                })
                .collect(),
            bgm: vec![],
            bga: vec![],
            timing: vec![TimingEvent::Bpm {
                beat: Beat(4.0),
                bpm: 240.0,
            }],
            sounds: vec![],
            images: vec![],
        }
    }

    /// DOM-SPD-01: HS-FIX の倍率 = 150 / BPM（ノーツの位置の BPM の最大・最小・平均・最多。OFF・CONSTANT は 1）
    #[rstest]
    #[case::off(HsFix::Off, &[1.0, 2.0, 5.0], 1.0)]
    #[case::constant(HsFix::Constant, &[1.0, 2.0, 5.0], 1.0)]
    #[case::max(HsFix::MaxBpm, &[1.0, 2.0, 5.0], 0.625)]
    #[case::min(HsFix::MinBpm, &[1.0, 2.0, 5.0], 1.25)]
    #[case::average(HsFix::Average, &[1.0, 2.0, 5.0], 0.9375)]
    #[case::main(HsFix::MainBpm, &[1.0, 2.0, 5.0], 1.25)]
    #[case::main_tie_first(HsFix::MainBpm, &[5.0, 1.0], 1.25)]
    #[case::max_from_initial(HsFix::MaxBpm, &[], 1.25)]
    #[case::average_no_notes(HsFix::Average, &[], 1.0)]
    #[case::main_no_notes(HsFix::MainBpm, &[], 1.0)]
    fn dom_spd_01_multiplier(#[case] fix: HsFix, #[case] beats: &[f64], #[case] want: f64) {
        let c = chart(beats);
        assert_eq!(speed_multiplier(&c, &TimingMap::from_chart(&c), fix), want);
    }

    /// DOM-SPD-02: 位置。CONSTANT 以外は 拍 × 480、CONSTANT は止まっていない経過時間（ms）× 1.2
    #[rstest]
    #[case::beats(HsFix::Off, 1_000_000, 960.0)]
    #[case::beats_after_change(HsFix::MaxBpm, 2_250_000, 2400.0)]
    #[case::constant(HsFix::Constant, 1_000_000, 1200.0)]
    #[case::constant_ignores_bpm(HsFix::Constant, 2_250_000, 2700.0)]
    fn dom_spd_02_position(#[case] fix: HsFix, #[case] t_us: i64, #[case] want: f64) {
        let c = chart(&[]);
        let tm = TimingMap::from_chart(&c);
        assert!((position_at(&tm, fix, t_us) - want).abs() < 1e-9);
        let b = tm.beat_at(t_us);
        assert!((position_of(&tm, fix, b) - want).abs() < 1e-9);
    }
}
