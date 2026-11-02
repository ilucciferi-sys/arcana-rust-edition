//! Original author-owned form/effect materials, overridable by resource packs.
use super::*;
const FRAMES: [(&str, &[u8]); 9] = [
    (
        "body_fire_0",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_0.png"
        ),
    ),
    (
        "body_fire_1",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_1.png"
        ),
    ),
    (
        "body_fire_2",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_2.png"
        ),
    ),
    (
        "body_fire_3",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_3.png"
        ),
    ),
    (
        "body_fire_4",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_4.png"
        ),
    ),
    (
        "body_fire_5",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_5.png"
        ),
    ),
    (
        "body_fire_6",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_6.png"
        ),
    ),
    (
        "body_fire_7",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_7.png"
        ),
    ),
    (
        "body_fire_glow",
        include_bytes!(
            "../../../../arcana/assets/arcana_four_elements/textures/entity/body_fire_glow.png"
        ),
    ),
];
macro_rules! effect {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(
                "../../../../arcana/assets/arcana_four_elements/textures/effect/",
                $name,
                ".png"
            ))
            .as_slice(),
        )
    };
}
const EFFECTS: [(&str, &[u8]); 10] = [
    effect!("sigil_fire"),
    effect!("halo"),
    effect!("flame_0"),
    effect!("flame_1"),
    effect!("flame_2"),
    effect!("flame_3"),
    effect!("flame_4"),
    effect!("flame_5"),
    effect!("flame_6"),
    effect!("flame_7"),
];
pub(super) fn images(root: &Path) -> Result<Vec<PackedImage>> {
    FRAMES
        .iter()
        .map(|(name, bytes)| ("entity", name, bytes))
        .chain(EFFECTS.iter().map(|(name, bytes)| ("effect", name, bytes)))
        .map(|(folder, name, bytes)| {
            let path = root.join(format!(
                "assets/arcana_four_elements/textures/{folder}/{name}.png"
            ));
            let (w, h, pixels) = if path.is_file() {
                decode(&path)?
            } else {
                decode_bytes(bytes)?
            };
            Ok((
                format!("arcana_four_elements:{folder}/{name}"),
                w,
                h,
                pixels,
            ))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_form_materials_decode_and_survive_atlas_rebuild() {
        let root = std::env::temp_dir().join("arcana-form-no-pack");
        let first = images(&root).unwrap();
        let second = images(&root).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 19);
        for (name, w, h, pixels) in first {
            assert!(name.starts_with("arcana_four_elements:"));
            let expected = if name.contains(":entity/") {
                64
            } else if name.ends_with("sigil_fire") {
                512
            } else if name.ends_with("halo") {
                128
            } else {
                256
            };
            assert_eq!((w, h), (expected, expected));
            assert_eq!(pixels.len(), (w * h * 4) as usize);
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
        }
    }
}
