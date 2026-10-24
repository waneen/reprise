//! Composition Root + プレゼンテーション層（Bevyはここだけ）。
//! 記法を増やす = chart-formats/ か skin-formats/ にcrateを足して、ここで1行登録するだけ。
use application::ports::{ChartLoader, ScoreImporter, SkinLoader};
use chart_loader::DecoderLoader;
use skin_loader::DecoderSkinLoader;

fn main() {
    let charts: Vec<Box<dyn ChartLoader>> = vec![
        Box::new(DecoderLoader(reprise_chart_bms::BmsDecoder::default())),
        Box::new(DecoderLoader(reprise_chart_bmson::BmsonDecoder)),
    ];
    let skins: Vec<Box<dyn SkinLoader>> = vec![
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_lr2csv::Lr2CsvDecoder,
            settings: Default::default(),
        }),
        Box::new(DecoderSkinLoader {
            decoder: reprise_skin_beatoraja::BeatorajaJsonDecoder,
            settings: Default::default(),
        }),
    ];
    let importers: Vec<Box<dyn ScoreImporter>> = vec![
        Box::new(score_import_lr2::Lr2ScoreDb),
        Box::new(score_import_beatoraja::BeatorajaScoreDb),
    ];
    println!(
        "chart loaders: {}, skin loaders: {}, importers: {}",
        charts.len(),
        skins.len(),
        importers
            .iter()
            .map(|i| i.source_name())
            .collect::<Vec<_>>()
            .join("/")
    );
}
