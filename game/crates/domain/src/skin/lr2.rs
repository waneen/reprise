//! LR2 のプレイ画面・選曲画面・キー設定画面のスキン描画（DOM-SKD-*）。どの要素を・どこに・どの切り出しで描くかを決め、
//! 描画 API には依存しない四角形・単色の四角形・システムフォントの文字列の列（[`Draw`]）を返す。OpenLR2 の `LR2_skindraw.cpp`・`Main.cpp`・
//! `Scene04_Play.cpp`・`Scene02_Songselect.cpp`・`LR2_skinobject.cpp`・`LR2_skinload.cpp` を仕様として参照した（コードは転載していない）。
use super::{Blend, Element, ElementKind, ImageFont, Keyframe, Skin, SourceRect, SystemFont};

/// 描画時に問い合わせるゲームの状態（LR2 の番号で引く）
pub trait SkinState {
    /// タイマーの経過 ms。止まっていれば None
    fn timer(&self, id: i32) -> Option<i64>;
    /// 条件（1 以上）が真か。分からない番号は偽
    fn option(&self, id: i32) -> bool;
    /// 数値。分からない番号は None（LR2 の −1）
    fn number(&self, id: i32) -> Option<i64>;
    /// BARGRAPH の値（種類番号ごと、0..=1）
    fn bar(&self, kind: i32) -> f64;
    /// SLIDER の値（種類番号ごと、0..=1）
    fn slider(&self, kind: i32) -> f64;
    /// 皿の角度（度）。`side` は 1 = 1P、2 = 2P
    fn turntable(&self, side: i32) -> f32;
    /// 文字列（LR2 の文字列番号）。分からない番号・空は None
    fn text(&self, id: i32) -> Option<&str>;
    /// BUTTON の状態値（描くコマ）。分からない動作は None で、描かない
    fn button(&self, _action: i32) -> Option<i32> {
        None
    }
}

/// プレイ画面に固有の描画材料
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayView {
    /// NOWJUDGE の添字（5=PG, 4=GR, 3=GD, 2=BD, 1=POOR, 0=空POOR）。まだ判定が無ければ None
    pub judge: Option<usize>,
    pub combo: i64,
    /// 表示する HP を 2 で割った整数（0..=50）
    pub gauge: i32,
    /// HARD 系のゲージ（全区間を同じ色で描く）
    pub survival: bool,
    /// ゲージのちらつき用の乱数（0..=2）
    pub tremble: i32,
    /// ハイスピード（×100）× HS-FIX の倍率
    pub hi_speed: f64,
    pub notes: Vec<NoteView>,
    /// 小節線（判定までの拍数）
    pub lines: Vec<f64>,
    /// BGA の層（奥から順）
    pub bga: Vec<BgaView>,
}

/// 描く BGA の層
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BgaView {
    /// 画像の番号（[`draw_play`] の `image_size` と同じ番号）
    pub image: u32,
    /// 後ろに黒い四角形を描く（LR2 の 1 層目）
    pub backbox: bool,
}

/// 描くノーツ
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteView {
    /// 0 = 皿、1..=7 = 鍵盤
    pub lane: i32,
    /// 判定までの拍数（過ぎていれば負）
    pub head: f64,
    /// LN の終端までの拍数
    pub tail: Option<f64>,
    pub mine: bool,
    /// 押下中の LN
    pub held: bool,
}

/// 描く四角形（スキン座標、y は下向き）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub image: u32,
    /// 画像の切り出し x, y, w, h（ピクセル）
    pub src: [f32; 4],
    /// 描く位置 x, y, w, h（w, h は 0 以上）
    pub dst: [f32; 4],
    /// a, r, g, b
    pub argb: [u8; 4],
    /// 度（画面上で時計回り）
    pub angle: f32,
    /// 回転の中心（テンキー配置。0, 5 などは中央）
    pub center: u8,
    pub blend: Blend,
    pub filter: bool,
}

/// 描くもの（奥から順）
#[derive(Debug, Clone, PartialEq)]
pub enum Draw {
    Quad(Quad),
    /// システムフォントの文字列（大きさの合わせ方は [`TextRun`]）
    Text(TextRun),
    /// 単色の四角形（BGA の後ろの黒）
    Fill(Fill),
}

/// 単色の四角形
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    /// x, y, w, h
    pub dst: [f32; 4],
    /// a, r, g, b
    pub argb: [u8; 4],
    pub blend: Blend,
}

/// システムフォントで描く文字列。描く側は LR2 と同じく、文字列の幅 `width`（フォントの大きさで測る）から
/// 縦 = h / 大きさ、横 = 縦 × (width > w なら w / width、それ以外 1) に拡大し、揃え 1 なら幅の半分、2 なら幅だけ左へずらす
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub font: SystemFont,
    pub text: String,
    /// x, y, w, h（x, y は揃える前の左上）
    pub dst: [f32; 4],
    /// 0 = 左、1 = 中央、2 = 右
    pub align: i32,
    pub argb: [u8; 4],
    pub blend: Blend,
    pub filter: bool,
}

/// キーフレームを評価した結果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub dst: [f32; 4],
    pub argb: [f32; 4],
    pub angle: f32,
    pub center: u8,
    pub blend: Blend,
    pub filter: bool,
}

// ---------------------------------------------------------------------------
// DOM-SKD-01 条件
// ---------------------------------------------------------------------------

/// 0 は真、負は否定、1000 以上は偽
pub fn op_true(st: &dyn SkinState, id: i32) -> bool {
    let holds = |i: i32| i < 1000 && st.option(i);
    match id {
        0 => true,
        i if i > 0 => holds(i),
        i => !holds(-i),
    }
}

fn ops_true(st: &dyn SkinState, e: &Element) -> bool {
    e.conditions.iter().all(|c| op_true(st, c.0))
}

// ---------------------------------------------------------------------------
// DOM-SKD-02, 03 キーフレーム
// ---------------------------------------------------------------------------

/// キーフレームを引く時刻。描かないなら None。`t` はタイマーの経過（止まっていれば −1）
pub fn effective_time(kfs: &[Keyframe], looping: i32, t: i64) -> Option<i64> {
    let (ts, te) = (kfs.first()?.t_ms as i64, kfs.last()?.t_ms as i64);
    let lp = looping as i64;
    if !(ts <= te && ts <= t && (lp >= 0 || t <= te)) {
        return None;
    }
    Some(if ts == te || lp == te {
        t.min(te)
    } else if lp < te {
        if t > te {
            (t - lp).rem_euclid(te - lp) + lp
        } else {
            t
        }
    } else {
        0
    })
}

fn ease(v1: f64, v2: f64, t1: f64, t2: f64, now: f64, accel: i32) -> f64 {
    if v1 == v2 {
        return v1;
    }
    if t1 <= now && now <= t2 && t1 < t2 {
        let r = (now - t1) / (t2 - t1);
        let r = match accel {
            0 => r,
            1 => r * r * r,
            2 => 1.0 - (1.0 - r).powi(3),
            _ => return v1,
        };
        return (1.0 - r) * v1 + r * v2;
    }
    if t1 < now {
        v2
    } else {
        v1
    }
}

/// 時刻 `t2` 以下で最後のキーフレームから次へ、その加速の種類で補間する
pub fn interpolate(kfs: &[Keyframe], t2: i64) -> Pose {
    let sel = kfs.iter().rposition(|k| k.t_ms as i64 <= t2).unwrap_or(0);
    let a = &kfs[sel];
    let color = |k: &Keyframe| k.color.map(|c| c as f32);
    let mut pose = Pose {
        dst: a.dst,
        argb: color(a),
        angle: a.angle,
        center: a.center,
        blend: a.blend,
        filter: a.filter,
    };
    if t2 != a.t_ms as i64 && sel + 1 < kfs.len() {
        let b = &kfs[sel + 1];
        let f = |v1: f32, v2: f32| {
            ease(
                v1 as f64,
                v2 as f64,
                a.t_ms as f64,
                b.t_ms as f64,
                t2 as f64,
                a.accel,
            ) as f32
        };
        let cb = color(b);
        pose.dst = std::array::from_fn(|i| f(a.dst[i], b.dst[i]));
        pose.argb = std::array::from_fn(|i| f(pose.argb[i], cb[i]));
        pose.angle = f(a.angle, b.angle);
    }
    pose
}

/// 要素をタイマーで評価する（描かないなら None）
pub fn pose_at(e: &Element, t: Option<i64>) -> Option<Pose> {
    let t2 = effective_time(&e.keyframes, e.looping.unwrap_or(0), t.unwrap_or(-1))?;
    Some(interpolate(&e.keyframes, t2))
}

// ---------------------------------------------------------------------------
// DOM-SKD-04 切り出し
// ---------------------------------------------------------------------------

fn frame_count(s: &SourceRect) -> usize {
    (s.divide.0.max(1) * s.divide.1.max(1)) as usize
}

/// 切り出しのコマ番号。`time` は SRC のタイマーの経過（止まっていれば −1）
pub fn src_frame(s: &SourceRect, time: i64) -> usize {
    let n = frame_count(s);
    if time < 0 || s.cycle_ms <= 0 {
        return 0;
    }
    let f = (time % s.cycle_ms as i64) as usize * n / s.cycle_ms as usize;
    if f >= n {
        0
    } else {
        f
    }
}

/// コマ `frame` の切り出し。w/h が負なら画像全体の大きさ。コマは左上から行ごとに数える
pub fn src_cell(s: &SourceRect, image_size: Option<(u32, u32)>, frame: usize) -> Option<[f32; 4]> {
    let [x, y, mut w, mut h] = s.rect;
    if w < 0 || h < 0 {
        let (iw, ih) = image_size?;
        if w < 0 {
            w = iw as i32;
        }
        if h < 0 {
            h = ih as i32;
        }
    }
    let (dx, dy) = (s.divide.0.max(1) as i32, s.divide.1.max(1) as i32);
    let (bw, bh) = (w / dx, h / dy);
    let f = frame as i32 % (dx * dy);
    let (col, row) = (f % dx, f / dx);
    Some([
        (x + bw * col) as f32,
        (y + bh * row) as f32,
        bw as f32,
        bh as f32,
    ])
}

/// SRC のタイマーの経過。DST と同じタイマーなら最初のキーフレームの時刻を引く
fn src_time(st: &dyn SkinState, e: &Element, s: &SourceRect) -> i64 {
    let Some(t) = st.timer(s.timer.0) else {
        return -1;
    };
    if s.timer == e.timer {
        t - e.keyframes.first().map_or(0, |k| k.t_ms as i64)
    } else {
        t
    }
}

// ---------------------------------------------------------------------------
// DOM-SKD-05 四角形
// ---------------------------------------------------------------------------

/// α < 2 で合成ありなら描かない。負の幅・高さは位置をずらして正にする
pub fn quad(image: u32, src: [f32; 4], pose: &Pose) -> Option<Quad> {
    let argb = pose.argb.map(|c| c.clamp(0.0, 255.0) as u8);
    if argb[0] < 2 && pose.blend != Blend::None {
        return None;
    }
    let [mut x, mut y, mut w, mut h] = pose.dst;
    if h < 0.0 {
        h = -h;
        y -= h;
    }
    if w < 0.0 {
        w = -w;
        x -= w;
    }
    Some(Quad {
        image,
        src,
        dst: [x, y, w, h],
        argb,
        angle: pose.angle,
        center: pose.center,
        blend: pose.blend,
        filter: pose.filter,
    })
}

// ---------------------------------------------------------------------------
// DOM-SKD-06 数字
// ---------------------------------------------------------------------------

/// 数字の並び（各桁のコマ番号。None は空白で描かない）と、左にずらす桁数（中央揃え、正なら右へ）。
/// コマ数が 10 の倍数なら 0-9、11 の倍数なら 10 番が空白、24 の倍数なら符号付き（11 = +、23 = −）。
/// 揃え 0 = 右（下位桁を残す）、1 = 左、2 = 中央。負の値は 24 の倍数のときだけ描く
pub fn number_glyphs(
    value: i64,
    align: i32,
    digits: i32,
    frames: usize,
) -> Option<(f32, Vec<Option<usize>>)> {
    if digits <= 0 || !(0..=2).contains(&align) {
        return None;
    }
    // 後の判定が勝つ（LR2 と同じ順）
    let mut set = None;
    for (n, kind) in [(10, 1), (11, 2), (24, 3)] {
        if frames.is_multiple_of(n) {
            set = Some((n, kind));
        }
    }
    let (_, kind) = set?;
    if kind != 3 && value < 0 {
        return None;
    }
    let abs = value.unsigned_abs().to_string();
    let d = digits as usize;
    let text: Vec<char> = match align {
        0 => {
            let s = format!("{abs:>10}");
            let c: Vec<char> = s.chars().collect();
            c[c.len().saturating_sub(d)..].to_vec()
        }
        _ => format!("{abs}          ").chars().take(d).collect(),
    };
    let shift = if align == 2 {
        let len = if value == 0 {
            1
        } else {
            value.to_string().len()
        };
        (digits - len as i32).max(0) as f32 * 0.5
    } else {
        0.0
    };
    let mut base = 0;
    let mut out = vec![];
    if kind == 3 {
        out.push(Some(if value < 0 { 23 } else { 11 }));
        if value < 0 && frames == 24 {
            base = 12;
        }
    }
    for ch in text {
        out.push(match ch {
            ' ' if kind == 1 => None,
            ' ' => Some(base + 10),
            c => Some(base + c.to_digit(10).unwrap() as usize),
        });
    }
    Some((shift, out))
}

/// 1 組のコマ数（10/11/24）
fn glyph_set(frames: usize) -> usize {
    [10, 11, 24]
        .into_iter()
        .rfind(|n| frames.is_multiple_of(*n))
        .unwrap_or(10)
}

// ---------------------------------------------------------------------------
// DOM-SKD-08 ゲージ
// ---------------------------------------------------------------------------

/// 50 区間それぞれのコマ（0..=3）。`hp` は HP/2、`tremble` は 0..=2 の乱数
pub fn gauge_frames(hp: i32, tremble: i32, survival: bool) -> [usize; 50] {
    let tr = hp - tremble;
    let mut out = [0; 50];
    for (k, o) in out.iter_mut().enumerate() {
        let i = k as i32 + 1;
        let steady = i < tr || hp <= i || i == 1;
        *o = if !survival && i < 40 {
            if steady && i <= hp {
                1
            } else {
                3
            }
        } else if !survival {
            if (tr <= i && i < hp && i != 1) || hp < i {
                2
            } else {
                0
            }
        } else if steady {
            if hp < i {
                2
            } else {
                0
            }
        } else {
            2
        };
    }
    out
}

// ---------------------------------------------------------------------------
// 全体
// ---------------------------------------------------------------------------

/// LR2 のノーツの速さ: 1 拍 = 480 × ハイスピード / 600 ピクセル
pub fn pixels_per_beat(hi_speed: f64) -> f64 {
    480.0 * hi_speed / 600.0
}

/// 描画順の付いたもの
struct Sorted {
    order: u32,
    sub: u8,
    d: Draw,
}

struct Ctx<'a> {
    skin: &'a Skin,
    st: &'a dyn SkinState,
    image_size: &'a dyn Fn(u32) -> Option<(u32, u32)>,
    out: Vec<Sorted>,
}

impl<'a> Ctx<'a> {
    fn push(&mut self, e: &Element, sub: u8, s: &SourceRect, frame: usize, pose: &Pose) {
        let Some(src) = src_cell(s, (self.image_size)(s.image), frame) else {
            return;
        };
        if let Some(q) = quad(s.image, src, pose) {
            self.out.push(Sorted {
                order: e.order,
                sub,
                d: Draw::Quad(q),
            });
        }
    }

    /// IMAGE / JUDGELINE など: DST のタイマーで位置、SRC のタイマーでコマ
    fn object(&mut self, e: &Element, dx: f32, dy: f32) -> Option<Pose> {
        let s = e.source.as_ref()?;
        let mut pose = pose_at(e, self.st.timer(e.timer.0))?;
        pose.dst[0] += dx;
        pose.dst[1] += dy;
        let f = src_frame(s, src_time(self.st, e, s));
        self.push(e, 0, s, f, &pose);
        Some(pose)
    }

    fn number(&mut self, e: &Element, value: i64, align: i32, digits: i32, dx: f32, dy: f32) {
        let Some(s) = e.source.as_ref() else {
            return;
        };
        let frames = frame_count(s);
        let Some((shift, glyphs)) = number_glyphs(value, align, digits, frames) else {
            return;
        };
        let Some(mut pose) = pose_at(e, self.st.timer(e.timer.0)) else {
            return;
        };
        // 動く数字: コマを 1 組ずつ進める
        let set = glyph_set(frames);
        let cyc = if s.cycle_ms > 0 {
            let t = self.st.timer(s.timer.0).unwrap_or(-1).max(0);
            (frames / set) * (t % s.cycle_ms as i64) as usize / s.cycle_ms as usize * set
        } else {
            0
        };
        pose.dst[0] += dx + shift * pose.dst[2];
        pose.dst[1] += dy;
        for g in glyphs {
            if let Some(g) = g {
                self.push(e, 0, s, cyc + g, &pose);
            }
            pose.dst[0] += pose.dst[2];
        }
    }

    fn find(&self, command: &str, index: i32) -> Option<&'a Element> {
        self.skin.elements.iter().rev().find(|e| {
            matches!(&e.kind, ElementKind::Other { command: c, index: i } if c == command && *i == index)
        })
    }
}

/// 場面によらない要素（DOM-SKD-07, 12, 17）: IMAGE・NUMBER・BARGRAPH・SLIDER・TEXT・BUTTON を条件をすべて満たすときだけ描く
fn common(cx: &mut Ctx<'_>, e: &Element) {
    let st = cx.st;
    if !ops_true(st, e) {
        return;
    }
    match &e.kind {
        ElementKind::Image => {
            let side = e.dst_extra[0];
            if side == 1 || side == 2 {
                if let (Some(s), Some(mut pose)) =
                    (e.source.as_ref(), pose_at(e, st.timer(e.timer.0)))
                {
                    pose.angle = st.turntable(side);
                    let f = src_frame(s, src_time(st, e, s));
                    cx.push(e, 0, s, f, &pose);
                }
            } else {
                cx.object(e, 0.0, 0.0);
            }
        }
        ElementKind::Number {
            value_ref,
            digits,
            align,
        } => {
            let v = st.number(*value_ref).unwrap_or(-1);
            cx.number(e, v, *align, *digits, 0.0, 0.0);
        }
        ElementKind::Bar {
            value_ref,
            direction,
        } => {
            let v = st.bar(*value_ref);
            if v <= 0.0 {
                return;
            }
            let (Some(s), Some(mut pose)) = (e.source.as_ref(), pose_at(e, st.timer(e.timer.0)))
            else {
                return;
            };
            let v = v.min(1.0) as f32;
            match direction {
                0 => pose.dst[2] *= v,
                1 => pose.dst[3] *= v,
                _ => {}
            }
            let f = src_frame(s, st.timer(s.timer.0).unwrap_or(-1));
            cx.push(e, 0, s, f, &pose);
        }
        ElementKind::Slider {
            value_ref,
            direction,
            range,
        } if *range >= 1 => {
            let off = (st.slider(*value_ref).clamp(0.0, 1.0) * *range as f64) as f32;
            let (dx, dy) = match direction {
                0 => (0.0, -off),
                1 => (off, 0.0),
                2 => (0.0, off),
                3 => (-off, 0.0),
                _ => return,
            };
            cx.object(e, dx, dy);
        }
        ElementKind::Text {
            value_ref,
            font,
            align,
        } => {
            if let Some(s) = st.text(*value_ref).filter(|s| !s.is_empty()) {
                text(cx, e, s, *font, *align, 0.0, 0.0);
            }
        }
        ElementKind::Button { action } => {
            if let Some(v) = st.button(*action) {
                button(cx, e, v);
            }
        }
        _ => {}
    }
}

/// DOM-SKD-17: BUTTON は状態値のコマ（コマ数以上なら最後、負なら 0）を DST のタイマーの位置に描く（LR2 `ButtonByInput` の `inArray`）
fn button(cx: &mut Ctx<'_>, e: &Element, value: i32) {
    let (Some(s), Some(pose)) = (e.source.as_ref(), pose_at(e, cx.st.timer(e.timer.0))) else {
        return;
    };
    let n = frame_count(s) as i32;
    let f = if value >= n { n - 1 } else { value.max(0) };
    cx.push(e, 0, s, f as usize, &pose);
}

/// プレイ画面を描くものを、奥から順に返す（DOM-SKD-07..12）
pub fn draw_play(
    skin: &Skin,
    st: &dyn SkinState,
    view: &PlayView,
    image_size: &dyn Fn(u32) -> Option<(u32, u32)>,
) -> Vec<Draw> {
    let mut cx = Ctx {
        skin,
        st,
        image_size,
        out: vec![],
    };
    let playing = st.timer(41).is_some_and(|t| t > 0);
    for e in &skin.elements {
        match &e.kind {
            ElementKind::Other { command, index } => match command.as_str() {
                "JUDGELINE" if *index == 0 => {
                    cx.object(e, 0.0, 0.0);
                }
                "GROOVEGAUGE" if *index == 0 => gauge(&mut cx, e, view),
                "LINE" if *index == 0 && playing => lines(&mut cx, e, view),
                "NOWJUDGE_1P" if playing && view.judge == Some(*index as usize) => {
                    judge_combo(&mut cx, e, view)
                }
                "BGA" if st.timer(41).is_some_and(|t| t >= 0) && ops_true(st, e) => {
                    bga(&mut cx, e, view)
                }
                _ => {}
            },
            ElementKind::NoteLane { .. } if playing => notes(&mut cx, e, view),
            _ => common(&mut cx, e),
        }
    }
    cx.out.sort_by_key(|s| (s.order, s.sub));
    cx.out.into_iter().map(|s| s.d).collect()
}

/// DOM-SKD-14: BGA。層ごとに画像全体を DST に描く。画像の幅が 256 未満なら 幅 × 画像の幅 / 256 にして
/// 中央へ寄せ、高さが 256 未満なら 高さ × 画像の高さ / 256（上揃え）。1 層目は回していなければ後ろに DST の α の黒を描き、
/// 2 層目以降はその上（描画順キーの次）に描く
fn bga(cx: &mut Ctx<'_>, e: &Element, view: &PlayView) {
    let Some(pose) = pose_at(e, cx.st.timer(e.timer.0)) else {
        return;
    };
    for b in &view.bga {
        let Some((iw, ih)) = (cx.image_size)(b.image) else {
            continue;
        };
        let mut p = pose;
        let [x, _, w, h] = &mut p.dst;
        if iw < 256 {
            *x += *w / 256.0 * (256.0 - iw as f32) * 0.5;
            *w *= iw as f32 / 256.0;
        }
        if ih < 256 {
            *h *= ih as f32 / 256.0;
        }
        let sub = if b.backbox { 0 } else { 1 };
        let Some(q) = quad(b.image, [0.0, 0.0, iw as f32, ih as f32], &p) else {
            continue;
        };
        if b.backbox && q.angle == 0.0 {
            cx.out.push(Sorted {
                order: e.order,
                sub,
                d: Draw::Fill(Fill {
                    dst: q.dst,
                    argb: [q.argb[0], 0, 0, 0],
                    blend: q.blend,
                }),
            });
        }
        cx.out.push(Sorted {
            order: e.order,
            sub,
            d: Draw::Quad(q),
        });
    }
}

/// DOM-SKD-12: 文字列。DST の幅・高さが 0 なら描かない。同じ番号の画像フォント（大きさ > 0）があればそれで、
/// 無ければ同じ番号のシステムフォントで描く。DST の角度は使わない（LR2 はそこにフォント番号を入れる）
fn text(cx: &mut Ctx<'_>, e: &Element, s: &str, font: u32, align: i32, dx: f32, dy: f32) {
    let Some(mut pose) = pose_at(e, cx.st.timer(e.timer.0)) else {
        return;
    };
    pose.angle = 0.0;
    pose.dst[0] += dx;
    pose.dst[1] += dy;
    let [x, y, w, h] = pose.dst;
    if w == 0.0 || h == 0.0 {
        return;
    }
    let skin = cx.skin;
    if let Some(Some(f)) = skin.image_fonts.get(font as usize) {
        if f.size > 0 {
            for (image, src, dst) in layout_text(f, s, [x, y, w, h], align) {
                let mut p = pose;
                p.dst = dst;
                if let Some(q) = quad(image, src, &p) {
                    cx.out.push(Sorted {
                        order: e.order,
                        sub: 0,
                        d: Draw::Quad(q),
                    });
                }
            }
            return;
        }
    }
    if let Some(Some(sf)) = skin.fonts.get(font as usize) {
        cx.out.push(Sorted {
            order: e.order,
            sub: 0,
            d: Draw::Text(TextRun {
                font: *sf,
                text: s.to_string(),
                dst: pose.dst,
                align,
                argb: pose.argb.map(|c| c.clamp(0.0, 255.0) as u8),
                blend: pose.blend,
                filter: pose.filter,
            }),
        });
    }
}

/// DOM-SKD-13: 画像フォントの並べ方（LR2 `LRDrawText`）。幅 = Σ(文字の幅 + 間隔) − 間隔（無い文字は `?` の幅）、
/// 縦の倍率 = h / 大きさ、横の倍率 = 縦の倍率 × (幅 > w なら w / 幅、それ以外 1)。揃え 1 は 幅 × 横の倍率 / 2、2 は 幅 × 横の倍率 だけ
/// 左へ（整数に切り捨て）。各文字は左から 文字の幅 × 横の倍率 と 間隔 × 横の倍率 ずつ進め、無い文字は `?` で描き、空白・改行は描かない。
/// 位置は 0.5 を足して切り捨てる。返すのは（画像, 切り出し, 位置）
pub fn layout_text(
    f: &ImageFont,
    s: &str,
    dst: [f32; 4],
    align: i32,
) -> Vec<(u32, [f32; 4], [f32; 4])> {
    let [x, y, w, h] = dst;
    let fallback = f.glyphs.get(&'?').filter(|g| g.rect[2] > 0);
    let own = |c: char| f.glyphs.get(&c).filter(|g| g.rect[2] > 0);
    let glyph = |c: char| own(c).or(fallback);
    let margin = f.margin as f32;
    let width: f32 = s
        .chars()
        .map(|c| margin + glyph(c).map_or(0.0, |g| g.rect[2] as f32))
        .sum::<f32>()
        - margin;
    if width == 0.0 {
        return vec![];
    }
    let hl = h / f.size as f32;
    let wl = if width > w { w / width } else { 1.0 } * hl;
    let x = match align {
        1 => x - (width * wl * 0.5).trunc(),
        2 => x - (width * wl).trunc(),
        _ => x,
    };
    let round = |v: f32| (v + 0.5).trunc();
    let mut out = vec![];
    let mut sum = 0.0;
    for c in s.chars() {
        // 無い文字は `?` に置き換えて描く
        let (c, g) = match own(c) {
            Some(g) => (c, Some(g)),
            None => ('?', fallback),
        };
        let xf = g.map_or(0.0, |g| g.rect[2] as f32) * wl;
        if let (Some(g), false) = (g, c == ' ' || c == '\n') {
            let (x1, x2) = (round(x + sum), round(x + sum + xf));
            let (y1, y2) = (round(y), round(y + hl * g.rect[3] as f32));
            out.push((
                g.image,
                g.rect.map(|v| v as f32),
                [x1, y1, x2 - x1, y2 - y1],
            ));
        }
        sum += margin * wl + xf;
    }
    out
}

/// DOM-SKD-11: 小節線。DST_LINE の位置から 拍数 × [`pixels_per_beat`] 上に、判定位置を過ぎていないものだけ描く。α は 255
fn lines(cx: &mut Ctx<'_>, e: &Element, view: &PlayView) {
    let (Some(s), Some(mut base)) = (e.source.as_ref(), pose_at(e, cx.st.timer(e.timer.0))) else {
        return;
    };
    base.argb[0] = 255.0;
    let ppb = pixels_per_beat(view.hi_speed) as f32;
    let f = src_frame(s, src_time(cx.st, e, s));
    for &b in view.lines.iter().filter(|b| **b >= 0.0) {
        let mut p = base;
        p.dst[1] -= b as f32 * ppb;
        if p.dst[1] + p.dst[3].abs() < 0.0 {
            continue;
        }
        cx.push(e, 0, s, f, &p);
    }
}

/// DOM-SKD-08: 50 区間を SRC の引数 1, 2 だけずらしながら描く
fn gauge(cx: &mut Ctx<'_>, e: &Element, view: &PlayView) {
    let (Some(s), Some(mut pose)) = (e.source.as_ref(), pose_at(e, cx.st.timer(e.timer.0))) else {
        return;
    };
    let frames = frame_count(s);
    let cyc = if frames > 4 && s.cycle_ms > 0 {
        let t = cx.st.timer(s.timer.0).unwrap_or(-1);
        (frames / 4) as i64 * (t % s.cycle_ms as i64) / s.cycle_ms as i64 * 4
    } else {
        0
    };
    for f in gauge_frames(view.gauge, view.tremble, view.survival) {
        let frame = cyc + f as i64;
        if (0..frames as i64).contains(&frame) {
            cx.push(e, 0, s, frame as usize, &pose);
        }
        pose.dst[0] += s.params[0] as f32;
        pose.dst[1] += s.params[1] as f32;
    }
}

/// DOM-SKD-09: 判定の文字とコンボ。GD 未満ならコンボは 0（描かない）。
/// コンボは判定の位置からの相対で、SRC 引数 1 が 0 なら判定をコンボの桁数 × 幅/2 だけ左へずらす
fn judge_combo<'a>(cx: &mut Ctx<'a>, je: &'a Element, view: &PlayView) {
    let index = view.judge.unwrap_or(0) as i32;
    let combo = if index < 3 { 0 } else { view.combo };
    let ce = cx.find("NOWCOMBO_1P", index);
    let t46 = cx.st.timer(46);
    let digits = if combo > 0 {
        combo.to_string().len() as f32
    } else {
        0.0
    };
    let cw = ce.and_then(|c| pose_at(c, t46)).map_or(0.0, |p| p.dst[2]);
    let noshift = je.source.as_ref().map_or(0, |s| s.params[0]);
    let (pos, posn) = if noshift == 0 && combo > 0 {
        (-(digits * cw * 0.5), 0.0)
    } else {
        (0.0, -((digits - 1.0) * cw * 0.5))
    };
    // 判定の文字はタイマー 46 で動かす
    let (Some(s), Some(mut pose)) = (je.source.as_ref(), pose_at(je, t46)) else {
        return;
    };
    pose.dst[0] += pos;
    let f = src_frame(
        s,
        t46.map_or(-1, |t| {
            t - je.keyframes.first().map_or(0, |k| k.t_ms as i64)
        }),
    );
    cx.push(je, 0, s, f, &pose);
    if combo > 0 {
        if let (Some(ce), Some(jp)) = (ce, pose_at(je, t46)) {
            let (align, digits) = ce
                .source
                .as_ref()
                .map_or((0, 0), |s| (s.params[1], s.params[2]));
            cx.number(ce, combo, align, digits, pos + posn + jp.dst[0], jp.dst[1]);
        }
    }
}

/// DOM-SKD-10: ノーツ。DST_NOTE の位置が判定位置で、そこから上へ 拍数 × [`pixels_per_beat`] ずらす
/// （通過したものは判定位置に留める）。α は 255、合成は Alpha。LN は 胴・終端・始端 の順に重ね、
/// 押していない LN はコマ 0
fn notes<'a>(cx: &mut Ctx<'a>, ne: &'a Element, view: &PlayView) {
    let ElementKind::NoteLane { lane } = ne.kind else {
        return;
    };
    let Some(mut base) = pose_at(ne, cx.st.timer(ne.timer.0)) else {
        return;
    };
    base.argb[0] = 255.0;
    base.blend = Blend::Alpha;
    base.filter = false;
    let ppb = pixels_per_beat(view.hi_speed) as f32;
    let at = |beats: f64| {
        let mut p = base;
        p.dst[1] -= (beats.max(0.0) as f32) * ppb;
        p
    };
    let src_of = |cmd: &str| -> Option<(&'a Element, &'a SourceRect)> {
        let e = if cmd == "NOTE" {
            Some(ne)
        } else {
            cx.find(cmd, lane)
        }?;
        Some((e, e.source.as_ref()?))
    };
    let mut parts: Vec<(&'a Element, u8, &'a SourceRect, usize, Pose)> = vec![];
    for n in view.notes.iter().filter(|n| n.lane == lane) {
        let head = at(n.head);
        // 画面より上は描かない
        if head.dst[1] + head.dst[3].abs() < 0.0 {
            continue;
        }
        let frame = |e: &Element, s: &SourceRect, animate: bool| {
            if animate {
                src_frame(s, src_time(cx.st, e, s))
            } else {
                0
            }
        };
        if n.mine {
            if let Some((e, s)) = src_of("MINE") {
                parts.push((ne, 0, s, frame(e, s, true), head));
            }
        } else if let Some(tail) = n.tail {
            let tl = at(tail);
            let h = base.dst[3];
            if let Some((e, s)) = src_of("LN_BODY") {
                let mut body = tl;
                body.dst[1] = tl.dst[1] + h;
                body.dst[3] = head.dst[1] - tl.dst[1] - h;
                parts.push((ne, 0, s, frame(e, s, n.held), body));
            }
            if let Some((e, s)) = src_of("LN_END") {
                parts.push((ne, 1, s, frame(e, s, n.held), tl));
            }
            if let Some((e, s)) = src_of("LN_START") {
                parts.push((ne, 2, s, frame(e, s, n.held), head));
            }
        } else if let Some((e, s)) = src_of("NOTE") {
            parts.push((ne, 0, s, frame(e, s, true), head));
        }
    }
    for (e, sub, s, f, pose) in parts {
        cx.push(e, sub, s, f, &pose);
    }
}

// ---------------------------------------------------------------------------
// 選曲画面（DOM-SKD-15..18）
// ---------------------------------------------------------------------------

/// 選曲の一覧の 1 本（LR2 の `SONGDATA` のうち曲バーを描くのに使うもの）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BarView {
    /// バーの種類（LR2 の `folderType`: 0 = 曲、1 = フォルダ …）。BAR_BODY の添字
    pub kind: i32,
    /// 題名（LR2 の `fulltitle`: 題名 + 空白 + 副題）
    pub title: String,
    /// 鍵盤数（フォルダは 0）
    pub key_mode: i32,
    pub level: i32,
    /// `#DIFFICULTY`
    pub difficulty: i32,
    /// ランプ（LR2 の `clear`: 0 = NO PLAY … 5 = FULL COMBO）
    pub lamp: i32,
}

/// 選曲画面に固有の描画材料
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectView<'a> {
    /// 一覧の位置（LR2 の `listCalculatedBar`。1 本 = 1000、0..本数 × 1000）
    pub position: i64,
    /// 選んでいるバーを置いたスロット（LR2 の `listSelectedBarFromScreenTop`）
    pub slot: i32,
    /// 一覧のすべてのバー
    pub bars: &'a [BarView],
}

/// スロット `slot` に出す一覧の添字（LR2: (位置 / 1000 − BAR_CENTER + スロット) を本数で回す）
pub fn slot_bar(position: i64, center: i32, slot: i32, n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    let n = n as i64;
    let mut bar = position / 1000 - center as i64 + slot as i64;
    while bar < 0 {
        bar += n;
    }
    Some((bar % n) as usize)
}

/// LR2 の `ByTime`（DSTDbyTime の補間）: v1 = v2 なら v1、t1 ≤ now ≤ t2 なら直線、それ以外は端の値
fn by_time(v1: f64, v2: f64, t1: f64, t2: f64, now: f64) -> f64 {
    if v1 == v2 {
        return v1;
    }
    if t1 <= now && now <= t2 && t1 < t2 {
        let r = (now - t1) / (t2 - t1);
        return (1.0 - r) * v1 + r * v2;
    }
    if now <= t1 {
        v1
    } else {
        v2
    }
}

impl<'a> Ctx<'a> {
    fn find_title(&self, index: i32) -> Option<(&'a Element, u32, i32)> {
        self.skin.elements.iter().rev().find_map(|e| match e.kind {
            ElementKind::BarTitle {
                index: i,
                font,
                align,
            } if i == index => Some((e, font, align)),
            _ => None,
        })
    }
    /// 名前だけで探す（添字を見ない。`#SRC_BAR_FLASH` は 1 つだけ）
    fn find_any(&self, command: &str) -> Option<&'a Element> {
        self.skin
            .elements
            .iter()
            .rev()
            .find(|e| matches!(&e.kind, ElementKind::Other { command: c, .. } if c == command))
    }
}

/// 曲バーのスロットの位置（タイマー 0 で引く。`on` なら BAR_BODY_ON）と描画順。DST が無い・描かない時刻なら None
fn slot_pose(cx: &Ctx<'_>, slot: i32, on: bool) -> Option<(Pose, u32)> {
    let name = if on { "BAR_BODY_ON" } else { "BAR_BODY_OFF" };
    let e = cx.find(name, slot)?;
    Some((pose_at(e, cx.st.timer(0))?, e.order))
}

/// DOM-SKD-15, 16, 19: 曲バー（OpenLR2 `Scene02_Songselect.cpp` の曲バーの描画）
fn select_bars(cx: &mut Ctx<'_>, view: &SelectView<'_>) {
    let n = view.bars.len();
    let center = cx.skin.bars.center;
    let has_off = |cx: &Ctx<'_>, i: i32| {
        cx.find("BAR_BODY_OFF", i)
            .is_some_and(|e| !e.keyframes.is_empty())
    };
    let frac = view.position % 1000;
    for i in 1..30 {
        if !(has_off(cx, i - 1) && has_off(cx, i)) {
            continue;
        }
        let Some(b) = slot_bar(view.position, center, i, n) else {
            return;
        };
        let bar = &view.bars[b];
        let d1 = slot_pose(cx, i, i == view.slot);
        let d2 = slot_pose(cx, i - 1, i - 1 == view.slot);
        let (Some((p1, o1)), Some((p2, o2))) = (d1, d2) else {
            continue;
        };
        // 位置の端数でスロット i と i−1 の間を補間する（合成・フィルタ・中心は i のもの）
        let (p3, o3) = if frac == 0 {
            (p1, o1)
        } else {
            let f = |a: f32, b: f32| by_time(a as f64, b as f64, 0.0, 1000.0, frac as f64) as f32;
            let mut p = p1;
            p.dst = std::array::from_fn(|k| f(p1.dst[k], p2.dst[k]));
            p.argb = std::array::from_fn(|k| f(p1.argb[k], p2.argb[k]));
            p.angle = f(p1.angle, p2.angle);
            (
                p,
                by_time(o1 as f64, o2 as f64, 0.0, 1000.0, frac as f64) as u32,
            )
        };
        // 本体: バーの種類の SRC を、その SRC のタイマーでコマ送り
        if let Some(be) = cx.find("BAR_BODY", bar.kind) {
            if let Some(s) = be.source.as_ref() {
                let f = src_frame(s, cx.st.timer(s.timer.0).unwrap_or(-1));
                if let Some(src) = src_cell(s, (cx.image_size)(s.image), f) {
                    if let Some(q) = quad(s.image, src, &p3) {
                        cx.out.push(Sorted {
                            order: o3,
                            sub: 0,
                            d: Draw::Quad(q),
                        });
                    }
                }
            }
        }
        // ほかの部品はバーの位置からの相対（LR2 は整数に切り捨てて足す）
        let (dx, dy) = (p3.dst[0].trunc(), p3.dst[1].trunc());
        if i == view.slot {
            if let Some(fe) = cx.find_any("BAR_FLASH") {
                cx.object(fe, p1.dst[0].trunc(), p1.dst[1].trunc());
            }
        }
        // 題名は通常（0）だけ（新着（1）は追加日時が無いので使わない）
        if let Some((te, font, align)) = cx.find_title(0) {
            if !bar.title.is_empty() {
                text(cx, te, &bar.title, font, align, dx, dy);
            }
        }
        // レベル: 曲（種類 0・5）で鍵盤数があるとき、#DIFFICULTY の添字（0 未満なら 0）で
        if (bar.kind == 0 || bar.kind == 5) && bar.key_mode > 0 {
            let idx = if bar.difficulty >= 0 && bar.kind <= 5 {
                bar.difficulty
            } else {
                0
            };
            if let Some(le) = cx.find("BAR_LEVEL", idx) {
                let (align, digits) = le
                    .source
                    .as_ref()
                    .map_or((0, 0), |s| (s.params[1], s.params[2]));
                cx.number(le, bar.level as i64, align, digits, dx, dy);
            }
        }
        if let Some(le) = cx.find("BAR_LAMP", bar.lamp) {
            cx.object(le, dx, dy);
        }
    }
}

/// 選曲画面を描くものを、奥から順に返す（DOM-SKD-07, 12, 15..17）
pub fn draw_select(
    skin: &Skin,
    st: &dyn SkinState,
    view: &SelectView<'_>,
    image_size: &dyn Fn(u32) -> Option<(u32, u32)>,
) -> Vec<Draw> {
    let mut cx = Ctx {
        skin,
        st,
        image_size,
        out: vec![],
    };
    for e in &skin.elements {
        common(&mut cx, e);
    }
    select_bars(&mut cx, view);
    cx.out.sort_by_key(|s| (s.order, s.sub));
    cx.out.into_iter().map(|s| s.d).collect()
}

/// キー設定画面を描くものを、奥から順に返す（DOM-SKD-20）。場面に固有の部品は無く、場面によらない要素だけを描く
pub fn draw_keyconfig(
    skin: &Skin,
    st: &dyn SkinState,
    image_size: &dyn Fn(u32) -> Option<(u32, u32)>,
) -> Vec<Draw> {
    let mut cx = Ctx {
        skin,
        st,
        image_size,
        out: vec![],
    };
    for e in &skin.elements {
        common(&mut cx, e);
    }
    cx.out.sort_by_key(|s| (s.order, s.sub));
    cx.out.into_iter().map(|s| s.d).collect()
}

/// DOM-SKD-18: マウスが DST の上にあるか（LR2 `MouseOnDSTD`）。画面の外・DST の外は 0、DST の右半分（中央を含む）は 1、左半分は 2
pub fn mouse_on(dst: [f32; 4], res: (u32, u32), x: i32, y: i32) -> i32 {
    let [dx, dy, w, h] = dst;
    let (fx, fy) = (x as f32, y as f32);
    let inside_screen = (0..res.0 as i32).contains(&x) && (0..res.1 as i32).contains(&y);
    if inside_screen && dx <= fx && fx <= w + dx && dy <= fy && fy <= h + dy {
        if fx >= w * 0.5 + dx {
            1
        } else {
            2
        }
    } else {
        0
    }
}

/// 曲バーのスロットの DST（タイマー 0 で引く。`on` なら BAR_BODY_ON）。マウスでバーを選ぶのに使う
pub fn bar_slot_dst(skin: &Skin, st: &dyn SkinState, slot: i32, on: bool) -> Option<[f32; 4]> {
    let name = if on { "BAR_BODY_ON" } else { "BAR_BODY_OFF" };
    let e = skin.elements.iter().rev().find(|e| {
        matches!(&e.kind, ElementKind::Other { command: c, index: i } if c == name && *i == slot)
    })?;
    Some(pose_at(e, st.timer(0))?.dst)
}

/// BUTTON のクリック（LR2 `ButtonByInput` が 2 を返すもの）: 動作と向き（+1 / −1）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonClick {
    pub action: i32,
    pub delta: i32,
}

/// DOM-SKD-18: 押した（LR2 のマウスの状態 1）位置の BUTTON。条件をすべて満たし、DST のタイマーが動いていて、
/// SRC 引数 12（クリック）が 1、引数 13（パネル）が 0 か今のパネルのもの。引数 14 が 0 なら右半分で +1・左半分で −1、
/// 1 ならどこでも +1、2 ならどこでも −1
pub fn button_clicks(
    skin: &Skin,
    st: &dyn SkinState,
    x: i32,
    y: i32,
    panel: i32,
) -> Vec<ButtonClick> {
    let mut out = vec![];
    for e in &skin.elements {
        let ElementKind::Button { action } = e.kind else {
            continue;
        };
        let Some(s) = e.source.as_ref() else {
            continue;
        };
        if !ops_true(st, e) {
            continue;
        }
        let Some(t) = st.timer(e.timer.0) else {
            continue;
        };
        let Some(pose) = pose_at(e, Some(t)) else {
            continue;
        };
        let [_, click, p, side, _] = s.params;
        let m = mouse_on(pose.dst, skin.resolution, x, y);
        let delta = if (m == 1 && side == 0) || (m != 0 && side == 1) {
            1
        } else if (m == 2 && side == 0) || (m != 0 && side == 2) {
            -1
        } else {
            continue;
        };
        if click == 1 && (p == panel || p == 0) {
            out.push(ButtonClick { action, delta });
        }
    }
    out
}
