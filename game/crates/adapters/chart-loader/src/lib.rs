//! 腐敗防止層：汎用 reprise-chart-core のモデル → ゲームの domain::Chart。
//! どの記法かは知らない（ChartDecoder を注入されるだけ）。
use application::ports::{ChartLoader, PortError};
use domain::{chart::*, random::LegacyMt};
use reprise_chart_core::{ChartDecoder, RandomSource};
use std::path::Path;

/// domainの乱数を汎用crateのtraitに適合させる
struct LegacyRng<'a>(&'a mut LegacyMt);
impl RandomSource for LegacyRng<'_> {
    fn next_inclusive(&mut self, max: u32) -> u32 {
        self.0.get_rand(max)
    }
}

pub struct DecoderLoader<D: ChartDecoder>(pub D);

impl<D: ChartDecoder> ChartLoader for DecoderLoader<D> {
    fn can_load(&self, p: &Path) -> bool {
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        ext.is_some_and(|e| self.0.extensions().contains(&e.as_str()))
    }
    fn load(&self, _p: &Path, bytes: &[u8], rng: &mut LegacyMt) -> Result<Chart, PortError> {
        let (doc, _diag) = self
            .0
            .decode(bytes)
            .map_err(|e| PortError::Parse(format!("{e:?}")))?;
        let resolved = doc.resolve(&mut LegacyRng(rng));
        Ok(map_chart(resolved, hash(bytes)))
    }
}

fn hash(_bytes: &[u8]) -> ChartHash {
    ChartHash {
        md5: [0; 16],
        sha256: None,
    }
} // TODO: md-5 / sha2

fn map_chart(r: reprise_chart_core::ResolvedChart, hash: ChartHash) -> Chart {
    let key_mode = match r.layout {
        reprise_chart_core::KeyLayout::Beat5 => KeyMode::K5,
        reprise_chart_core::KeyLayout::Beat10 => KeyMode::K10,
        reprise_chart_core::KeyLayout::Beat14 => KeyMode::K14,
        reprise_chart_core::KeyLayout::Popn9 => KeyMode::K9,
        _ => KeyMode::K7,
    };
    let meta = ChartMeta {
        title: r.meta.title,
        artist: r.meta.artist,
        level: r.meta.level,
        total: r.meta.total,
        judge_rank: r.meta.judge_rank,
    };
    // TODO: Position(小節) → Beat 変換は MeasureLength を見て行う
    Chart {
        hash,
        meta,
        key_mode,
        notes: vec![],
        bgm: vec![],
        timing: vec![],
    }
}
