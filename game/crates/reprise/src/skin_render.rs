//! LR2 スキンの描くもの（`domain::skin::lr2::Draw`）を Bevy で描く（プレイ画面・選曲画面で共用）。
//! 同じ（画像・合成モード・フィルタ）が続く四角形を 1 つのメッシュにまとめ、合成モードはマテリアルのパイプライン
//! （ブレンド設定とシェーダの切り替え）で表す。システムフォントの文字列は Bevy の Text2d（OS のフォントを探す）で描き、
//! 幅が決まってから LR2 と同じ倍率で置く
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::{OrthographicProjection, Projection, ScalingMode};
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
    RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite::Anchor;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};
use bevy::text::{FontSize, FontSource, FontWeight, TextLayoutInfo};
use domain::skin::lr2::{Draw, Quad, TextRun};
use domain::skin::Blend;
use std::collections::HashMap;

/// 読み込んだスキン画像（幅, 高さ, RGBA）
pub type Pixels = (u32, u32, Vec<u8>);

/// スキンを描くための Bevy 側の持ち物
#[derive(Default)]
pub struct SkinGfx {
    /// スキン画像（読めなかったものは None）。フィルタ無し・有りの 2 つ
    handles: Vec<Option<[Handle<Image>; 2]>>,
    /// （画像, 合成モード, フィルタ）ごとのマテリアル
    materials: HashMap<(u32, u8, bool), Handle<SkinMaterial>>,
    sizes: Vec<Option<(u32, u32)>>,
    /// システムフォントの文字列の実体（使い回す。場面を出ると消えるので入り直したら [`SkinGfx::forget_texts`]）
    texts: Vec<Entity>,
    /// 単色の四角形に使う白 1 画素の画像の番号
    white: u32,
    resolution: (u32, u32),
}

impl SkinGfx {
    /// 画像を Bevy に渡す（最後に単色用の白 1 画素を足す）
    pub fn new(
        pixels: Vec<Option<Pixels>>,
        resolution: (u32, u32),
        images: &mut Assets<Image>,
    ) -> Self {
        let mut pixels = pixels;
        let white = pixels.len() as u32;
        pixels.push(Some((1, 1, vec![255; 4])));
        let sizes = pixels
            .iter()
            .map(|p| p.as_ref().map(|(w, h, _)| (*w, *h)))
            .collect();
        let handles = pixels
            .into_iter()
            .map(|p| {
                let (w, h, data) = p?;
                let mut img = |sampler| {
                    let mut image = Image::new(
                        Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        TextureDimension::D2,
                        data.clone(),
                        TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::default(),
                    );
                    image.sampler = sampler;
                    images.add(image)
                };
                Some([img(ImageSampler::nearest()), img(ImageSampler::linear())])
            })
            .collect();
        Self {
            handles,
            materials: HashMap::new(),
            sizes,
            texts: vec![],
            white,
            resolution,
        }
    }
    /// 画像の大きさ
    pub fn size(&self, i: u32) -> Option<(u32, u32)> {
        self.sizes.get(i as usize).copied().flatten()
    }
    /// 文字列の実体を忘れる（場面を出て消えたとき）
    pub fn forget_texts(&mut self) {
        self.texts.clear();
    }
    /// 文字列の実体を消す
    pub fn despawn_texts(&mut self, commands: &mut Commands) {
        for e in self.texts.drain(..) {
            commands.entity(e).despawn();
        }
    }
}

/// スキンの解像度を窓いっぱいに映すカメラ（縦横比は保つ）
pub fn camera(resolution: (u32, u32)) -> (Camera2d, Projection) {
    let (w, h) = resolution;
    (
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: w as f32,
                min_height: h as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
    )
}

/// スキンで描いた四角形のまとまり（毎フレーム作り直す）
#[derive(Component)]
pub struct SkinSprite;

/// システムフォントの文字列（描く位置と大きさは文字列の幅が決まってから [`place_text`] で決める）。
/// Bevy は見えている文字列しか組まないので、実体は使い回して見えたままにする
#[derive(Component)]
pub struct SkinText {
    run: Option<TextRun>,
    z: f32,
    res: (u32, u32),
}

/// スキンを描くのに使う Bevy の資源
#[derive(SystemParam)]
pub struct SkinDraw<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SkinMaterial>>,
    old: Query<'w, 's, (Entity, &'static Mesh2d), With<SkinSprite>>,
    texts: Query<
        'w,
        's,
        (
            &'static mut Text2d,
            &'static mut TextFont,
            &'static mut TextColor,
            &'static mut SkinText,
        ),
    >,
}

/// 前のフレームの四角形を消す
pub fn clear_sprites(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    old: &Query<(Entity, &Mesh2d), With<SkinSprite>>,
) {
    for (e, m) in old {
        meshes.remove(&m.0);
        commands.entity(e).despawn();
    }
}

/// 描くもの（奥から順）を描く。奥から順に z を増やし、続く四角形はまとめ、文字列はその間に挟む。
/// 実体は `scene` を出ると消える
pub fn render<S: States + Copy>(p: &mut SkinDraw, gfx: &mut SkinGfx, draws: Vec<Draw>, scene: S) {
    clear_sprites(&mut p.commands, &mut p.meshes, &p.old);
    let mut z = 0.0;
    let mut quads: Vec<Quad> = vec![];
    let mut runs: Vec<(TextRun, f32)> = vec![];
    let white = gfx.white;
    for d in draws {
        match d {
            Draw::Quad(q) => quads.push(q),
            Draw::Fill(f) => quads.push(Quad {
                image: white,
                src: [0.0, 0.0, 1.0, 1.0],
                dst: f.dst,
                argb: f.argb,
                angle: 0.0,
                center: 0,
                blend: f.blend,
                filter: false,
            }),
            Draw::Text(run) => {
                flush(p, gfx, &mut quads, &mut z, scene);
                runs.push((run, z));
                z += 0.001;
            }
        }
    }
    flush(p, gfx, &mut quads, &mut z, scene);
    texts(p, gfx, runs, scene);
}

/// たまった四角形をメッシュにする
fn flush<S: States + Copy>(
    p: &mut SkinDraw,
    gfx: &mut SkinGfx,
    quads: &mut Vec<Quad>,
    z: &mut f32,
    scene: S,
) {
    let loaded = |i: u32| {
        gfx.handles
            .get(i as usize)
            .is_some_and(|h| h.is_some())
            .then(|| gfx.size(i))
            .flatten()
    };
    let list = batches(quads, gfx.resolution, &loaded);
    quads.clear();
    for b in list {
        let Some(Some(h)) = gfx.handles.get(b.image as usize) else {
            continue;
        };
        let texture = h[b.filter as usize].clone();
        let materials = &mut p.materials;
        let material = gfx
            .materials
            .entry((b.image, b.blend as u8, b.filter))
            .or_insert_with(|| {
                materials.add(SkinMaterial {
                    texture,
                    blend: b.blend,
                })
            })
            .clone();
        let mesh = p.meshes.add(b.mesh);
        p.commands.spawn((
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, *z),
            SkinSprite,
            DespawnOnExit(scene),
        ));
        *z += 0.001;
    }
}

/// 文字列の実体を使い回す（足りなければ作り、余りは空にする）
fn texts<S: States + Copy>(
    p: &mut SkinDraw,
    gfx: &mut SkinGfx,
    runs: Vec<(TextRun, f32)>,
    scene: S,
) {
    let res = gfx.resolution;
    let style = |r: &TextRun| {
        let [a, red, g, b] = r.argb.map(|c| c as f32 / 255.0);
        (
            TextFont {
                // ブラウザ版は OS のフォントを探せないので Bevy の既定のフォント（Fira Mono。日本語の字は無い）
                font: if cfg!(target_arch = "wasm32") {
                    TextFont::default().font
                } else {
                    FontSource::SansSerif
                },
                font_size: FontSize::Px(r.font.size.max(1) as f32),
                weight: FontWeight((r.font.thickness.clamp(1, 9) * 100) as u16),
                ..default()
            },
            TextColor(Color::srgba(red, g, b, a)),
        )
    };
    for i in 0..runs.len().max(gfx.texts.len()) {
        let run = runs.get(i).cloned();
        match gfx.texts.get(i).and_then(|e| p.texts.get_mut(*e).ok()) {
            Some((mut text, mut tf, mut color, mut st)) => {
                let s = run.as_ref().map_or(String::new(), |(r, _)| r.text.clone());
                if text.0 != s {
                    text.0 = s;
                }
                if let Some((r, z)) = &run {
                    let (f, c) = style(r);
                    if tf.font_size != f.font_size || tf.weight != f.weight {
                        *tf = f;
                    }
                    color.set_if_neq(c);
                    st.z = *z;
                }
                st.run = run.map(|(r, _)| r);
            }
            None => {
                let Some((r, z)) = run else { continue };
                let (f, c) = style(&r);
                let e = p
                    .commands
                    .spawn((
                        Text2d::new(r.text.clone()),
                        f,
                        c,
                        Anchor::TOP_LEFT,
                        // 幅が分かるまでは大きさ 0
                        Transform::from_scale(Vec3::ZERO),
                        NoFrustumCulling,
                        SkinText {
                            run: Some(r),
                            z,
                            res,
                        },
                        DespawnOnExit(scene),
                    ))
                    .id();
                gfx.texts.push(e);
            }
        }
    }
}

/// システムフォントの文字列を LR2 と同じく置く: 縦 = h / 大きさ、横 = 縦 × min(1, w / 幅)、揃え 1 は幅の半分・2 は幅だけ左へ
pub fn place_text(mut q: Query<(&SkinText, &TextLayoutInfo, &mut Transform)>) {
    for (t, info, mut tf) in &mut q {
        let width = info.size.x;
        let Some(run) = t.run.as_ref().filter(|_| width > 0.0) else {
            tf.scale = Vec3::ZERO;
            continue;
        };
        let [x, y, w, h] = run.dst;
        let hl = h / run.font.size.max(1) as f32;
        let wl = if width > w { w / width } else { 1.0 } * hl;
        let x = match run.align {
            1 => x - (width * wl * 0.5).trunc(),
            2 => x - (width * wl).trunc(),
            _ => x,
        };
        *tf = Transform::from_xyz(x - t.res.0 as f32 / 2.0, t.res.1 as f32 / 2.0 - y, t.z)
            .with_scale(Vec3::new(wl, hl, 1.0));
    }
}

pub struct SkinRenderPlugin;

impl Plugin for SkinRenderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "skin.wgsl");
        app.add_plugins(Material2dPlugin::<SkinMaterial>::default());
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(SkinMaterialKey)]
pub struct SkinMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    pub blend: Blend,
}

#[repr(C)]
#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct SkinMaterialKey {
    blend: u8,
}

impl From<&SkinMaterial> for SkinMaterialKey {
    fn from(m: &SkinMaterial) -> Self {
        Self {
            blend: m.blend as u8,
        }
    }
}

const BLENDS: [Blend; 9] = [
    Blend::None,
    Blend::Alpha,
    Blend::Add,
    Blend::Sub,
    Blend::Mul,
    Blend::Sub2,
    Blend::InvDest,
    Blend::InvSrc,
    Blend::MulAlpha,
];

fn component(src: BlendFactor, dst: BlendFactor, op: BlendOperation) -> BlendComponent {
    BlendComponent {
        src_factor: src,
        dst_factor: dst,
        operation: op,
    }
}

/// 合成モード → （シェーダの切り替え, ブレンド設定）。アルファは画面に残さない（画面のアルファは使わない）
fn blend_state(blend: Blend) -> (Option<&'static str>, BlendState) {
    use BlendFactor::*;
    use BlendOperation::*;
    let keep_alpha = component(Zero, One, Add);
    let color = |def, c| {
        (
            def,
            BlendState {
                color: c,
                alpha: keep_alpha,
            },
        )
    };
    match blend {
        Blend::None => color(
            Some("BLEND_NONE"),
            component(SrcAlpha, OneMinusSrcAlpha, Add),
        ),
        Blend::Alpha => color(None, component(SrcAlpha, OneMinusSrcAlpha, Add)),
        Blend::Add => color(None, component(SrcAlpha, One, Add)),
        // 画面 − 画像
        Blend::Sub | Blend::Sub2 => color(None, component(SrcAlpha, One, ReverseSubtract)),
        // 画面 × 画像（アルファの分だけ白に寄せる）
        Blend::Mul | Blend::MulAlpha => color(Some("BLEND_MUL"), component(Zero, Src, Add)),
        // 画面の色を反転して重ねる
        Blend::InvDest => color(
            Some("BLEND_PREMUL"),
            component(OneMinusDst, OneMinusSrcAlpha, Add),
        ),
        // 画像の色を反転して重ねる
        Blend::InvSrc => color(
            Some("BLEND_INV_SRC"),
            component(SrcAlpha, OneMinusSrcAlpha, Add),
        ),
    }
}

impl Material2d for SkinMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://reprise/skin.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let blend = BLENDS[key.bind_group_data.blend as usize];
        let (def, state) = blend_state(blend);
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if let Some(d) = def {
                fragment.shader_defs.push(d.into());
            }
            for t in fragment.targets.iter_mut().flatten() {
                t.blend = Some(state);
            }
        }
        Ok(())
    }
}

/// 回転の中心（テンキー配置）→ 四角形の中での位置（左上 0、右下 1）
fn pivot(center: u8) -> (f32, f32) {
    match center {
        1 => (0.0, 1.0),
        2 => (0.5, 1.0),
        3 => (1.0, 1.0),
        4 => (0.0, 0.5),
        6 => (1.0, 0.5),
        7 => (0.0, 0.0),
        8 => (0.5, 0.0),
        9 => (1.0, 0.0),
        _ => (0.5, 0.5),
    }
}

/// 続けて描ける四角形のまとまり
pub struct Batch {
    pub image: u32,
    pub blend: Blend,
    pub filter: bool,
    pub mesh: Mesh,
}

/// 四角形をまとまりに分けてメッシュにする。スキン座標（左上原点・y 下向き）→ 画面（中央原点・y 上向き）。
/// `size` は画像の大きさ（読めなかった画像の四角形は飛ばす）
pub fn batches(
    quads: &[Quad],
    res: (u32, u32),
    size: &dyn Fn(u32) -> Option<(u32, u32)>,
) -> Vec<Batch> {
    struct Acc {
        image: u32,
        blend: Blend,
        filter: bool,
        pos: Vec<[f32; 3]>,
        uv: Vec<[f32; 2]>,
        color: Vec<[f32; 4]>,
    }
    let finish = |a: Acc| {
        let n = a.pos.len() as u32 / 4;
        let idx = (0..n)
            .flat_map(|i| [0, 1, 2, 0, 2, 3].map(|k| i * 4 + k))
            .collect();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, a.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, a.uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, a.color);
        mesh.insert_indices(Indices::U32(idx));
        Batch {
            image: a.image,
            blend: a.blend,
            filter: a.filter,
            mesh,
        }
    };
    let mut out = vec![];
    let mut cur: Option<Acc> = None;
    for q in quads {
        let Some((iw, ih)) = size(q.image) else {
            continue;
        };
        let same = cur
            .as_ref()
            .is_some_and(|a| (a.image, a.blend, a.filter) == (q.image, q.blend, q.filter));
        if !same {
            if let Some(a) = cur.take() {
                out.push(finish(a));
            }
            cur = Some(Acc {
                image: q.image,
                blend: q.blend,
                filter: q.filter,
                pos: vec![],
                uv: vec![],
                color: vec![],
            });
        }
        let a = cur.as_mut().unwrap();
        let [x, y, w, h] = q.dst;
        let [sx, sy, sw, sh] = q.src;
        let (fx, fy) = pivot(q.center);
        let (cx, cy) = (x + fx * w, y + fy * h);
        let (s, c) = q.angle.to_radians().sin_cos();
        // 左上・右上・右下・左下
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            let (dx, dy) = ((u - fx) * w, (v - fy) * h);
            // スキン座標（y 下向き）で時計回り
            let (px, py) = (cx + dx * c - dy * s, cy + dx * s + dy * c);
            a.pos
                .push([px - res.0 as f32 / 2.0, res.1 as f32 / 2.0 - py, 0.0]);
            a.uv.push([(sx + u * sw) / iw as f32, (sy + v * sh) / ih as f32]);
        }
        let [al, r, g, b] = q.argb.map(|c| c as f32 / 255.0);
        let lin = Color::srgba(r, g, b, al).to_linear().to_f32_array();
        a.color.extend([lin; 4]);
    }
    if let Some(a) = cur {
        out.push(finish(a));
    }
    out
}
