//! Original author-owned form materials. A resource pack may override each PNG.
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
pub(super) fn images(root: &Path) -> Result<Vec<PackedImage>> {
    FRAMES
        .iter()
        .map(|(name, bytes)| {
            let path = root.join(format!(
                "assets/arcana_four_elements/textures/entity/{name}.png"
            ));
            let (w, h, pixels) = if path.is_file() {
                decode(&path)?
            } else {
                decode_bytes(bytes)?
            };
            Ok((format!("arcana_four_elements:entity/{name}"), w, h, pixels))
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
        assert_eq!(first.len(), 9);
        for (name, w, h, pixels) in first {
            assert!(name.starts_with("arcana_four_elements:entity/"));
            assert_eq!((w, h), (64, 64));
            assert_eq!(pixels.len(), 64 * 64 * 4);
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
        }
    }
}
