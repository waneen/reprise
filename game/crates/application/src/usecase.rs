use crate::ports::*;
use domain::{chart::Chart, random::LegacyMt};
use std::path::Path;

/// 譜面を読み込み、同じ乱数列を続けてレーンRANDOMに使えるよう rng も返す（LR2互換）
pub fn load_chart(
    loaders: &[Box<dyn ChartLoader>],
    path: &Path,
    bytes: &[u8],
    seed: u32,
) -> Result<(Chart, LegacyMt), PortError> {
    let l = loaders
        .iter()
        .find(|l| l.can_load(path))
        .ok_or(PortError::Unsupported)?;
    let mut rng = LegacyMt::new(seed);
    let chart = l.load(path, bytes, &mut rng)?;
    Ok((chart, rng))
}

/// ② 取り込み：ドメインのマージ規則を適用して保存
pub fn import_scores(
    importer: &dyn ScoreImporter,
    src: &Path,
    repo: &mut dyn ScoreRepository,
) -> Result<usize, PortError> {
    let recs = importer.import(src)?;
    let n = recs.len();
    for r in recs {
        match repo.find(&r.chart, r.ruleset) {
            Some(mut cur) => {
                cur.merge_best(&r);
                repo.save(cur);
            }
            None => repo.save(r),
        }
    }
    Ok(n)
}
