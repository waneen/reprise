//! LR2 の `#FLIPSIDE`（DOM-SKD-23）。OpenLR2 `LR2_skinload.cpp` の `ApplyFlipside`・`FlipSide_Timer` を仕様として参照した（コードは転載していない）
use super::{ElementKind, Skin};

/// LR2 `FlipSide_Timer`: 1P と 2P のタイマーを入れ替える
pub fn flip_timer(t: i32) -> i32 {
    match t {
        42 | 44 | 46 | 48 => t + 1,
        43 | 45 | 47 | 49 => t - 1,
        143 => 144,
        144 => 143,
        50..=59 | 70..=79 | 100..=109 | 120..=129 => t + 10,
        60..=69 | 80..=89 | 110..=119 | 130..=139 => t - 10,
        _ => t,
    }
}

/// DOM-SKD-23: LR2 の `#FLIPSIDE`（`ApplyFlipside`。読み込みの最後に 1 回）。1P と 2P の部品を入れ替え、2P 側に描いた部品で 1P のプレイを描く:
/// GROOVEGAUGE 0↔1・NOWJUDGE/NOWCOMBO の 1P↔2P・NOTE と LN_BODY/LN_START/LN_END の i↔10+i（i < 10。MINE は入れ替えない）・LINE 0↔1、
/// IMAGE の SRC と DST のタイマー（[`flip_timer`]）と DST 引数 21 の 1↔2、NUMBER の値 10↔12・100..=119 は +20・120 以上は −20、
/// BARGRAPH 10↔14、TEXT 1↔2、SLIDER 4↔5
pub fn flip_side(skin: &mut Skin) {
    for e in &mut skin.elements {
        match &mut e.kind {
            ElementKind::Other { command, index } => match command.as_str() {
                "GROOVEGAUGE" | "LINE" if (0..=1).contains(index) => *index = 1 - *index,
                "NOWJUDGE_1P" => *command = "NOWJUDGE_2P".into(),
                "NOWJUDGE_2P" => *command = "NOWJUDGE_1P".into(),
                "NOWCOMBO_1P" => *command = "NOWCOMBO_2P".into(),
                "NOWCOMBO_2P" => *command = "NOWCOMBO_1P".into(),
                "LN_BODY" | "LN_START" | "LN_END" => *index = flip_lane(*index),
                _ => {}
            },
            ElementKind::NoteLane { lane } => *lane = flip_lane(*lane),
            ElementKind::Image => {
                e.timer.0 = flip_timer(e.timer.0);
                if let Some(s) = e.source.as_mut() {
                    s.timer.0 = flip_timer(s.timer.0);
                }
                e.dst_extra[0] = match e.dst_extra[0] {
                    1 => 2,
                    2 => 1,
                    v => v,
                };
            }
            ElementKind::Number { value_ref, .. } => {
                *value_ref = match *value_ref {
                    10 => 12,
                    12 => 10,
                    v if v < 100 => v,
                    v if v < 120 => v + 20,
                    v => v - 20,
                }
            }
            ElementKind::Bar { value_ref, .. } => swap(value_ref, 10, 14),
            ElementKind::Text { value_ref, .. } => swap(value_ref, 1, 2),
            ElementKind::Slider { value_ref, .. } => swap(value_ref, 4, 5),
            _ => {}
        }
    }
}

/// レーンの番号 i（0..10）と 10+i を入れ替える
fn flip_lane(i: i32) -> i32 {
    match i {
        0..=9 => i + 10,
        10..=19 => i - 10,
        _ => i,
    }
}

fn swap(v: &mut i32, a: i32, b: i32) {
    if *v == a {
        *v = b;
    } else if *v == b {
        *v = a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::{Element, SceneKind, SourceRect, TimerId};
    use rstest::rstest;

    fn other(command: &str, index: i32) -> ElementKind {
        ElementKind::Other {
            command: command.into(),
            index,
        }
    }
    fn skin(elements: Vec<Element>) -> Skin {
        Skin {
            scene: SceneKind::Play7,
            resolution: (640, 480),
            images: vec![],
            elements,
            options: vec![],
            color_key: None,
            timing: Default::default(),
            fonts: vec![],
            image_fonts: vec![],
            bars: Default::default(),
        }
    }
    fn el(kind: ElementKind, timer: i32, src_timer: i32) -> Element {
        Element {
            kind,
            source: Some(SourceRect {
                image: 0,
                rect: [0, 0, 1, 1],
                divide: (1, 1),
                cycle_ms: 0,
                timer: TimerId(src_timer),
                params: [0; 5],
            }),
            timer: TimerId(timer),
            conditions: vec![],
            looping: Some(0),
            dst_extra: [0; 2],
            order: 0,
            keyframes: vec![],
        }
    }
    fn flipped(
        kind: ElementKind,
        timer: i32,
        src_timer: i32,
        side: i32,
    ) -> (ElementKind, i32, i32, i32) {
        let mut e = el(kind, timer, src_timer);
        e.dst_extra[0] = side;
        let mut sk = skin(vec![e]);
        flip_side(&mut sk);
        let e = &sk.elements[0];
        (
            e.kind.clone(),
            e.timer.0,
            e.source.as_ref().unwrap().timer.0,
            e.dst_extra[0],
        )
    }
    fn number(v: i32) -> ElementKind {
        ElementKind::Number {
            value_ref: v,
            digits: 3,
            align: 0,
        }
    }

    /// DOM-SKD-23: `#FLIPSIDE` で入れ替えるもの（種類, DST のタイマー, SRC のタイマー, DST 引数 21）。タイマーと皿の向きは IMAGE だけ
    #[rstest]
    #[case::gauge(other("GROOVEGAUGE", 0), 0, 0, 0, (other("GROOVEGAUGE", 1), 0, 0, 0))]
    #[case::gauge_2p(other("GROOVEGAUGE", 1), 0, 0, 0, (other("GROOVEGAUGE", 0), 0, 0, 0))]
    #[case::line(other("LINE", 1), 0, 0, 0, (other("LINE", 0), 0, 0, 0))]
    #[case::judge(other("NOWJUDGE_2P", 3), 0, 0, 0, (other("NOWJUDGE_1P", 3), 0, 0, 0))]
    #[case::combo(other("NOWCOMBO_1P", 5), 0, 0, 0, (other("NOWCOMBO_2P", 5), 0, 0, 0))]
    #[case::note(ElementKind::NoteLane { lane: 13 }, 0, 0, 0, (ElementKind::NoteLane { lane: 3 }, 0, 0, 0))]
    #[case::note_1p(ElementKind::NoteLane { lane: 0 }, 0, 0, 0, (ElementKind::NoteLane { lane: 10 }, 0, 0, 0))]
    #[case::ln(other("LN_START", 11), 0, 0, 0, (other("LN_START", 1), 0, 0, 0))]
    #[case::mine_kept(other("MINE", 1), 0, 0, 0, (other("MINE", 1), 0, 0, 0))]
    #[case::number_hs(number(10), 0, 0, 0, (number(12), 0, 0, 0))]
    #[case::number_score(number(101), 0, 0, 0, (number(121), 0, 0, 0))]
    #[case::number_2p(number(121), 0, 0, 0, (number(101), 0, 0, 0))]
    #[case::number_bpm(number(160), 0, 0, 0, (number(140), 0, 0, 0))]
    #[case::number_low(number(71), 0, 0, 0, (number(71), 0, 0, 0))]
    #[case::bar(ElementKind::Bar { value_ref: 14, direction: 0 }, 0, 0, 0, (ElementKind::Bar { value_ref: 10, direction: 0 }, 0, 0, 0))]
    #[case::text(ElementKind::Text { value_ref: 1, font: 0, align: 0 }, 0, 0, 0, (ElementKind::Text { value_ref: 2, font: 0, align: 0 }, 0, 0, 0))]
    #[case::slider(ElementKind::Slider { value_ref: 4, direction: 0, range: 1 }, 0, 0, 0, (ElementKind::Slider { value_ref: 5, direction: 0, range: 1 }, 0, 0, 0))]
    #[case::bomb(ElementKind::Image, 50, 60, 1, (ElementKind::Image, 60, 50, 2))]
    #[case::key(ElementKind::Image, 110, 130, 2, (ElementKind::Image, 100, 120, 1))]
    #[case::judge_timer(ElementKind::Image, 46, 47, 0, (ElementKind::Image, 47, 46, 0))]
    #[case::gauge_up(ElementKind::Image, 42, 45, 0, (ElementKind::Image, 43, 44, 0))]
    #[case::last_note(ElementKind::Image, 143, 0, 0, (ElementKind::Image, 144, 0, 0))]
    #[case::ln_hold(ElementKind::Image, 75, 89, 0, (ElementKind::Image, 85, 79, 0))]
    #[case::untouched(ElementKind::Image, 41, 140, 3, (ElementKind::Image, 41, 140, 3))]
    #[case::button_kept(ElementKind::Button { action: 13 }, 100, 50, 1, (ElementKind::Button { action: 13 }, 100, 50, 1))]
    fn dom_skd_23_flip_side(
        #[case] kind: ElementKind,
        #[case] timer: i32,
        #[case] src_timer: i32,
        #[case] side: i32,
        #[case] want: (ElementKind, i32, i32, i32),
    ) {
        assert_eq!(flipped(kind, timer, src_timer, side), want);
    }
}
