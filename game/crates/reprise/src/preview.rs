//! 選曲のプレビュー音（LR2 と同じく、選んでいる曲の譜面を頭からオートプレイで鳴らす。`#PREVIEW` は使わない）。
//! いつ鳴らし始めて止めるかは `domain::select::Preview`。譜面と音は別のタスクで読み込み（LR2 も別スレッド）、
//! 読み終えた時刻を譜面の頭にして、毎フレーム `PlaySession` をオートプレイで進めて BGM とキー音を鳴らす。
//! 鳴らし終えても繰り返さない（LR2 も同じ）
use crate::{load, Boot, Scene, SettingsRes, Sounds};
use application::play::{KeyInput, PlaySession};
use application::ports::AudioOutput;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use domain::chart::{Chart, SoundId};
use domain::play::GaugeType;
use domain::select::{Preview, PreviewAction};
use std::collections::HashMap;
use std::path::PathBuf;

/// 鳴らしている音（止めるときに消す）
#[derive(Component)]
pub struct PreviewSound;

/// 鳴らしている譜面
struct Playing {
    session: PlaySession,
    sounds: HashMap<SoundId, Handle<AudioSource>>,
    start: Instant,
}

#[derive(Resource, Default)]
pub struct PreviewPlayer {
    state: Preview,
    loading: Option<Task<Result<(Chart, Sounds), String>>>,
    playing: Option<Playing>,
}

impl PreviewPlayer {
    fn stop(&mut self, commands: &mut Commands, sounds: &Query<Entity, With<PreviewSound>>) {
        self.loading = None;
        self.playing = None;
        for e in sounds {
            commands.entity(e).despawn();
        }
    }
}

struct Out<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    sounds: &'a HashMap<SoundId, Handle<AudioSource>>,
}

impl AudioOutput for Out<'_, '_, '_> {
    fn play(&mut self, sound: SoundId, _at_us: i64) {
        if let Some(h) = self.sounds.get(&sound) {
            self.commands.spawn((
                AudioPlayer::new(h.clone()),
                PlaybackSettings::DESPAWN,
                PreviewSound,
                DespawnOnExit(Scene::Select),
            ));
        }
    }
}

/// 1 フレーム。`bar_timer` は選んでいるバーを替えてからの時間（タイマー 11）、`current` は選んでいる曲（添字・MD5・パス）
#[allow(clippy::too_many_arguments)]
pub fn update(
    commands: &mut Commands,
    player: &mut PreviewPlayer,
    boot: &Boot,
    settings: &SettingsRes,
    audio: &mut Assets<AudioSource>,
    sounds: &Query<Entity, With<PreviewSound>>,
    bar_timer: Option<i64>,
    current: Option<(usize, domain::chart::ChartHash, PathBuf)>,
) {
    if !settings.saved.preview {
        return;
    }
    let action = player
        .state
        .update(bar_timer, current.as_ref().map(|(i, h, _)| (*i, *h)));
    match action {
        PreviewAction::Start(_) => {
            player.stop(commands, sounds);
            if let Some((_, _, path)) = current {
                let fs = boot.fs.clone();
                let seed = rand_seed();
                player.loading =
                    Some(AsyncComputeTaskPool::get().spawn(async move { load(&*fs, &path, seed) }));
            }
        }
        PreviewAction::Stop => player.stop(commands, sounds),
        PreviewAction::None => {}
    }
    if let Some(task) = player.loading.as_mut() {
        if let Some(done) = check_ready(task) {
            player.loading = None;
            match done {
                Ok((chart, bytes)) => {
                    let handles = bytes
                        .into_iter()
                        .map(|(id, bytes)| (id, audio.add(AudioSource { bytes })))
                        .collect();
                    if let Some(mut session) =
                        PlaySession::new(&chart, GaugeType::Groove, KeyInput::new([]))
                    {
                        session.set_autoplay(0);
                        player.playing = Some(Playing {
                            session,
                            sounds: handles,
                            start: Instant::now(),
                        });
                    }
                }
                Err(e) => eprintln!("プレビュー音: {e}"),
            }
        }
    }
    if let Some(p) = player.playing.as_mut() {
        let t = p.start.elapsed().as_millis() as i64;
        let mut out = Out {
            commands,
            sounds: &p.sounds,
        };
        p.session.frame(t, &[], &mut out);
    }
}

/// 選曲を出るときは止めて、次に入ったときはまた 500ms 待つ
pub fn exit(
    mut commands: Commands,
    mut player: ResMut<PreviewPlayer>,
    sounds: Query<Entity, With<PreviewSound>>,
) {
    player.stop(&mut commands, &sounds);
    player.state = Preview::default();
}

fn rand_seed() -> u32 {
    crate::platform::seed()
}
