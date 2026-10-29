//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

// ---- テスト用の部品 ----------------------------------------------------------

/// 決められた値を順に返す乱数。呼ばれた `max` を記録する
struct Script {
    values: Vec<u32>,
    calls: Vec<u32>,
}
impl Script {
    fn new(values: &[u32]) -> Self {
        Self {
            values: values.to_vec(),
            calls: vec![],
        }
    }
}
impl RandomSource for Script {
    fn next_inclusive(&mut self, max: u32) -> u32 {
        self.calls.push(max);
        if self.values.is_empty() {
            0
        } else {
            self.values.remove(0)
        }
    }
}

fn pos(measure: u32, fraction: f64) -> Position {
    Position::new(measure, fraction)
}
fn bgm(id: u32) -> Block {
    Block::Statements(vec![Statement::Event(RawEvent::Bgm {
        pos: pos(1, 0.0),
        id: ObjId(id),
    })])
}
fn note(m: u32, f: f64, lane: u8, object: RawObject, id: u32) -> Statement {
    Statement::Event(RawEvent::Note {
        pos: pos(m, f),
        lane: Lane(lane),
        object,
        id: ObjId(id),
    })
}
fn def(d: Definition) -> Statement {
    Statement::Define(d)
}
fn doc(body: Vec<Block>) -> ChartDocument {
    ChartDocument {
        layout: KeyLayout::Beat7,
        policy: ResolvePolicy::default(),
        body,
    }
}
fn doc_with(policy: ResolvePolicy, body: Vec<Block>) -> ChartDocument {
    ChartDocument {
        layout: KeyLayout::Beat7,
        policy,
        body,
    }
}
fn bgm_ids(r: &ResolvedChart) -> Vec<u32> {
    r.events
        .iter()
        .filter_map(|e| match e {
            Event::Bgm { sound, .. } => Some(sound.0),
            _ => None,
        })
        .collect()
}
fn if_block(arms: Vec<(u32, Vec<Block>)>, otherwise: Option<Vec<Block>>) -> Block {
    Block::If { arms, otherwise }
}
fn random(max: u32, body: Vec<Block>) -> Block {
    Block::Random {
        generator: Generator::Draw { max },
        body,
    }
}

// ---- CORE-RND ---------------------------------------------------------------

/// CORE-RND-01: `Draw(n)` は `next_inclusive(n-1)+1`
#[rstest]
#[case::max1_always_1(1, 0, 0, 1)]
#[case::max2_rng0_is_1(2, 0, 1, 1)]
#[case::max2_rng1_is_2(2, 1, 1, 2)]
#[case::max5_rng4_is_5(5, 4, 4, 5)]
fn core_rnd_01_draw_maps_rng_to_one_based(
    #[case] max: u32,
    #[case] rng_value: u32,
    #[case] expected_rng_max: u32,
    #[case] selected: u32,
) {
    let arms = (1..=max).map(|k| (k, vec![bgm(k)])).collect();
    let mut rng = Script::new(&[rng_value]);
    let r = doc(vec![random(max, vec![if_block(arms, None)])]).resolve(&mut rng);
    assert_eq!(rng.calls, vec![expected_rng_max]);
    assert_eq!(bgm_ids(&r), vec![selected]);
}

/// CORE-RND-02: `Fixed(v)` は乱数を消費しない
#[rstest]
#[case::fixed1(1)]
#[case::fixed3(3)]
fn core_rnd_02_fixed_does_not_consume(#[case] v: u32) {
    let mut rng = Script::new(&[]);
    let body = vec![Block::Random {
        generator: Generator::Fixed(v),
        body: vec![if_block(vec![(1, vec![bgm(1)]), (3, vec![bgm(3)])], None)],
    }];
    let r = doc(body).resolve(&mut rng);
    assert!(rng.calls.is_empty());
    assert_eq!(bgm_ids(&r), vec![v]);
}

/// CORE-RND-03: 一致した腕のみ。無ければ otherwise
#[rstest]
#[case::first_arm(0, vec![10])]
#[case::second_arm(1, vec![20])]
#[case::falls_to_otherwise(2, vec![99])]
fn core_rnd_03_if_selects_matching_arm_or_otherwise(
    #[case] rng_value: u32,
    #[case] expected: Vec<u32>,
) {
    let body = vec![random(
        3,
        vec![if_block(
            vec![(1, vec![bgm(10)]), (2, vec![bgm(20)])],
            Some(vec![bgm(99)]),
        )],
    )];
    let r = doc(body).resolve(&mut Script::new(&[rng_value]));
    assert_eq!(bgm_ids(&r), expected);
}

/// CORE-RND-04: Random の外の If はどの腕にも一致しない（otherwise も採らない）
#[rstest]
#[case::without_otherwise(None)]
#[case::with_otherwise(Some(vec![bgm(99)]))]
fn core_rnd_04_if_outside_random_never_matches(#[case] otherwise: Option<Vec<Block>>) {
    let r = doc(vec![if_block(vec![(1, vec![bgm(1)])], otherwise)]).resolve(&mut Script::new(&[]));
    assert!(bgm_ids(&r).is_empty());
}

/// CORE-RND-05: 乱数は文書順に消費（入れ子含む）
#[rstest]
#[case::outer_then_inner(vec![0, 1, 0], vec![1, 12, 3])]
#[case::outer_other_branch_skips_inner(vec![1, 0], vec![2, 3])]
fn core_rnd_05_consumption_follows_document_order(
    #[case] values: Vec<u32>,
    #[case] expected: Vec<u32>,
) {
    // RANDOM 2 { IF 1 { bgm1; RANDOM 2 { IF 1 bgm11 IF 2 bgm12 } } IF 2 { bgm2 } } ; RANDOM 3 {IF 1 bgm3}
    let inner = random(
        2,
        vec![if_block(vec![(1, vec![bgm(11)]), (2, vec![bgm(12)])], None)],
    );
    let body = vec![
        random(
            2,
            vec![if_block(
                vec![(1, vec![bgm(1), inner]), (2, vec![bgm(2)])],
                None,
            )],
        ),
        random(3, vec![if_block(vec![(1, vec![bgm(3)])], None)]),
    ];
    let r = doc(body).resolve(&mut Script::new(&values));
    assert_eq!(bgm_ids(&r), expected);
}

// ---- CORE-DEF ---------------------------------------------------------------

/// CORE-DEF-01: 定義は後勝ち
#[rstest]
#[case::single("a.wav", None, "a.wav")]
#[case::later_overrides("a.wav", Some("b.wav"), "b.wav")]
fn core_def_01_definitions_last_wins(
    #[case] first: &str,
    #[case] second: Option<&str>,
    #[case] expected: &str,
) {
    let mut ss = vec![def(Definition::Sound(ObjId(1), first.into()))];
    if let Some(s) = second {
        ss.push(def(Definition::Sound(ObjId(1), s.into())));
    }
    let r = doc(vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    assert_eq!(r.sounds.get(&ObjId(1)).map(String::as_str), Some(expected));
}

/// CORE-DEF-02: 初期 BPM の優先
#[rstest]
#[case::last_wins(Precedence::LastWins, 180.0)]
#[case::first_wins_lr2(Precedence::FirstWins, 120.0)]
fn core_def_02_initial_bpm_precedence(#[case] p: Precedence, #[case] expected: f64) {
    let policy = ResolvePolicy {
        initial_bpm: p,
        ..Default::default()
    };
    let body = vec![Block::Statements(vec![
        def(Definition::Meta(MetaField::InitialBpm(120.0))),
        def(Definition::Meta(MetaField::InitialBpm(180.0))),
    ])];
    let r = doc_with(policy, body).resolve(&mut Script::new(&[]));
    assert_eq!(r.meta.initial_bpm, Some(expected));
}

/// CORE-DEF-03: 未定義参照
#[rstest]
#[case::bpm_defined(UndefinedRef::Drop, true, Some(Event::Bpm { pos: pos(1, 0.0), bpm: 200.0 }))]
#[case::bpm_undefined_drop(UndefinedRef::Drop, false, None)]
#[case::bpm_undefined_lr2_minus_one(UndefinedRef::Value(-1.0), false, Some(Event::Bpm { pos: pos(1, 0.0), bpm: -1.0 }))]
fn core_def_03_undefined_bpm_reference(
    #[case] undefined: UndefinedRef,
    #[case] defined: bool,
    #[case] expected: Option<Event>,
) {
    let policy = ResolvePolicy {
        undefined_bpm: undefined,
        ..Default::default()
    };
    let mut ss = vec![Statement::Event(RawEvent::BpmRef {
        pos: pos(1, 0.0),
        id: ObjId(1),
    })];
    if defined {
        ss.push(def(Definition::Bpm(ObjId(1), 200.0)));
    }
    let r = doc_with(policy, vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    assert_eq!(r.events.first().cloned(), expected);
}

// ---- CORE-LN ----------------------------------------------------------------

fn kinds(r: &ResolvedChart) -> Vec<(u8, u32, NoteKind)> {
    r.events
        .iter()
        .filter_map(|e| match e {
            Event::Note {
                lane, kind, pos, ..
            } => Some((lane.0, pos.measure, kind.clone())),
            _ => None,
        })
        .collect()
}
fn long_to(m: u32, f: f64, end_id: u32) -> NoteKind {
    NoteKind::Long {
        end: pos(m, f),
        end_sound: Some(ObjId(end_id)),
    }
}

/// CORE-LN-01: LnToggle は時刻順に頭→尾
#[rstest]
#[case::one_pair(vec![(1, 0.0, 1), (1, 0.5, 2)], vec![(1, 1, long_to(1, 0.5, 2))])]
#[case::two_pairs(
    vec![(1, 0.0, 1), (1, 0.5, 2), (2, 0.0, 3), (2, 0.5, 4)],
    vec![(1, 1, long_to(1, 0.5, 2)), (1, 2, long_to(2, 0.5, 4))]
)]
#[case::written_out_of_order(vec![(1, 0.5, 2), (1, 0.0, 1)], vec![(1, 1, long_to(1, 0.5, 2))])]
fn core_ln_01_toggle_pairs_in_time_order(
    #[case] objs: Vec<(u32, f64, u32)>,
    #[case] expected: Vec<(u8, u32, NoteKind)>,
) {
    let ss = objs
        .into_iter()
        .map(|(m, f, id)| note(m, f, 1, RawObject::LnToggle, id))
        .collect();
    let r = doc(vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    assert_eq!(kinds(&r), expected);
}

/// CORE-LN-02: LN 終端 ID は直前の通常ノーツを LN にする
#[rstest]
#[case::ends_previous(Some(9), vec![(1, 0.0, 1), (1, 0.5, 9)], vec![(1, 1, long_to(1, 0.5, 9))])]
#[case::no_lnobj_defined(None, vec![(1, 0.0, 1), (1, 0.5, 9)], vec![(1, 1, NoteKind::Normal), (1, 1, NoteKind::Normal)])]
#[case::end_without_head_is_normal(Some(9), vec![(1, 0.0, 9)], vec![(1, 1, NoteKind::Normal)])]
fn core_ln_02_ln_end_object_closes_previous_note(
    #[case] lnobj: Option<u32>,
    #[case] objs: Vec<(u32, f64, u32)>,
    #[case] expected: Vec<(u8, u32, NoteKind)>,
) {
    let mut ss: Vec<Statement> = objs
        .into_iter()
        .map(|(m, f, id)| note(m, f, 1, RawObject::Normal, id))
        .collect();
    if let Some(id) = lnobj {
        ss.push(def(Definition::LnEnd(ObjId(id))));
    }
    let r = doc(vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    assert_eq!(kinds(&r), expected);
}

/// CORE-LN-03: 対にならない LN 頭は通常ノーツ + 診断
#[rstest]
#[case::single_toggle(1)]
#[case::three_toggles(3)]
fn core_ln_03_unpaired_head_becomes_normal_with_diagnostic(#[case] count: u32) {
    let ss = (0..count)
        .map(|i| note(1 + i, 0.0, 1, RawObject::LnToggle, i + 1))
        .collect();
    let r = doc(vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    assert_eq!(
        kinds(&r).last().map(|k| k.2.clone()),
        Some(NoteKind::Normal)
    );
    assert_eq!(r.diagnostics.len(), 1);
}

/// CORE-LN-04: 保留は LnToggle と LN 終端 ID で共有（LR2 互換）
#[rstest]
#[case::normal_then_toggle_closes_normal(
    vec![(0.0, RawObject::Normal, 1), (0.5, RawObject::LnToggle, 2)],
    vec![long_to(1, 0.5, 2)]
)]
#[case::toggle_head_overwritten_by_normal(
    vec![(0.0, RawObject::LnToggle, 1), (0.25, RawObject::Normal, 2), (0.5, RawObject::Normal, 9)],
    vec![NoteKind::Normal, long_to(1, 0.5, 9)]
)]
fn core_ln_04_pending_is_shared_per_lane(
    #[case] objs: Vec<(f64, RawObject, u32)>,
    #[case] expected: Vec<NoteKind>,
) {
    let mut ss: Vec<Statement> = objs
        .into_iter()
        .map(|(f, o, id)| note(1, f, 1, o, id))
        .collect();
    ss.push(def(Definition::LnEnd(ObjId(9))));
    let r = doc(vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    let got: Vec<NoteKind> = kinds(&r).into_iter().map(|k| k.2).collect();
    assert_eq!(got, expected);
}

/// CORE-LN-05: 長さ付き LN
#[rstest]
#[case::plain(vec![], vec![long_none(2, 0.0)])]
#[case::toggle_pending_unaffected(
    vec![note(1, 0.25, 1, RawObject::LnToggle, 7), note(1, 0.75, 1, RawObject::LnToggle, 8)],
    vec![long_none(2, 0.0), long_to(1, 0.75, 8)]
)]
fn core_ln_05_explicit_long_note(#[case] extra: Vec<Statement>, #[case] expected: Vec<NoteKind>) {
    let mut ss = vec![Statement::Event(RawEvent::Long {
        pos: pos(1, 0.0),
        end: pos(2, 0.0),
        lane: Lane(1),
        id: ObjId(5),
    })];
    ss.extend(extra);
    let r = doc(vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    let got: Vec<NoteKind> = kinds(&r).into_iter().map(|k| k.2).collect();
    assert_eq!(got, expected);
}
fn long_none(m: u32, f: f64) -> NoteKind {
    NoteKind::Long {
        end: pos(m, f),
        end_sound: None,
    }
}

// ---- CORE-ORD ---------------------------------------------------------------

/// CORE-ORD-01: 位置順・同位置は文書順
#[rstest]
#[case::reversed(vec![(2, 0.0, 1), (1, 0.0, 2)], vec![2, 1])]
#[case::fraction_order(vec![(1, 0.75, 1), (1, 0.25, 2)], vec![2, 1])]
#[case::same_position_keeps_document_order(vec![(1, 0.5, 7), (1, 0.5, 3)], vec![7, 3])]
fn core_ord_01_events_sorted_stably(
    #[case] objs: Vec<(u32, f64, u32)>,
    #[case] expected: Vec<u32>,
) {
    let ss = objs
        .into_iter()
        .map(|(m, f, id)| {
            Statement::Event(RawEvent::Bgm {
                pos: pos(m, f),
                id: ObjId(id),
            })
        })
        .collect();
    let r = doc(vec![Block::Statements(ss)]).resolve(&mut Script::new(&[]));
    assert_eq!(bgm_ids(&r), expected);
}

/// CORE-ORD-02: 位置 0 の可視/不可視ノーツで全体を 1 小節ずらす
#[rstest]
#[case::visible_at_zero_shifts(true, RawObject::Normal, 0.0, 1)]
#[case::invisible_at_zero_shifts(true, RawObject::Invisible, 0.0, 1)]
#[case::mine_at_zero_does_not_shift(true, RawObject::Mine, 0.0, 0)]
#[case::not_exactly_zero_does_not_shift(true, RawObject::Normal, 0.5, 0)]
#[case::policy_off(false, RawObject::Normal, 0.0, 0)]
fn core_ord_02_shift_when_note_at_zero(
    #[case] enabled: bool,
    #[case] object: RawObject,
    #[case] fraction: f64,
    #[case] expected_measure: u32,
) {
    let policy = ResolvePolicy {
        shift_when_note_at_zero: enabled,
        ..Default::default()
    };
    let body = vec![Block::Statements(vec![note(0, fraction, 1, object, 1)])];
    let r = doc_with(policy, body).resolve(&mut Script::new(&[]));
    assert_eq!(r.events[0].position().measure, expected_measure);
}
