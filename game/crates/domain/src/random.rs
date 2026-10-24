//! LR2 (DxLib) 互換乱数とレーンRANDOM。
//! DxLib GetRand = MT19937 + 旧sgenrand(69069 LCG)初期化 + 乗算スケーリング。
//!
//! アルゴリズム: Mersenne Twister（M. Matsumoto & T. Nishimura）を本プロジェクトで独自に実装。
//! LR2の挙動（シード範囲・消費順）は OpenLR2 の調査結果を仕様として参照（コードの転載はしていない）。

const N: usize = 624;
const M: usize = 397;

pub struct LegacyMt {
    mt: [u32; N],
    idx: usize,
}

impl LegacyMt {
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
    /// DxLib GetRand(max): 0..=max
    pub fn get_rand(&mut self, max: u32) -> u32 {
        ((self.next_u32() as u64 * (max as u64 + 1)) >> 32) as u32
    }
}

/// 7鍵/5鍵 RANDOM（皿非対象）。戻り値 t[1..=keys] = 元レーン番号。
pub fn lane_random(rng: &mut LegacyMt, keys: usize) -> Vec<usize> {
    let mut t: Vec<usize> = (0..=keys).collect();
    for c in 1..keys {
        let a = c + rng.get_rand((keys - c) as u32) as usize;
        t.swap(c, a);
    }
    t
}

/// LR2の表示用レイアウト番号（例 1234567）
pub fn layout_number(t: &[usize], keys: usize) -> u64 {
    (1..=keys)
        .map(|i| 10u64.pow((keys - t[i]) as u32) * i as u64)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自前実装の回帰テスト（値は本実装で生成。外部データは同梱しない）
    #[test]
    fn regression_7k_layouts() {
        let cases = [
            (0u32, 0u64),
            (1, 0),
            (391, 1234567),
            (3884, 1234576),
            (32766, 0),
        ];
        for (seed, expect) in cases {
            let got = layout_number(&lane_random(&mut LegacyMt::new(seed), 7), 7);
            if expect != 0 {
                assert_eq!(got, expect, "seed {seed}");
            }
            // 7レーンの順列であること
            let mut d: Vec<char> = got.to_string().chars().collect();
            d.sort();
            assert_eq!(d.iter().collect::<String>(), "1234567", "seed {seed}");
        }
    }

    /// OpenLR2 の対応表（LR2/Unrandomizer_SeedMap7K.h）との全件照合。
    /// 対応表はライセンス不明のため同梱しない。手元に置いて
    /// `OPENLR2_SEEDMAP7K=path/to/Unrandomizer_SeedMap7K.h cargo test -- --ignored` で実行。
    #[test]
    #[ignore]
    fn matches_openlr2_seedmap_7k() {
        let path = std::env::var("OPENLR2_SEEDMAP7K").expect("set OPENLR2_SEEDMAP7K");
        let src = std::fs::read_to_string(path).unwrap();
        let mut n = 0;
        for line in src.lines() {
            let l = line.trim();
            let Some(rest) = l.strip_prefix("case ") else {
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
