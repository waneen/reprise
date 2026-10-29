//! LR2 (DxLib) 互換乱数とレーン RANDOM。
//! DxLib `GetRand` = MT19937 + 旧 `sgenrand`（69069 LCG）初期化 + 乗算スケーリング。
//!
//! アルゴリズム: Mersenne Twister（M. Matsumoto & T. Nishimura）を本プロジェクトで独自に実装。
//! LR2 の挙動（シード範囲・消費順）は OpenLR2 の調査結果を仕様として参照（コードの転載はしていない）。

const N: usize = 624;
const M: usize = 397;

#[derive(Clone)]
pub struct LegacyMt {
    mt: [u32; N],
    idx: usize,
}

impl std::fmt::Debug for LegacyMt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LegacyMt")
            .field("idx", &self.idx)
            .finish_non_exhaustive()
    }
}

impl LegacyMt {
    /// DOM-RNG-01
    pub fn new(mut seed: u32) -> Self {
        let mut mt = [0u32; N];
        for v in mt.iter_mut() {
            *v = seed & 0xffff_0000;
            seed = seed.wrapping_mul(69069).wrapping_add(1);
            *v |= (seed & 0xffff_0000) >> 16;
            seed = seed.wrapping_mul(69069).wrapping_add(1);
        }
        Self { mt, idx: N }
    }
    fn generate(&mut self) {
        for k in 0..N {
            let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % N] & 0x7fff_ffff);
            self.mt[k] = self.mt[(k + M) % N] ^ (y >> 1) ^ if y & 1 != 0 { 0x9908_b0df } else { 0 };
        }
        self.idx = 0;
    }
    pub fn next_u32(&mut self) -> u32 {
        if self.idx >= N {
            self.generate();
        }
        let mut y = self.mt[self.idx];
        self.idx += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }
    /// DOM-RNG-02: DxLib `GetRand(max)`（`0..=max`）
    pub fn get_rand(&mut self, max: u32) -> u32 {
        ((self.next_u32() as u64 * (max as u64 + 1)) >> 32) as u32
    }
}

/// DOM-RNG-03: レーン RANDOM（皿非対象）。戻り値 `t[1..=keys]` = 元レーン番号
pub fn lane_random(rng: &mut LegacyMt, keys: usize) -> Vec<usize> {
    let mut t: Vec<usize> = (0..=keys).collect();
    for c in 1..keys {
        let a = c + rng.get_rand((keys - c) as u32) as usize;
        t.swap(c, a);
    }
    t
}

/// DOM-RNG-04: LR2 の表示用配置番号（例 1234567）
pub fn layout_number(t: &[usize], keys: usize) -> u64 {
    (1..=keys)
        .map(|i| 10u64.pow((keys - t[i]) as u32) * i as u64)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// DOM-RNG-01: 同じシードは同じ列、違うシードは違う列
    #[rstest]
    #[case::same_seed(391, 391, true)]
    #[case::different_seed(391, 392, false)]
    #[case::zero_vs_one(0, 1, false)]
    fn dom_rng_01_seed_determines_sequence(#[case] a: u32, #[case] b: u32, #[case] same: bool) {
        let take = |s| {
            let mut r = LegacyMt::new(s);
            (0..1000).map(|_| r.next_u32()).collect::<Vec<_>>() // 624 を跨いで再生成も通す
        };
        assert_eq!(take(a) == take(b), same);
    }

    /// DOM-RNG-02: 乗算スケーリングの範囲
    #[rstest]
    #[case::max0_always0(0)]
    #[case::max1(1)]
    #[case::max6(6)]
    #[case::max_0x7ffe(0x7ffe)]
    fn dom_rng_02_get_rand_range(#[case] max: u32) {
        let mut r = LegacyMt::new(12345);
        let mut raw = LegacyMt::new(12345);
        for _ in 0..2000 {
            let v = r.get_rand(max);
            let expected = ((raw.next_u32() as u64 * (max as u64 + 1)) >> 32) as u32;
            assert!(v <= max);
            assert_eq!(v, expected);
        }
    }

    /// DOM-RNG-03: レーン RANDOM（値は OpenLR2 対応表で検証済みの組）
    #[rstest]
    #[case::seed391_identity(391, 7, vec![0, 1, 2, 3, 4, 5, 6, 7])]
    #[case::seed3884_swap_6_7(3884, 7, vec![0, 1, 2, 3, 4, 5, 7, 6])]
    fn dom_rng_03_lane_random(
        #[case] seed: u32,
        #[case] keys: usize,
        #[case] expected: Vec<usize>,
    ) {
        assert_eq!(lane_random(&mut LegacyMt::new(seed), keys), expected);
    }

    /// DOM-RNG-04: 配置番号
    #[rstest]
    #[case::identity(vec![0, 1, 2, 3, 4, 5, 6, 7], 7, 1234567)]
    #[case::swap_last(vec![0, 1, 2, 3, 4, 5, 7, 6], 7, 1234576)]
    #[case::mirror(vec![0, 7, 6, 5, 4, 3, 2, 1], 7, 7654321)]
    #[case::five_keys(vec![0, 2, 1, 3, 4, 5], 5, 21345)]
    fn dom_rng_04_layout_number(#[case] t: Vec<usize>, #[case] keys: usize, #[case] expected: u64) {
        assert_eq!(layout_number(&t, keys), expected);
    }

    /// DOM-RNG-05: OpenLR2 の対応表（LR2/Unrandomizer_SeedMap7K.h）と全件照合。
    /// 対応表はライセンス不明のため同梱しない。
    /// `OPENLR2_SEEDMAP7K=path/to/Unrandomizer_SeedMap7K.h cargo test -- --ignored` で実行
    #[rstest]
    #[case::seed_map_7k("OPENLR2_SEEDMAP7K")]
    #[ignore]
    fn dom_rng_05_matches_openlr2_seed_map(#[case] env: &str) {
        let path = std::env::var(env).expect("環境変数で対応表のパスを指定する");
        let src = std::fs::read_to_string(path).unwrap();
        let mut n = 0;
        for line in src.lines() {
            let Some(rest) = line.trim().strip_prefix("case ") else {
                continue;
            };
            let Some((layout, seed)) = rest.split_once(": return ") else {
                continue;
            };
            let layout: u64 = layout.parse().unwrap();
            let seed: u32 = seed.trim_end_matches(';').parse().unwrap();
            assert_eq!(
                layout_number(&lane_random(&mut LegacyMt::new(seed), 7), 7),
                layout,
                "seed {seed}"
            );
            n += 1;
        }
        assert!(n > 5000, "parsed {n}");
    }
}
