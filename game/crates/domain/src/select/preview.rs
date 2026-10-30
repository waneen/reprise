//! 選曲のプレビュー音とパネル 1 の開け閉め（OpenLR2 `Scene02_Songselect.cpp` の `ProcS_Select`・`ThreadProc_LoadPreview`・
//! `ProcI_Select`。書き起こしの元の `LR2f.cpp` の `LoadPreview` でも同じ）
use crate::chart::ChartHash;

/// 選んでから鳴らし始めるまで（ms。タイマー 11 の経過）
pub const PREVIEW_DELAY_MS: i64 = 500;

/// プレビュー音の状態（LR2 の `previewStatus` と `previewBMShash`）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preview {
    /// 読み込み中か鳴らしている（LR2 の 1・2。鳴らし終えても戻らない）
    active: bool,
    hash: Option<ChartHash>,
}

/// このフレームですること
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewAction {
    None,
    /// この曲（`Library::songs` の添字）を譜面の頭からオートプレイで鳴らし始める
    Start(usize),
    /// 鳴らしている音を止める
    Stop,
}

impl Preview {
    /// DOM-SEL-18: `since_ms` は選んでいるバーを替えてからの時間（タイマー 11。止まっていれば None）、`current` は選んでいる曲。
    /// 鳴らしていなければ、曲を選んで 500ms を過ぎたら始める。鳴らしている間（読み込み中・鳴らし終えた後も）に選んでいるバーの MD5 が
    /// 替われば（曲でないバーでも）止める。止めた後はまた 500ms を待つ
    pub fn update(
        &mut self,
        since_ms: Option<i64>,
        current: Option<(usize, ChartHash)>,
    ) -> PreviewAction {
        let hash = current.map(|(_, h)| h);
        if !self.active {
            if let (Some(t), Some((i, h))) = (since_ms, current) {
                if t >= PREVIEW_DELAY_MS {
                    self.active = true;
                    self.hash = Some(h);
                    return PreviewAction::Start(i);
                }
            }
            return PreviewAction::None;
        }
        if self.hash != hash {
            self.active = false;
            self.hash = hash;
            return PreviewAction::Stop;
        }
        PreviewAction::None
    }
}

/// パネル 1（START で開く。LR2 の `panel`）の開け閉め
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Panel {
    open: bool,
}

/// パネルが開いた・閉じた
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelChange {
    Opened,
    Closed,
}

impl Panel {
    pub fn is_open(&self) -> bool {
        self.open
    }
    /// DOM-SEL-19: START を押したら開く（開いていれば閉じる）。開いている間に START を離したら閉じる
    pub fn update(&mut self, start_pressed: bool, start_released: bool) -> Option<PanelChange> {
        if start_pressed {
            self.open = !self.open;
            return Some(if self.open {
                PanelChange::Opened
            } else {
                PanelChange::Closed
            });
        }
        if self.open && start_released {
            self.open = false;
            return Some(PanelChange::Closed);
        }
        None
    }
}
