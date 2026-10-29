use super::*;
impl Studio {
    pub(super) fn restore_pitch_settings(
        &self,
        dto: &UiSessionDto,
        key: &str,
        policy: &WorkspacePathPolicy,
    ) -> Result<Option<PitchShiftSpec>, String> {
        let saved = self.live_pitch_store.load(key).map_err(err)?;
        if saved.is_some() {
            return Ok(saved);
        }
        let Some(source) = atomusicator_infrastructure::storage::legacy_import::historical_source(
            policy,
            Path::new(&dto.directory),
        ) else {
            return Ok(None);
        };
        let previous = atomusicator_infrastructure::session_reader::read_session_files(
            &source,
            &CancellationToken::default(),
        )
        .map_err(err)?;
        let mut old = dto.clone();
        old.directory = previous.directory.to_string_lossy().into_owned();
        for track in &mut old.tracks {
            let kind = parse_stem(&track.stem_type)?;
            if let Some((_, file)) = previous.stems.iter().find(|(stem, _)| *stem == kind) {
                track.file_path = file.to_string_lossy().into_owned();
            }
        }
        let old_key = pitch_persistence_key(&old)?;
        let saved = self.live_pitch_store.load(&old_key).map_err(err)?;
        if let Some(spec) = saved {
            self.live_pitch_store.queue(key.into(), spec).map_err(err)?;
        }
        Ok(saved)
    }
    pub fn live_pitch_status(&self) -> Result<crate::api::UiLivePitchDto, String> {
        let loaded = self.loaded.lock().map_err(err)?;
        let loaded = loaded.as_ref().ok_or("no session loaded")?;
        Ok(live_pitch_dto(
            &loaded.dto.id,
            self.mixer.live_pitch().snapshot(),
        ))
    }
    pub fn set_live_pitch(
        &self,
        session_id: &str,
        epoch: u64,
        revision: u64,
        semitones: i32,
        preserve: bool,
    ) -> Result<crate::api::UiLivePitchDto, String> {
        let spec = PitchShiftSpec::try_new(semitones as f32, preserve).map_err(err)?;
        let loaded = self.loaded.lock().map_err(err)?;
        let loaded = loaded.as_ref().ok_or("no session loaded")?;
        if loaded.dto.id != session_id {
            return Err("pitch session changed; refresh the session".into());
        }
        let snapshot = self
            .mixer
            .live_pitch()
            .set(epoch, revision, spec)
            .map_err(err)?;
        self.live_pitch_store
            .queue(loaded.pitch_key.clone(), snapshot.spec)
            .map_err(err)?;
        Ok(live_pitch_dto(session_id, snapshot))
    }
}
pub(super) fn live_pitch_dto(
    id: &str,
    snapshot: atomusicator_infrastructure::live_pitch_control::PitchSnapshot,
) -> crate::api::UiLivePitchDto {
    crate::api::UiLivePitchDto {
        session_id: id.into(),
        epoch: snapshot.epoch,
        revision: snapshot.revision,
        applied_revision: snapshot.applied_revision,
        semitones: snapshot.spec.semitones() as i32,
        preserve_formants: snapshot.spec.preserve_formants(),
    }
}
pub(super) fn pitch_persistence_key(dto: &UiSessionDto) -> Result<String, String> {
    let mut key = format!("{}\0{}", dto.id, dto.directory);
    for track in &dto.tracks {
        let metadata = fs::metadata(&track.file_path).map_err(err)?;
        let modified = metadata
            .modified()
            .map_err(err)?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(err)?
            .as_nanos();
        key.push_str(&format!(
            "\0{}:{}:{modified}",
            track.file_path,
            metadata.len()
        ));
    }
    Ok(key)
}
