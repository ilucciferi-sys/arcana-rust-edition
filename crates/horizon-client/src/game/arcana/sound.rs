//! Author-owned Arcana OGGs are registered as a normal sound namespace.
use super::*;
impl Game<'_> {
    pub(super) fn arcana_prepare_audio(&mut self) {
        if self.arcana.school.audio_loaded {
            return;
        }
        self.arcana.school.audio_loaded = true;
        let directory = std::env::var_os("HORIZON_ARCANA_ASSETS")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::current_exe()
                    .ok()?
                    .ancestors()
                    .find(|p| p.join("research-lock.json").is_file())
                    .map(|p| p.join("arcana/assets/arcana_four_elements"))
            });
        if let Some(directory) = directory {
            if let Err(e) = self
                .audio
                .register_namespace("arcana_four_elements", &directory)
            {
                log::warn!("Arcana audio: {e}");
            }
        } else {
            log::warn!("Arcana audio assets were not found beside the repository");
        }
    }
    pub(super) fn arcana_fire_sound(&mut self, owner: u32, id: u64, cue: u8, at: DVec3) {
        self.arcana_prepare_audio();
        let key = (owner, id, cue);
        if self.arcana.school.audio_cues.contains_key(&key) {
            return;
        }
        if self.arcana.school.audio_cues.len() >= 512 {
            return;
        }
        self.arcana.school.audio_cues.insert(key, Instant::now());
        let stage = match cue {
            0 => "charge",
            1 => "release",
            2 => "impact",
            3 => "ultimate",
            _ => "sustain",
        };
        self.audio.play(
            &format!("arcana_four_elements:fire.{stage}"),
            Some(at.to_array()),
            if cue >= 10 { 0.45 } else { 0.85 },
            1.0,
        );
    }
}
