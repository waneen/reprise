//! LR2 スキンの四角形を Bevy で描く。同じ（画像・合成モード・フィルタ）が続く四角形を 1 つのメッシュにまとめ、
//! 合成モードはマテリアルのパイプライン（ブレンド設定とシェーダの切り替え）で表す
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
    SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};
use domain::skin::lr2::Quad;
use domain::skin::Blend;

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
