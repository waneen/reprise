//! 腐敗防止層：汎用 reprise-skin-core → domain::skin。LR2のタイマー/条件番号の「意味」はここで解釈。
use application::ports::{PortError, SkinLoader};
use domain::{random::LegacyMt, skin::Skin};
use reprise_skin_core::{FileResolver, SkinDecoder, SkinSettings};
use std::path::{Path, PathBuf};

/// ワイルドカード画像のランダム選択をLR2互換乱数で行う
struct LegacyFiles<'a>(&'a mut LegacyMt);
impl FileResolver for LegacyFiles<'_> {
    fn resolve(&mut self, base: &Path, pattern: &str) -> Option<PathBuf> {
        let _ = &mut self.0; // TODO: glob → 候補数nで get_rand(n-1)
        Some(base.join(pattern))
    }
}

pub struct DecoderSkinLoader<D: SkinDecoder> {
    pub decoder: D,
    pub settings: SkinSettings,
}

impl<D: SkinDecoder> SkinLoader for DecoderSkinLoader<D> {
    fn can_load(&self, p: &Path) -> bool {
        self.decoder.can_decode(p)
    }
    fn load(&self, p: &Path, rng: &mut LegacyMt) -> Result<Skin, PortError> {
        let doc = self
            .decoder
            .decode(p, &self.settings, &mut LegacyFiles(rng))
            .map_err(|e| PortError::Parse(format!("{e:?}")))?;
        let _ = doc; // TODO: map to domain::skin::Skin
        Err(PortError::Unsupported)
    }
}
