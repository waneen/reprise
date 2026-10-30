//! プレイオプションとプレイ中の操作（APP-OPT-*）。LR2 の `LR2_gameloop.cpp`（START/SELECT を押しながらの
//! ハイスピード・レーンカバーの変更、START/SELECT の 2 度押しでのレーンカバーの表示切り替え）と
//! `LR2_configsave.cpp`（既定値）を仕様として参照した。
use domain::play::HsFix;

/// ハイスピードの最小・最大・刻み（LR2 の `hsmin`・`hsmax`・`hsmargin` の既定）
const HS_MIN: i32 = 10;
const HS_MAX: i32 = 900;
const HS_STEP: i32 = 10;
/// レーンカバーの刻み（LR2 の `shuttermargin` の既定）
const COVER_STEP: i32 = 10;
/// レーンカバーの表示を切り替える 2 度押しの間隔（ms 以下）
const DOUBLE_PRESS_MS: i64 = 250;
/// 判定タイミングの範囲（LR2 の設定画面の範囲）
const OFFSET_RANGE: i32 = 199;

/// プレイオプション（LR2 の設定の値）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayOptions {
    /// ハイスピード（×100。LR2 の既定は 100）
    pub hi_speed: i32,
    pub hs_fix: HsFix,
    /// レーンカバーを使う
    pub lane_cover: bool,
    /// レーンカバーの量（0..=100）
    pub cover: i32,
    /// オートプレイ
    pub auto: bool,
    /// 判定タイミング（ms。正ならノーツを早く描く）
    pub judge_offset: i32,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            hi_speed: 100,
            hs_fix: HsFix::Off,
            lane_cover: false,
            cover: 0,
            auto: false,
            judge_offset: 0,
        }
    }
}

impl PlayOptions {
    /// 値を範囲に収める（ハイスピード 10..=900、レーンカバー 0..=100、判定タイミング −199..=199）
    pub fn clamped(mut self) -> Self {
        self.hi_speed = self.hi_speed.clamp(HS_MIN, HS_MAX);
        self.cover = self.cover.clamp(0, 100);
        self.judge_offset = self.judge_offset.clamp(-OFFSET_RANGE, OFFSET_RANGE);
        self
    }
}

/// そのフレームの操作（START・SELECT・鍵盤 1..=7・マウスホイール）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ControlInput {
    /// このフレームで押した
    pub start_pressed: bool,
    pub select_pressed: bool,
    /// 前のフレームから押し続けている
    pub start_held: bool,
    pub select_held: bool,
    /// このフレームで押した鍵盤（添字 0 = 鍵盤 1）
    pub keys_pressed: [bool; 7],
    /// ホイールの回転（上が正）
    pub wheel: i32,
}

/// プレイ中に変わるオプションの状態
#[derive(Debug, Clone)]
pub struct OptionState {
    pub options: PlayOptions,
    /// レーンカバーを表示している（2 度押しで切り替わる）
    pub cover_shown: bool,
    /// START か SELECT を押し続けている
    pub holding: bool,
    /// 2 度押しの 1 度目の時刻
    first_press: Option<i64>,
}

impl OptionState {
    pub fn new(options: PlayOptions) -> Self {
        Self {
            options: options.clamped(),
            cover_shown: true,
            holding: false,
            first_press: None,
        }
    }

    /// レーンカバーが効いている（使う設定で、表示している）
    pub fn cover_active(&self) -> bool {
        self.options.lane_cover && self.cover_shown
    }

    /// 1 フレームの操作を反映する（APP-OPT-01, 02）
    pub fn control(&mut self, t_ms: i64, c: &ControlInput) {
        let o = &mut self.options;
        // ホイール: レーンカバーを使うなら量を変える
        if o.lane_cover {
            o.cover -= c.wheel;
        }
        self.holding = c.start_held || c.select_held;
        if self.holding {
            let k = |i: usize| c.keys_pressed[i - 1];
            if k(1) || k(3) || k(5) {
                o.hi_speed -= HS_STEP;
            }
            if k(2) || k(4) {
                o.hi_speed += HS_STEP;
            }
            if o.lane_cover && self.cover_shown {
                if k(6) {
                    o.cover -= COVER_STEP;
                }
                if k(7) {
                    o.cover += COVER_STEP;
                }
            } else {
                if k(6) {
                    o.hi_speed += HS_STEP;
                }
                if k(7) {
                    o.hi_speed -= HS_STEP;
                }
            }
        }
        // START か SELECT の片方だけの 2 度押しでレーンカバーの表示を切り替える
        let single = (c.start_pressed && !c.select_pressed && !c.select_held)
            || (c.select_pressed && !c.start_pressed && !c.start_held);
        if single {
            match self.first_press {
                Some(t0) if t_ms - t0 <= DOUBLE_PRESS_MS => {
                    self.first_press = None;
                    self.cover_shown = !self.cover_shown;
                }
                _ => self.first_press = Some(t_ms),
            }
        }
        self.options = self.options.clamped();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// START を押し続けて鍵盤 `keys`（1..=7）を押す
    fn hold(start: bool, select: bool, keys: &[usize]) -> ControlInput {
        let mut c = ControlInput {
            start_held: start,
            select_held: select,
            ..Default::default()
        };
        for k in keys {
            c.keys_pressed[k - 1] = true;
        }
        c
    }
    fn opts(hi_speed: i32, lane_cover: bool, cover: i32) -> PlayOptions {
        PlayOptions {
            hi_speed,
            lane_cover,
            cover,
            ..Default::default()
        }
    }

    /// APP-OPT-01: START か SELECT を押し続けて 鍵盤 1/3/5 で −10、2/4 で +10、6/7 は（レーンカバーが効いていなければ）+10/−10。
    /// 同じフレームの同じ向きは 1 回。押した瞬間の START では変わらない。10..=900 に収める
    #[rstest]
    #[case::minus(opts(100, false, 0), hold(true, false, &[1]), 90)]
    #[case::plus(opts(100, false, 0), hold(true, false, &[4]), 110)]
    #[case::same_direction_once(opts(100, false, 0), hold(true, false, &[1, 3, 5]), 90)]
    #[case::both_directions(opts(100, false, 0), hold(true, false, &[1, 2]), 100)]
    #[case::key6_plus(opts(100, false, 0), hold(true, false, &[6]), 110)]
    #[case::key7_minus(opts(100, false, 0), hold(false, true, &[7]), 90)]
    #[case::not_holding(opts(100, false, 0), hold(false, false, &[2]), 100)]
    #[case::start_just_pressed(opts(100, false, 0), ControlInput { start_pressed: true, keys_pressed: [false, true, false, false, false, false, false], ..Default::default() }, 100)]
    #[case::clamp_min(opts(10, false, 0), hold(true, false, &[1]), 10)]
    #[case::clamp_max(opts(900, false, 0), hold(true, false, &[2]), 900)]
    fn app_opt_01_hi_speed(#[case] o: PlayOptions, #[case] c: ControlInput, #[case] want: i32) {
        let mut st = OptionState::new(o);
        st.control(0, &c);
        assert_eq!(st.options.hi_speed, want);
    }

    /// APP-OPT-02: レーンカバー。効いていれば START/SELECT を押し続けて 鍵盤 6 で −10、7 で +10（ハイスピードは変わらない）。
    /// 使う設定ならホイールで 上に回した分だけ減らす。0..=100 に収める。START か SELECT の片方だけを 250ms 以内に 2 度押すと表示を切り替える
    #[rstest]
    #[case::key6(opts(100, true, 50), &[(0, hold(true, false, &[6]))], 40, 100, true)]
    #[case::key7(opts(100, true, 50), &[(0, hold(true, false, &[7]))], 60, 100, true)]
    #[case::wheel(opts(100, true, 50), &[(0, ControlInput { wheel: 3, ..Default::default() })], 47, 100, true)]
    #[case::wheel_without_cover(opts(100, false, 50), &[(0, ControlInput { wheel: 3, ..Default::default() })], 50, 100, true)]
    #[case::clamp(opts(100, true, 95), &[(0, hold(true, false, &[7]))], 100, 100, true)]
    #[case::double_press_hides(opts(100, true, 50), &[(0, ControlInput { start_pressed: true, ..Default::default() }), (250, ControlInput { start_pressed: true, ..Default::default() })], 50, 100, false)]
    #[case::slow_double_press(opts(100, true, 50), &[(0, ControlInput { start_pressed: true, ..Default::default() }), (251, ControlInput { start_pressed: true, ..Default::default() })], 50, 100, true)]
    #[case::other_button_held(opts(100, true, 50), &[(0, ControlInput { start_pressed: true, select_held: true, ..Default::default() }), (100, ControlInput { start_pressed: true, select_held: true, ..Default::default() })], 50, 100, true)]
    #[case::hidden_cover_changes_speed(opts(100, true, 50), &[(0, ControlInput { select_pressed: true, ..Default::default() }), (100, ControlInput { select_pressed: true, ..Default::default() }), (500, hold(false, true, &[6]))], 50, 110, false)]
    fn app_opt_02_lane_cover(
        #[case] o: PlayOptions,
        #[case] frames: &[(i64, ControlInput)],
        #[case] cover: i32,
        #[case] hi_speed: i32,
        #[case] shown: bool,
    ) {
        let mut st = OptionState::new(o);
        for (t, c) in frames {
            st.control(*t, c);
        }
        assert_eq!(
            (st.options.cover, st.options.hi_speed, st.cover_shown),
            (cover, hi_speed, shown)
        );
    }
}
