//! 拍 ⇄ 時刻（µs）の変換。BPM 変化と STOP を区間表にして持つ（DOM-TIM-01..05）。
use crate::chart::{Beat, Chart, TimingEvent};

/// `#BPM` が無い譜面の初期 BPM（BMS 仕様の既定値）
pub const DEFAULT_BPM: f64 = 130.0;

#[derive(Debug, Clone, PartialEq)]
struct Segment {
    beat: f64,
    /// この拍に着いた時刻（STOP 前）
    arrive_us: f64,
    /// この拍を発つ時刻（STOP 後）
    depart_us: f64,
    bpm: f64,
}

impl Segment {
    fn us_per_beat(&self) -> f64 {
        60_000_000.0 / self.bpm
    }
}

/// 時刻(µs)→拍、拍→時刻の変換（BPM変化・STOP込み）
#[derive(Debug, Clone, PartialEq)]
pub struct TimingMap {
    segments: Vec<Segment>,
}

impl TimingMap {
    pub fn new(initial_bpm: Option<f64>, events: &[TimingEvent]) -> Self {
        let initial = initial_bpm.filter(|b| *b > 0.0).unwrap_or(DEFAULT_BPM);
        // (拍, 種別順, 元の順) で並べる: 同じ拍では BPM → STOP（DOM-TIM-03）
        let mut evs: Vec<(f64, u8, usize, f64)> = events
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match *e {
                TimingEvent::Bpm { beat, bpm } if bpm > 0.0 => Some((beat.0, 0, i, bpm)),
                TimingEvent::Stop { beat, beats } if beats > 0.0 => Some((beat.0, 1, i, beats)),
                _ => None, // DOM-TIM-04: BPM ≤ 0 は無視。Measure は拍に織り込み済み
            })
            .collect();
        evs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

        let mut segs = vec![Segment {
            beat: 0.0,
            arrive_us: 0.0,
            depart_us: 0.0,
            bpm: initial,
        }];
        for (beat, kind, _, v) in evs {
            let last = segs.last().unwrap();
            if beat != last.beat {
                let t = last.depart_us + (beat - last.beat) * last.us_per_beat();
                let bpm = last.bpm;
                segs.push(Segment {
                    beat,
                    arrive_us: t,
                    depart_us: t,
                    bpm,
                });
            }
            let cur = segs.last_mut().unwrap();
            match kind {
                0 => cur.bpm = v,
                _ => cur.depart_us += v * cur.us_per_beat(),
            }
        }
        Self { segments: segs }
    }

    pub fn from_chart(c: &Chart) -> Self {
        Self::new(c.meta.initial_bpm, &c.timing)
    }

    fn seg_by_beat(&self, b: f64) -> &Segment {
        let i = self.segments.partition_point(|s| s.beat <= b);
        &self.segments[i.saturating_sub(1)]
    }

    /// 拍の時刻（µs）。STOP のある拍は停止開始の時刻
    pub fn time_of(&self, b: Beat) -> i64 {
        let s = self.seg_by_beat(b.0);
        let t = if b.0 <= s.beat {
            s.arrive_us + (b.0 - s.beat) * s.us_per_beat()
        } else {
            s.depart_us + (b.0 - s.beat) * s.us_per_beat()
        };
        t.round() as i64
    }

    /// 時刻（µs）の拍。停止中は停止している拍（DOM-TIM-05）
    pub fn beat_at(&self, t_us: i64) -> Beat {
        let t = t_us as f64;
        let i = self.segments.partition_point(|s| s.arrive_us <= t);
        let s = &self.segments[i.saturating_sub(1)];
        if t < s.arrive_us {
            // 0 より前（負の時刻）
            Beat(s.beat + (t - s.arrive_us) / s.us_per_beat())
        } else if t <= s.depart_us {
            Beat(s.beat)
        } else {
            Beat(s.beat + (t - s.depart_us) / s.us_per_beat())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn bpm(b: f64, v: f64) -> TimingEvent {
        TimingEvent::Bpm {
            beat: Beat(b),
            bpm: v,
        }
    }
    fn stop(b: f64, beats: f64) -> TimingEvent {
        TimingEvent::Stop {
            beat: Beat(b),
            beats,
        }
    }

    /// DOM-TIM-01: 初期 BPM で一定（未指定・0 以下は 130）
    #[rstest]
    #[case::bpm120_one_beat(Some(120.0), 1.0, 500_000)]
    #[case::bpm120_four_beats(Some(120.0), 4.0, 2_000_000)]
    #[case::bpm150(Some(150.0), 1.0, 400_000)]
    #[case::default_130(None, 13.0, 6_000_000)]
    #[case::zero_falls_back_to_130(Some(0.0), 13.0, 6_000_000)]
    fn dom_tim_01_constant_tempo(#[case] init: Option<f64>, #[case] beat: f64, #[case] us: i64) {
        assert_eq!(TimingMap::new(init, &[]).time_of(Beat(beat)), us);
    }

    /// DOM-TIM-02: BPM 変化は以降に効く。同じ拍では後勝ち
    #[rstest]
    #[case::before_change(vec![bpm(4.0, 240.0)], 2.0, 1_000_000)]
    #[case::at_change(vec![bpm(4.0, 240.0)], 4.0, 2_000_000)]
    #[case::after_change(vec![bpm(4.0, 240.0)], 6.0, 2_500_000)]
    #[case::same_beat_last_wins(vec![bpm(4.0, 60.0), bpm(4.0, 240.0)], 6.0, 2_500_000)]
    #[case::unsorted_input(vec![bpm(8.0, 60.0), bpm(4.0, 240.0)], 9.0, 4_000_000)]
    fn dom_tim_02_bpm_changes(#[case] evs: Vec<TimingEvent>, #[case] beat: f64, #[case] us: i64) {
        assert_eq!(TimingMap::new(Some(120.0), &evs).time_of(Beat(beat)), us);
    }

    /// DOM-TIM-03: STOP の拍は停止前の時刻、以降は停止分遅れる。同じ拍の BPM 変化の後に評価、同じ拍の STOP は加算
    #[rstest]
    #[case::at_stop_is_before(vec![stop(4.0, 1.0)], 4.0, 2_000_000)]
    #[case::after_stop_delayed(vec![stop(4.0, 1.0)], 5.0, 3_000_000)]
    #[case::stop_uses_new_bpm(vec![stop(4.0, 1.0), bpm(4.0, 240.0)], 5.0, 2_500_000)]
    #[case::stops_add(vec![stop(4.0, 1.0), stop(4.0, 1.0)], 5.0, 3_500_000)]
    fn dom_tim_03_stops(#[case] evs: Vec<TimingEvent>, #[case] beat: f64, #[case] us: i64) {
        assert_eq!(TimingMap::new(Some(120.0), &evs).time_of(Beat(beat)), us);
    }

    /// DOM-TIM-04: BPM ≤ 0 は無視する（LR2 の未定義参照 -1 など。LR2 実機の挙動は未検証）
    #[rstest]
    #[case::negative(-1.0)]
    #[case::zero(0.0)]
    fn dom_tim_04_non_positive_bpm_ignored(#[case] v: f64) {
        let m = TimingMap::new(Some(120.0), &[bpm(4.0, v)]);
        assert_eq!(m.time_of(Beat(8.0)), 4_000_000);
    }

    /// DOM-TIM-05: beat_at は time_of の逆。停止中は停止している拍
    #[rstest]
    #[case::constant(vec![], 1_000_000, 2.0)]
    #[case::after_bpm_change(vec![bpm(4.0, 240.0)], 2_500_000, 6.0)]
    #[case::during_stop(vec![stop(4.0, 1.0)], 2_400_000, 4.0)]
    #[case::after_stop(vec![stop(4.0, 1.0)], 3_000_000, 5.0)]
    #[case::before_zero(vec![], -500_000, -1.0)]
    fn dom_tim_05_beat_at(#[case] evs: Vec<TimingEvent>, #[case] us: i64, #[case] beat: f64) {
        assert_eq!(TimingMap::new(Some(120.0), &evs).beat_at(us), Beat(beat));
    }
}
