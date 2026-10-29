//! ドメイン層。ゲームのルールそのもの。Bevy・ファイル形式・DB を知らない。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | DOM-RNG-01 | `LegacyMt` は旧 `sgenrand`（69069 の線形合同法）で状態を初期化した MT19937。同じシードは同じ列 |
//! | DOM-RNG-02 | `get_rand(max)` は `(u32 出力 × (max+1)) >> 32` で `0..=max` を返す（DxLib `GetRand`） |
//! | DOM-RNG-03 | レーン RANDOM: `c = 1..keys-1` について `a = c + get_rand(keys-c)` と入れ替える（皿は対象外） |
//! | DOM-RNG-04 | 配置番号: 位置 i に元レーン t[i] が来るとき Σ i·10^(keys−t[i]) |
//! | DOM-RNG-05 | OpenLR2 のシード対応表と全件一致（対応表は同梱せず、手元で任意実行） |
//! | DOM-SCR-01 | 取り込みマージ: ランプは良い方 |
//! | DOM-SCR-02 | 取り込みマージ: EX スコアが高い方の判定内訳を採る（同点は既存を残す） |
//! | DOM-SCR-03 | 取り込みマージ: 最大コンボは大きい方 |
//! | DOM-SCR-04 | 取り込みマージ: 最小 BP は小さい方（片方だけあればそれ） |
//! | DOM-SCR-05 | 取り込みマージ: プレイ回数は合算 |
pub mod chart;
pub mod play;
pub mod random;
pub mod score;
pub mod skin;
