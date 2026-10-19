//! glTF models for `s:model`: the default scene's meshes flattened to
//! world-space triangles with their normals, texture coordinates, and PBR
//! materials, plus the images those materials use.

use std::{collections::HashMap, sync::Arc};

use crate::scene::{Material, TextureData, V3};

/// A parsed model, ready to place into a scene.
#[derive(Debug)]
pub struct Model {
    pub(crate) triangles: Vec<Triangle>,
    pub(crate) materials: Vec<ModelMaterial>,
    /// Encoded images (decoded, size-checked, only when drawn), indexed by
    /// `ModelMaterial::texture`.
    pub(crate) images: Vec<TextureData>,
    pub(crate) bounds: (V3, V3),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Triangle {
    pub(crate) positions: [V3; 3],
    pub(crate) normals: Option<[V3; 3]>,
    pub(crate) uvs: Option<[[f32; 2]; 3]>,
    pub(crate) material: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ModelMaterial {
    /// sRGB `0xRRGGBB` and alpha.
    pub(crate) rgb: u32,
    pub(crate) alpha: f32,
    pub(crate) texture: Option<usize>,
    pub(crate) material: Material,
    pub(crate) double_sided: bool,
}

/// Most triangles one model may bring.
const MAX_TRIANGLES: usize = 400_000;
/// Most node visits while walking the scene (instancing can repeat nodes;
/// a malformed file can loop).
const MAX_NODE_VISITS: usize = 100_000;
const MAX_MODEL_ENTRIES: usize = 128;
const MAX_IMPORTED_BYTES: usize = 16 * 1024 * 1024;
/// Decoded attribute/index vectors one primitive visit holds at once: a
/// model at the triangle limit with normals and UVs, without index sharing,
/// needs about 48 MB.
const MAX_EXPANDED_BYTES: usize = 64 * 1024 * 1024;
/// The same, summed over every primitive and instance visit (each re-reads
/// its accessors): bounds the total decoding work while primitives that share
/// one large vertex accessor stay usable.
const MAX_EXPANDED_WORK: usize = 256 * 1024 * 1024;
const UNREADABLE_INDICES: &str = "the model has an index accessor its buffer view can't hold";

fn charge(total: &mut usize, bytes: usize, kind: &str) -> Result<(), String> {
    *total = total
        .checked_add(bytes)
        .ok_or_else(|| format!("model {kind} byte budget exceeded"))?;
    if *total > MAX_IMPORTED_BYTES {
        return Err(format!("model {kind} byte budget exceeded"));
    }
    Ok(())
}

/// An upper bound before base64 decoding, including malformed input. Never
/// trust a buffer's declared length instead of its actual encoded payload.
fn data_uri_bytes(uri: &str) -> Result<usize, String> {
    let data = uri
        .strip_prefix("data:")
        .and_then(|s| s.split_once(";base64,"))
        .map(|(_, data)| data)
        .ok_or("models require self-contained base64 data URIs")?;
    data.len()
        .checked_add(3)
        .and_then(|n| (n / 4).checked_mul(3))
        .ok_or_else(|| "model data URI byte budget exceeded".into())
}

fn preflight(document: &gltf::Document, blob: Option<&[u8]>) -> Result<(), String> {
    if document.buffers().len() > MAX_MODEL_ENTRIES {
        return Err("model buffer count exceeds 128".into());
    }
    if document.images().len() > MAX_MODEL_ENTRIES {
        return Err("model image count exceeds 128".into());
    }
    let mut buffers = 0;
    for buffer in document.buffers() {
        let actual = match buffer.source() {
            gltf::buffer::Source::Bin => blob.map_or(0, <[u8]>::len),
            gltf::buffer::Source::Uri(uri) => data_uri_bytes(uri)?,
        };
        let padded = actual
            .max(buffer.length())
            .checked_add(3)
            .map(|n| n & !3)
            .ok_or("model buffer byte budget exceeded")?;
        charge(&mut buffers, padded, "buffer")?;
    }
    let mut images = 0;
    for image in document.images() {
        let length = match image.source() {
            gltf::image::Source::View { view, .. } => view.length(),
            gltf::image::Source::Uri { uri, .. } => data_uri_bytes(uri)?,
        };
        // Charge logical references even when identical view bytes are shared.
        charge(&mut images, length, "image")?;
    }
    Ok(())
}

fn to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn rgb_of(linear: [f32; 3]) -> u32 {
    let channel = |c: f32| (to_srgb(c) * 255.0).round() as u32;
    channel(linear[0]) << 16 | channel(linear[1]) << 8 | channel(linear[2])
}

type Mat4 = [[f32; 4]; 4];

fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    // Column-major, as glTF stores them: (a * b)[col][row].
    let mut out = [[0.0; 4]; 4];
    for (col, out_col) in out.iter_mut().enumerate() {
        for (row, value) in out_col.iter_mut().enumerate() {
            *value = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    out
}

fn transform_point(m: &Mat4, p: [f32; 3]) -> V3 {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
    ]
}

/// Normals transform by the matrix's linear part; uniform scales are by far
/// the common case, and the shading normal is renormalized anyway.
fn transform_normal(m: &Mat4, n: [f32; 3]) -> V3 {
    let v = [
        m[0][0] * n[0] + m[1][0] * n[1] + m[2][0] * n[2],
        m[0][1] * n[0] + m[1][1] * n[1] + m[2][1] * n[2],
        m[0][2] * n[0] + m[1][2] * n[1] + m[2][2] * n[2],
    ];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Visit the scene's usable triangle primitives with their world matrix and
/// whether their normals/texture coordinates have a readable shape.
fn walk(
    scene: &gltf::Scene,
    identity: Mat4,
    mut visit: impl FnMut(&gltf::mesh::Primitive, &Mat4, (bool, bool)) -> Result<(), String>,
) -> Result<(), String> {
    let mut stack: Vec<(gltf::Node, Mat4)> = scene.nodes().map(|node| (node, identity)).collect();
    let mut visits = 0;
    while let Some((node, parent)) = stack.pop() {
        visits += 1;
        if visits > MAX_NODE_VISITS {
            return Err("the model's node tree is too large (or loops)".into());
        }
        let world = multiply(&parent, &node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                // Only read attributes of the shape their meaning needs.
                use gltf::accessor::{DataType, Dimensions};
                use gltf::mesh::Semantic;
                let shaped = |semantic: Semantic, dimensions: Dimensions, types: &[DataType]| {
                    primitive.get(&semantic).is_some_and(|a| {
                        a.dimensions() == dimensions && types.contains(&a.data_type())
                    })
                };
                if !shaped(Semantic::Positions, Dimensions::Vec3, &[DataType::F32])
                    || primitive.indices().is_some_and(|a| {
                        a.dimensions() != Dimensions::Scalar
                            || ![DataType::U8, DataType::U16, DataType::U32]
                                .contains(&a.data_type())
                    })
                {
                    continue;
                }
                let normals_ok = shaped(Semantic::Normals, Dimensions::Vec3, &[DataType::F32]);
                let uvs_ok = shaped(
                    Semantic::TexCoords(0),
                    Dimensions::Vec2,
                    &[DataType::F32, DataType::U8, DataType::U16],
                );
                visit(&primitive, &world, (normals_ok, uvs_ok))?;
            }
        }
        for child in node.children() {
            if stack.len() >= MAX_NODE_VISITS {
                return Err("the model's node tree is too large (or loops)".into());
            }
            stack.push((child, world));
        }
    }
    Ok(())
}

/// Charge the vectors `parse` collects for one primitive: positions, the
/// readable normals/texture coordinates, and explicit or implicit indices.
fn charge_expanded(
    total: &mut usize,
    primitive: &gltf::mesh::Primitive,
    (normals_ok, uvs_ok): (bool, bool),
) -> Result<(), String> {
    use gltf::mesh::Semantic;
    let exceeded = || "model expanded accessor byte budget exceeded".to_string();
    let count = |semantic: Semantic| primitive.get(&semantic).map_or(0, |a| a.count());
    let positions = count(Semantic::Positions);
    let indices = primitive.indices().map_or(positions, |a| a.count());
    let mut bytes = [
        (positions, size_of::<V3>()),
        (
            if normals_ok {
                count(Semantic::Normals)
            } else {
                0
            },
            size_of::<V3>(),
        ),
        (
            if uvs_ok {
                count(Semantic::TexCoords(0))
            } else {
                0
            },
            size_of::<[f32; 2]>(),
        ),
        (indices, size_of::<usize>()),
    ]
    .into_iter()
    .try_fold(0usize, |sum, (n, size)| {
        n.checked_mul(size).and_then(|b| sum.checked_add(b))
    })
    .ok_or_else(exceeded)?;
    if bytes > MAX_EXPANDED_BYTES {
        return Err(exceeded());
    }
    bytes = total.checked_add(bytes).ok_or_else(exceeded)?;
    if bytes > MAX_EXPANDED_WORK {
        return Err(exceeded());
    }
    *total = bytes;
    Ok(())
}

impl Model {
    /// Parse a `.glb` or self-contained `.gltf` (buffers and images embedded
    /// or as data URIs; no external files).
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        // The glTF reader asserts on some malformed files rather than
        // erroring; a model is untrusted, so that's an error here too.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::parse_unguarded(bytes)
        }))
        .unwrap_or_else(|_| Err("not a usable glTF model: malformed accessors".into()))
    }

    fn parse_unguarded(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > crate::assets::MAX_ASSET_BYTES {
            return Err("model input byte budget exceeded".into());
        }
        let unusable = |e: gltf::Error| format!("not a usable glTF model: {e}");
        let gltf = gltf::Gltf::from_slice(bytes).map_err(unusable)?;
        let document = gltf.document;
        preflight(&document, gltf.blob.as_deref())?;
        // No base path: external files are refused, only embedded data loads.
        let buffers = gltf::import_buffers(&document, None, gltf.blob).map_err(unusable)?;
        // Images stay encoded: the renderer decodes them with the same size
        // limits as image assets (a small file can declare a huge image).
        let mut views: HashMap<(usize, usize, usize), Arc<[u8]>> = HashMap::new();
        let images = document
            .images()
            .map(|image| -> Result<TextureData, String> {
                let bytes: Arc<[u8]> = match image.source() {
                    gltf::image::Source::View { view, .. } => {
                        let key = (view.buffer().index(), view.offset(), view.length());
                        if let Some(bytes) = views.get(&key) {
                            bytes.clone()
                        } else {
                            let end = view
                                .offset()
                                .checked_add(view.length())
                                .ok_or("invalid image buffer view")?;
                            let data = buffers
                                .get(view.buffer().index())
                                .and_then(|data| data.0.get(view.offset()..end))
                                .ok_or("invalid image buffer view")?;
                            let bytes: Arc<[u8]> = data.into();
                            views.insert(key, bytes.clone());
                            bytes
                        }
                    }
                    gltf::image::Source::Uri { uri, .. } => {
                        use base64::Engine as _;
                        let (_, data) = uri
                            .strip_prefix("data:")
                            .and_then(|s| s.split_once(";base64,"))
                            .ok_or("models require self-contained base64 data URIs")?;
                        base64::engine::general_purpose::STANDARD
                            .decode(data)
                            .map_err(|_| "invalid model image data URI")?
                            .into()
                    }
                };
                Ok(TextureData::Encoded(bytes))
            })
            .collect::<Result<_, _>>()?;
        let materials: Vec<ModelMaterial> = document
            .materials()
            .map(|m| {
                let pbr = m.pbr_metallic_roughness();
                let base = pbr.base_color_factor();
                let emissive = m.emissive_factor();
                let strength = m.emissive_strength().unwrap_or(1.0);
                let glow = emissive.iter().copied().fold(0.0f32, f32::max) * strength;
                let mut rgb = rgb_of([base[0], base[1], base[2]]);
                // An emitter whose base is dark shines its emissive color.
                if glow > 0.0 && base[0].max(base[1]).max(base[2]) < 0.05 {
                    rgb = rgb_of(emissive);
                }
                let glass = m
                    .transmission()
                    .map(|t| t.transmission_factor())
                    .unwrap_or(0.0);
                // OPAQUE ignores alpha, and MASK cuts rather than blends:
                // only BLEND materials are see-through.
                let alpha = match m.alpha_mode() {
                    gltf::material::AlphaMode::Blend => base[3],
                    gltf::material::AlphaMode::Opaque | gltf::material::AlphaMode::Mask => 1.0,
                };
                ModelMaterial {
                    rgb,
                    alpha,
                    texture: pbr
                        .base_color_texture()
                        .map(|info| info.texture().source().index()),
                    material: Material {
                        roughness: pbr.roughness_factor().clamp(0.02, 1.0),
                        metalness: pbr.metallic_factor().clamp(0.0, 1.0),
                        glow: (glow * 2.0).min(8.0),
                        glass,
                        ior: m.ior().unwrap_or(1.5),
                        dispersion: 0.0,
                    },
                    double_sided: m.double_sided(),
                }
            })
            .chain(std::iter::once(ModelMaterial {
                // Primitives without a material.
                rgb: 0xcccccc,
                alpha: 1.0,
                texture: None,
                material: Material::default(),
                double_sided: false,
            }))
            .collect();
        let default_material = materials.len() - 1;

        let mut model = Self {
            triangles: Vec::new(),
            materials,
            images,
            bounds: ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
        };
        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .ok_or("the model has no scene")?;
        let identity: Mat4 = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        // Admit every collected attribute/index vector, cumulatively over all
        // primitives and instances, before reading any: a sparse accessor
        // without a base view expands a tiny file to `count` elements.
        let mut expanded = 0;
        walk(&scene, identity, |primitive, _, shape| {
            // Only a primitive WITHOUT an index accessor draws implicit
            // indices: one whose accessor can't be read (a view too short
            // for its count) would otherwise collect `positions` of them,
            // which the charge below never counted.
            if primitive.indices().is_some()
                && primitive
                    .reader(|b| buffers.get(b.index()).map(|d| &d.0[..]))
                    .read_indices()
                    .is_none()
            {
                return Err(UNREADABLE_INDICES.into());
            }
            charge_expanded(&mut expanded, primitive, shape)
        })?;
        walk(
            &scene,
            identity,
            |primitive, world, (normals_ok, uvs_ok)| {
                let reader = primitive.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
                let Some(positions) = reader.read_positions() else {
                    return Ok(());
                };
                let positions: Vec<V3> = positions.map(|p| transform_point(world, p)).collect();
                // Attributes must cover every vertex; a short one is dropped.
                let normals: Option<Vec<V3>> = normals_ok
                    .then(|| reader.read_normals())
                    .flatten()
                    .map(|n| n.map(|n| transform_normal(world, n)).collect::<Vec<_>>())
                    .filter(|n| n.len() >= positions.len());
                let uvs: Option<Vec<[f32; 2]>> = uvs_ok
                    .then(|| reader.read_tex_coords(0))
                    .flatten()
                    .map(|t| t.into_f32().collect::<Vec<_>>())
                    .filter(|t| t.len() >= positions.len());
                let indices: Vec<usize> = match (primitive.indices(), reader.read_indices()) {
                    (_, Some(indices)) => indices.into_u32().map(|i| i as usize).collect(),
                    (None, None) => (0..positions.len()).collect(),
                    (Some(_), None) => return Err(UNREADABLE_INDICES.into()),
                };
                let material = primitive.material().index().unwrap_or(default_material);
                for tri in indices.chunks_exact(3) {
                    if tri.iter().any(|&i| i >= positions.len()) {
                        continue;
                    }
                    if model.triangles.len() >= MAX_TRIANGLES {
                        return Err(format!("the model has over {MAX_TRIANGLES} triangles"));
                    }
                    let pick = |list: &[V3]| [list[tri[0]], list[tri[1]], list[tri[2]]];
                    let p = pick(&positions);
                    for v in &p {
                        for (axis, value) in v.iter().enumerate() {
                            model.bounds.0[axis] = model.bounds.0[axis].min(*value);
                            model.bounds.1[axis] = model.bounds.1[axis].max(*value);
                        }
                    }
                    model.triangles.push(Triangle {
                        positions: p,
                        normals: normals.as_deref().map(pick),
                        uvs: uvs.as_ref().map(|uv| [uv[tri[0]], uv[tri[1]], uv[tri[2]]]),
                        material,
                    });
                }
                Ok(())
            },
        )?;
        if model.triangles.is_empty() {
            return Err("the model has no triangle meshes".into());
        }
        Ok(model)
    }
}

/// An `Arc`'d model for the scene's cache.
pub(crate) fn parse_shared(bytes: &[u8]) -> Result<Arc<Model>, String> {
    Model::parse(bytes).map(Arc::new)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use base64::Engine as _;

    /// A binary glTF of one red, double-sided triangle under a node that
    /// moves it up by 2 and doubles it.
    pub(crate) fn triangle_glb() -> Vec<u8> {
        let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let bin: Vec<u8> = positions.iter().flat_map(|f| f.to_le_bytes()).collect();
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[0,2,0],"scale":[2,2,2]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],
            "materials":[{{"doubleSided":true,"pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1],"metallicFactor":0.5,"roughnessFactor":0.25}}}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],
            "bufferViews":[{{"buffer":0,"byteLength":{len}}}],
            "buffers":[{{"byteLength":{len}}}]}}"#,
            len = bin.len()
        );
        glb(&json, &bin)
    }

    /// A binary glTF of `json` over `bin`.
    fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut json = json.as_bytes().to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut glb = Vec::with_capacity(total);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\0");
        glb.extend_from_slice(bin);
        glb
    }

    /// One triangle (3 positions, then `extra` bytes) and `rest` of the JSON.
    fn triangle_with(extra: &[u8], rest: &str) -> Vec<u8> {
        let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let mut bin: Vec<u8> = positions.iter().flat_map(|f| f.to_le_bytes()).collect();
        bin.extend_from_slice(extra);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "buffers":[{{"byteLength":{len}}}],{rest}}}"#,
            len = bin.len()
        );
        glb(&json, &bin)
    }

    fn repeated_images(count: usize, size: usize) -> Vec<u8> {
        let images = std::iter::repeat_n(r#"{"bufferView":1,"mimeType":"image/png"}"#, count)
            .collect::<Vec<_>>()
            .join(",");
        triangle_with(
            &vec![0; size],
            &format!(
                r#""nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":{size}}}],
            "images":[{images}]"#
            ),
        )
    }

    // Tiny sparse GLB: no base bufferViews, one sparse item, large logical counts.
    // Each rejected case expands to 72 MB in one visit (> 64 MiB) or 280 MB
    // over seven 40 MB visits (> 256 MiB); the unfixed path stops at the
    // triangle limit after the first. Run under the bounded subprocess limits.
    fn sparse_work(
        positions: usize,
        extra: Option<(&str, usize)>,
        primitives: usize,
        instances: usize,
    ) -> Vec<u8> {
        let sparse = |count, shape: &str, component| {
            format!(
                r#"{{"componentType":{component},"count":{count},"type":"{shape}","min":[0,0,0],"max":[1,1,1],"sparse":{{"count":1,"indices":{{"bufferView":0,"componentType":5125}},"values":{{"bufferView":1}}}}}}"#
            )
        };
        let mut accessors = vec![sparse(positions, "VEC3", 5126)];
        let mut attrs = r#""POSITION":0"#.to_string();
        let mut indices = String::new();
        if let Some((kind, count)) = extra {
            let shape = match kind {
                "NORMAL" => "VEC3",
                "TEXCOORD_0" => "VEC2",
                _ => "SCALAR",
            };
            // min/max apply only to positions (shape-appropriate validation).
            accessors.push(
                sparse(count, shape, if kind == "indices" { 5125 } else { 5126 })
                    .replace(r#","min":[0,0,0],"max":[1,1,1]"#, ""),
            );
            if kind == "indices" {
                indices = r#","indices":1"#.into();
            } else {
                attrs += &format!(r#","{kind}":1"#);
            }
        }
        let primitive = format!(r#"{{"attributes":{{{attrs}}}{indices}}}"#);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[{nodes}]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{prims}]}}],"buffers":[{{"byteLength":16}}],"bufferViews":[{{"buffer":0,"byteLength":4}},{{"buffer":0,"byteOffset":4,"byteLength":12}}],"accessors":[{accessors}]}}"#,
            nodes = vec!["0"; instances].join(","),
            prims = vec![primitive; primitives].join(","),
            accessors = accessors.join(",")
        );
        glb(&json, &[0; 16])
    }

    fn expanded_rejected(
        positions: usize,
        extra: Option<(&str, usize)>,
        primitives: usize,
        instances: usize,
    ) {
        let error =
            Model::parse(&sparse_work(positions, extra, primitives, instances)).unwrap_err();
        assert!(
            error.contains("expanded accessor byte budget"),
            "{positions} {extra:?} {primitives}/{instances}: {error}"
        );
    }
    #[test]
    fn sparse_expanded_positions_and_implicit_indices() {
        expanded_rejected(4_000_000, None, 1, 1);
    }
    #[test]
    fn sparse_expanded_normals() {
        expanded_rejected(3, Some(("NORMAL", 6_000_000)), 1, 1);
    }
    #[test]
    fn sparse_expanded_uvs() {
        expanded_rejected(3, Some(("TEXCOORD_0", 9_000_000)), 1, 1);
    }
    #[test]
    fn sparse_expanded_indices() {
        expanded_rejected(3, Some(("indices", 9_000_000)), 1, 1);
    }
    // 2M positions + implicit indices are 40 MB, admitted alone; seven
    // repeats over primitives or instances cross the 256 MiB work budget.
    #[test]
    fn sparse_expanded_primitives() {
        expanded_rejected(2_000_000, None, 7, 1);
    }
    #[test]
    fn sparse_expanded_instances() {
        expanded_rejected(2_000_000, None, 1, 7);
    }

    #[test]
    fn primitives_sharing_a_large_vertex_accessor_stay_usable() {
        // 20 primitives x 3.6 MB of shared positions: past the per-visit
        // budget in sum only, well within the work budget.
        let model = Model::parse(&sparse_work(300_000, Some(("indices", 3)), 20, 1)).unwrap();
        assert_eq!(model.triangles.len(), 20);
    }

    #[test]
    fn sparse_normal_models_remain_usable() {
        let model = Model::parse(&sparse_work(3, Some(("NORMAL", 3)), 2, 2)).unwrap();
        assert_eq!(model.triangles.len(), 4);
        assert!(model.triangles.iter().all(|t| t.normals.is_some()));
    }

    #[test]
    fn repeated_image_views_share_backing_and_aggregate_images_are_bounded() {
        let model = Model::parse(&repeated_images(4, 1024)).unwrap();
        let (TextureData::Encoded(a), TextureData::Encoded(b)) =
            (&model.images[0], &model.images[1])
        else {
            panic!("encoded images")
        };
        assert!(Arc::ptr_eq(a, b), "repeated image view was copied");
    }

    #[test]
    fn aggregate_image_materialization_is_bounded_before_copy() {
        // Only 512 KiB on input; old implementation copies 32 MiB, not GB.
        let result = Model::parse(&repeated_images(64, 512 * 1024));
        assert!(
            result.is_err(),
            "oversized aggregate image materialization was accepted"
        );
        let error = result.unwrap_err();
        assert!(error.contains("image byte budget"), "{error}");
    }

    #[test]
    fn model_image_and_buffer_counts_are_checked_before_import() {
        let error = Model::parse(&repeated_images(129, 4)).unwrap_err();
        assert!(error.contains("image count"), "{error}");
        let buffers = std::iter::repeat_n(
            r#"{"byteLength":4,"uri":"data:application/octet-stream;base64,AAAAAA=="}"#,
            129,
        )
        .collect::<Vec<_>>()
        .join(",");
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[]}}],"buffers":[{buffers}]}}"#
        );
        let error = Model::parse(json.as_bytes()).unwrap_err();
        assert!(error.contains("buffer count"), "{error}");
    }

    #[test]
    fn embedded_uri_imports_are_bounded_before_decode_and_preserve_images() {
        let binary = repeated_images(1, 4);
        let json_len = u32::from_le_bytes(binary[12..16].try_into().unwrap()) as usize;
        let json = std::str::from_utf8(&binary[20..20 + json_len]).unwrap();
        let model_bytes = &binary[28 + json_len..];
        let uri = base64::engine::general_purpose::STANDARD.encode(model_bytes);
        let json = json.replace(r#""buffers":[{"byteLength":40}]"#, &format!(r#""buffers":[{{"byteLength":40,"uri":"data:application/octet-stream;base64,{uri}"}}]"#))
            .replace(r#"{"bufferView":1,"mimeType":"image/png"}"#, r#"{"uri":"data:image/png;base64,AQIDBA=="}"#);
        let model = Model::parse(json.as_bytes()).unwrap();
        let TextureData::Encoded(bytes) = &model.images[0] else {
            panic!("encoded")
        };
        assert_eq!(bytes.as_ref(), &[1, 2, 3, 4]);
        let error = Model::parse(br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":16777217,"uri":"data:application/octet-stream;base64,AAAAAA=="}]}"#).unwrap_err();
        assert!(error.contains("buffer byte budget"), "{error}");
        for uri in ["outside.bin", "data:image/png;utf8,abcd"] {
            let json = format!(r#"{{"asset":{{"version":"2.0"}},"images":[{{"uri":"{uri}"}}]}}"#);
            assert!(
                Model::parse(json.as_bytes())
                    .unwrap_err()
                    .contains("self-contained base64")
            );
        }
    }

    #[test]
    fn looping_node_trees_are_refused_not_walked_forever() {
        for children in ["[0]", "[0,0,0,0]"] {
            let json = format!(
                r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"children":{children}}}]}}"#
            );
            let error = Model::parse(&glb(&json, &[])).unwrap_err();
            assert!(error.contains("loops"), "{error}");
        }
    }

    #[test]
    fn attributes_shorter_than_the_positions_are_dropped() {
        // POSITION has 3 vertices, NORMAL only 1.
        let normal: Vec<u8> = [0.0f32, 0.0, 1.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let model = Model::parse(&triangle_with(
            &normal,
            r#""nodes":[{"mesh":0}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1}}]}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
                         {"bufferView":1,"componentType":5126,"count":1,"type":"VEC3"}],
            "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":12}]"#,
        ))
        .unwrap();
        assert!(model.triangles[0].normals.is_none());
    }

    #[test]
    fn attributes_of_the_wrong_shape_are_skipped_not_read() {
        // TEXCOORD_0 pointing at the VEC3 positions (the glTF reader asserts).
        let model = Model::parse(&triangle_with(
            &[],
            r#""nodes":[{"mesh":0}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":0}}]}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
            "bufferViews":[{"buffer":0,"byteLength":36}]"#,
        ))
        .unwrap();
        assert!(model.triangles[0].uvs.is_none());
    }

    #[test]
    fn embedded_images_stay_encoded_and_are_size_checked_when_drawn() {
        // A tiny PNG declaring an image wider than any texture may be.
        let mut png = Vec::new();
        ::image::RgbaImage::new(20_000, 1)
            .write_to(
                &mut std::io::Cursor::new(&mut png),
                ::image::ImageFormat::Png,
            )
            .unwrap();
        let rest = format!(
            r#""nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],
            "textures":[{{"source":0}}],
            "images":[{{"bufferView":1,"mimeType":"image/png"}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":{len}}}]"#,
            len = png.len()
        );
        let model = Model::parse(&triangle_with(&png, &rest)).unwrap();
        let TextureData::Encoded(bytes) = &model.images[0] else {
            panic!("decoded eagerly")
        };
        assert_eq!(bytes.len(), png.len());
        let error = crate::image::decode_texture(&model.images[0]).unwrap_err();
        assert!(error.contains("20000x1"), "{error}");
    }

    #[test]
    fn a_glb_parses_into_world_space_triangles_with_materials() {
        let model = Model::parse(&triangle_glb()).unwrap();
        assert_eq!(model.triangles.len(), 1);
        let tri = model.triangles[0];
        assert_eq!(
            tri.positions,
            [[0.0, 2.0, 0.0], [2.0, 2.0, 0.0], [0.0, 4.0, 0.0]]
        );
        assert_eq!(model.bounds, ([0.0, 2.0, 0.0], [2.0, 4.0, 0.0]));
        let material = &model.materials[tri.material];
        assert_eq!(material.rgb, 0xff0000);
        assert!(material.double_sided);
        assert_eq!(material.material.metalness, 0.5);
        assert_eq!(material.material.roughness, 0.25);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(Model::parse(b"not a model").is_err());
        assert!(Model::parse(b"glTF\x02\0\0\0\x0c\0\0\0").is_err());
    }
}
