//! Receiving a density model trained elsewhere.
//!
//! A model and the tuning it was trained under travel together (`model.onnx`
//! and `site.json`), because pairing a model with a window it never saw
//! produces estimates that look plausible and are not.
//!
//! Nothing is trusted on arrival: the archive comes from a browser, so its
//! members are checked by name and by size before anything is written, and
//! the model is only activated once it has been loaded and matched against
//! this appliance's receivers.

use std::path::{Path, PathBuf};

use flow_infer::{DensityModel, LiveConfig, LivePipeline};
use serde::{Deserialize, Serialize};

use crate::config::SiteTuning;
use crate::error::ModelError;

/// The model file inside a bundle.
pub const BUNDLE_MODEL: &str = "model.onnx";

/// The tuning file inside a bundle.
pub const BUNDLE_SITE: &str = "site.json";

/// Largest bundle accepted, uncompressed.
///
/// A classical model is kilobytes; a megabyte is already generous. The cap is
/// against a decompression bomb rather than against a real model — the
/// archive arrives from a browser, and an authenticated caller is not a
/// trusted one.
const MAX_MEMBER_BYTES: u64 = 32 * 1024 * 1024;

/// The manifest a bundle carries.
pub const BUNDLE_MANIFEST: &str = "model.json";

/// What a model says about itself.
///
/// The appliance can date the moment it received a bundle, but not the run
/// that produced it — and the two answer different questions when estimates
/// change. A bundle without a manifest is still usable; it is simply
/// anonymous, and named after the moment it arrived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Short name the training run gave itself.
    pub name: String,
    /// When it was trained, as the run recorded it.
    #[serde(default)]
    pub trained_at: String,
    /// How many recorded sessions it was trained on.
    #[serde(default)]
    pub sessions: u32,
}

/// One model held by the appliance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredModel {
    /// Directory name, and the handle every action uses.
    pub id: String,
    /// What the bundle said about itself, when it said anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Manifest>,
    /// When this appliance received it.
    pub imported_at_us: u64,
    /// The analysis window it was trained under, in µs.
    pub window_us: u64,
    /// Receivers it expects, derived from its input width.
    pub receivers: usize,
    /// Whether it is the one estimating right now.
    pub active: bool,
}

/// A bundle unpacked but not yet in service.
#[derive(Debug)]
pub struct StagedBundle {
    /// Where the candidate model was written.
    pub model_path: PathBuf,
    /// The tuning it was trained under.
    pub tuning: SiteTuning,
    /// What it says about itself, when it says anything.
    pub manifest: Option<Manifest>,
}

/// Unpacks a bundle into `staging`, keeping only what it should hold.
///
/// # Errors
///
/// [`ModelError`] when the archive cannot be read, carries a member it should
/// not, or is missing one it must.
pub fn stage(archive: &[u8], staging: &Path) -> Result<StagedBundle, ModelError> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).map_err(|err| ModelError::Staging(err.to_string()))?;

    let decoder = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar
        .entries()
        .map_err(|err| ModelError::Unreadable(err.to_string()))?;

    for entry in entries {
        let mut entry = entry.map_err(|err| ModelError::Unreadable(err.to_string()))?;
        let path = entry
            .path()
            .map_err(|err| ModelError::Unreadable(err.to_string()))?
            .into_owned();
        let name = path.to_string_lossy();
        // The rule is about paths, not names. A member that could be written
        // anywhere but the staging directory is refused outright; a member
        // that is merely not one of the two — a README, or the metadata a
        // desktop archiver slips in — is skipped, because it is harmless and
        // refusing it would turn an ordinary archive into a puzzle.
        if name.starts_with('/')
            || path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(ModelError::UnexpectedMember(name.into_owned()));
        }
        if name != BUNDLE_MODEL && name != BUNDLE_SITE && name != BUNDLE_MANIFEST {
            continue;
        }
        if entry.size() > MAX_MEMBER_BYTES {
            return Err(ModelError::TooLarge {
                member: name.into_owned(),
            });
        }
        entry
            .unpack(staging.join(name.as_ref()))
            .map_err(|err| ModelError::Staging(err.to_string()))?;
    }

    let model_path = staging.join(BUNDLE_MODEL);
    if !model_path.is_file() {
        return Err(ModelError::MissingMember(BUNDLE_MODEL));
    }
    let site = std::fs::read(staging.join(BUNDLE_SITE))
        .map_err(|_| ModelError::MissingMember(BUNDLE_SITE))?;
    let tuning: SiteTuning =
        serde_json::from_slice(&site).map_err(|err| ModelError::Tuning(err.to_string()))?;
    tuning
        .validate()
        .map_err(|err| ModelError::Tuning(err.to_string()))?;

    // Absent rather than fatal: an older bundle predates the manifest, and
    // refusing it would strand a model that works.
    let manifest = std::fs::read(staging.join(BUNDLE_MANIFEST))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    Ok(StagedBundle {
        model_path,
        tuning,
        manifest,
    })
}

/// Checks that a staged bundle can actually drive this appliance.
///
/// Verified by building the pipeline it would run, rather than by a check of
/// its own: the pipeline already knows what it requires, and a second opinion
/// would be one more thing to keep in step.
///
/// # Errors
///
/// [`ModelError::Unusable`] naming what does not fit — most often a model
/// trained for a different number of receivers.
pub fn check(bundle: &StagedBundle, rx_nodes: Vec<String>) -> Result<(), ModelError> {
    let model = DensityModel::load(&bundle.model_path)
        .map_err(|err| ModelError::Unusable(err.to_string()))?;
    LivePipeline::new(
        model,
        LiveConfig {
            window_us: bundle.tuning.window_us,
            hop_us: bundle.tuning.hop_us,
            rx_nodes,
            wait: bundle.tuning.wait_config(),
        },
    )
    .map(|_| ())
    .map_err(|err| ModelError::Unusable(err.to_string()))
}

/// Where the appliance keeps every model it has been given.
pub fn library_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models")
}

/// Files an appliance keeps for each model it holds.
const KEPT: [&str; 3] = [BUNDLE_MODEL, BUNDLE_SITE, BUNDLE_MANIFEST];

/// Files a staged bundle into the library and returns its handle.
///
/// Kept rather than replaced: a site that has been recalibrated twice should
/// be able to go back to the model that was working, not only to the one
/// immediately before.
///
/// # Errors
///
/// [`ModelError::Staging`] if the files cannot be moved into place.
pub fn store(bundle: &StagedBundle, data_dir: &Path, now_us: u64) -> Result<String, ModelError> {
    let id = model_id(bundle.manifest.as_ref(), now_us);
    let dir = library_dir(data_dir).join(&id);
    std::fs::create_dir_all(&dir).map_err(|err| ModelError::Staging(err.to_string()))?;

    let staging = bundle
        .model_path
        .parent()
        .ok_or_else(|| ModelError::Staging("staged bundle has no directory".to_owned()))?;
    for name in KEPT {
        let from = staging.join(name);
        if from.is_file() {
            std::fs::rename(&from, dir.join(name))
                .map_err(|err| ModelError::Staging(err.to_string()))?;
        }
    }
    Ok(id)
}

/// Puts a stored model into service.
///
/// Copied to the path the pipeline loads rather than pointed at: one place to
/// read a model from means the intake never has to know a library exists.
///
/// # Errors
///
/// [`ModelError::Unknown`] if no such model is held, or a copy failure.
pub fn activate(data_dir: &Path, id: &str) -> Result<SiteTuning, ModelError> {
    let dir = library_dir(data_dir).join(id);
    if !dir.is_dir() {
        return Err(ModelError::Unknown(id.to_owned()));
    }
    let tuning: SiteTuning = serde_json::from_slice(
        &std::fs::read(dir.join(BUNDLE_SITE))
            .map_err(|err| ModelError::Staging(err.to_string()))?,
    )
    .map_err(|err| ModelError::Tuning(err.to_string()))?;

    std::fs::copy(dir.join(BUNDLE_MODEL), data_dir.join(crate::ACTIVE_MODEL))
        .map_err(|err| ModelError::Staging(err.to_string()))?;
    Ok(tuning)
}

/// Gives a stored model a new name.
///
/// The manifest is created when the bundle arrived without one, which is also
/// how an anonymous bundle stops being anonymous. The directory keeps its
/// name: it is the handle the configuration records as being in service, and
/// moving it would mean rewriting that in the same breath, with an appliance
/// pointing at a model that no longer exists if only one of the two lands.
///
/// # Errors
///
/// [`ModelError::Unknown`] if no model is held under that handle, or a write
/// failure.
pub fn rename(data_dir: &Path, id: &str, name: &str) -> Result<(), ModelError> {
    let dir = library_dir(data_dir).join(id);
    if !dir.is_dir() {
        return Err(ModelError::Unknown(id.to_owned()));
    }
    let path = dir.join(BUNDLE_MANIFEST);
    let mut manifest: Manifest = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| Manifest {
            name: String::new(),
            trained_at: String::new(),
            sessions: 0,
        });
    manifest.name = name.to_owned();

    let rendered =
        serde_json::to_vec_pretty(&manifest).map_err(|err| ModelError::Staging(err.to_string()))?;
    std::fs::write(&path, rendered).map_err(|err| ModelError::Staging(err.to_string()))
}

/// Removes a stored model.
///
/// # Errors
///
/// [`ModelError::Unknown`] if no such model is held.
pub fn remove(data_dir: &Path, id: &str) -> Result<(), ModelError> {
    let dir = library_dir(data_dir).join(id);
    if !dir.is_dir() {
        return Err(ModelError::Unknown(id.to_owned()));
    }
    std::fs::remove_dir_all(&dir).map_err(|err| ModelError::Staging(err.to_string()))
}

/// Every model the appliance holds, newest first.
#[must_use]
pub fn library(data_dir: &Path, active: Option<&str>) -> Vec<StoredModel> {
    let Ok(entries) = std::fs::read_dir(library_dir(data_dir)) else {
        return Vec::new();
    };

    let mut models: Vec<StoredModel> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| describe(&entry.path(), active))
        .collect();
    models.sort_by_key(|model| std::cmp::Reverse(model.imported_at_us));
    models
}

fn describe(dir: &Path, active: Option<&str>) -> Option<StoredModel> {
    let id = dir.file_name()?.to_string_lossy().into_owned();
    let tuning: SiteTuning =
        serde_json::from_slice(&std::fs::read(dir.join(BUNDLE_SITE)).ok()?).ok()?;
    let manifest: Option<Manifest> = std::fs::read(dir.join(BUNDLE_MANIFEST))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let receivers = DensityModel::load(&dir.join(BUNDLE_MODEL))
        .ok()
        .map_or(0, |model| model.n_features() / NODE_FEATURE_COUNT);

    Some(StoredModel {
        active: active == Some(id.as_str()),
        imported_at_us: imported_at(&id).unwrap_or(0),
        window_us: tuning.window_us,
        receivers,
        manifest,
        id,
    })
}

/// Features each receiver contributes, mirrored from the inference crate so a
/// listing can say how many receivers a model expects.
const NODE_FEATURE_COUNT: usize = 7;

/// A handle that sorts by arrival and reads as what it is.
fn model_id(manifest: Option<&Manifest>, now_us: u64) -> String {
    let seconds = i64::try_from(now_us / 1_000_000).unwrap_or(0);
    let stamp = jiff::Timestamp::from_second(seconds).map_or_else(
        |_| seconds.to_string(),
        |ts| ts.strftime("%Y%m%dT%H%M%SZ").to_string(),
    );
    let name = manifest.map_or("model", |m| m.name.as_str());
    format!("{stamp}-{}", slug(name))
}

fn slug(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "model".to_owned()
    } else {
        trimmed
    }
}

fn imported_at(id: &str) -> Option<u64> {
    let stamp = id.split('-').next()?;
    let parsed = jiff::civil::DateTime::strptime("%Y%m%dT%H%M%SZ", stamp).ok()?;
    let seconds = parsed
        .to_zoned(jiff::tz::TimeZone::UTC)
        .ok()?
        .timestamp()
        .as_second();
    u64::try_from(seconds).ok().map(|s| s * 1_000_000)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// Builds a gzipped tar from `(name, bytes)` members.
    fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (name, bytes) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn site_json() -> Vec<u8> {
        br#"{"people_per_class":[0.0,4.0,12.0,25.0],"service_rate_per_min":6.0,
             "smoothing_tau_s":30.0,"hysteresis_margin":0.15,"min_confidence":0.5,
             "window_us":5000000,"hop_us":1000000}"#
            .to_vec()
    }

    fn real_model() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../flow-infer/tests/fixtures/model.onnx"
        ))
        .unwrap()
    }

    #[test]
    fn a_bundle_yields_its_model_and_the_tuning_it_was_trained_under() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = archive(&[(BUNDLE_MODEL, &real_model()), (BUNDLE_SITE, &site_json())]);

        let staged = stage(&bytes, &dir.path().join("staging")).unwrap();

        assert!(staged.model_path.is_file());
        // The value that must never be chosen apart from the run.
        assert_eq!(staged.tuning.window_us, 5_000_000);
    }

    /// A tar entry written byte by byte, so a name the `tar` builder refuses
    /// to produce can still be fed to the reader — which is what a hostile
    /// client would send.
    fn raw_entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..107].copy_from_slice(b"0000644");
        header[108..115].copy_from_slice(b"0000000");
        header[116..123].copy_from_slice(b"0000000");
        header[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        header[136..147].copy_from_slice(b"00000000000");
        header[156] = b'0';
        header[148..156].copy_from_slice(b"        ");
        let checksum: u32 = header.iter().map(|&b| u32::from(b)).sum();
        header[148..154].copy_from_slice(format!("{checksum:06o}").as_bytes());
        header[154] = 0;
        header[155] = b' ';

        let mut bytes = header.to_vec();
        bytes.extend_from_slice(data);
        bytes.resize(bytes.len().div_ceil(512) * 512, 0);
        bytes
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.write_all(&[0u8; 1024]).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn a_member_the_bundle_should_not_hold_is_refused() {
        // Matched against the two known names rather than sanitised: a path
        // that could climb out of the staging directory is not a bundle with
        // a stray file, it is not this appliance's bundle.
        let dir = tempfile::tempdir().unwrap();
        for hostile in ["../escape.onnx", "../../etc/passwd", "/absolute.onnx"] {
            let bytes = gzip(&raw_entry(hostile, b"x"));
            let outcome = stage(&bytes, &dir.path().join("staging"));
            assert!(
                matches!(outcome, Err(ModelError::UnexpectedMember(_))),
                "{hostile} was accepted: {outcome:?}"
            );
            assert!(
                !dir.path().join("escape.onnx").exists(),
                "{hostile} wrote outside the staging directory"
            );
        }
    }

    #[test]
    fn a_harmless_extra_member_is_skipped_rather_than_refused() {
        // A desktop archiver slips its own metadata into an archive, and a
        // bundle may carry a note. Neither is a threat, and refusing them
        // would turn an ordinary archive into a puzzle.
        let dir = tempfile::tempdir().unwrap();
        let bytes = archive(&[
            ("._model.onnx", b"apple metadata"),
            ("README.txt", b"trained 2026-08-01"),
            (BUNDLE_MODEL, &real_model()),
            (BUNDLE_SITE, &site_json()),
        ]);

        let staged = stage(&bytes, &dir.path().join("staging")).unwrap();

        assert!(staged.model_path.is_file());
        assert!(!dir.path().join("staging/README.txt").exists());
    }

    #[test]
    fn a_bundle_missing_a_half_is_refused() {
        let dir = tempfile::tempdir().unwrap();

        let without_tuning = stage(
            &archive(&[(BUNDLE_MODEL, &real_model())]),
            &dir.path().join("a"),
        );
        assert!(matches!(
            without_tuning,
            Err(ModelError::MissingMember(BUNDLE_SITE))
        ));

        let without_model = stage(
            &archive(&[(BUNDLE_SITE, &site_json())]),
            &dir.path().join("b"),
        );
        assert!(matches!(
            without_model,
            Err(ModelError::MissingMember(BUNDLE_MODEL))
        ));
    }

    #[test]
    fn something_that_is_not_an_archive_is_refused_by_its_kind() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = stage(b"this is not a tar.gz", &dir.path().join("staging"));
        assert!(matches!(outcome, Err(ModelError::Unreadable(_))));
    }

    #[test]
    fn unusable_tuning_is_refused_before_anything_is_activated() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = archive(&[
            (BUNDLE_MODEL, &real_model()),
            (
                BUNDLE_SITE,
                br#"{"people_per_class":[0.0,4.0,12.0,25.0],
                "service_rate_per_min":0.0,"smoothing_tau_s":30.0,
                "hysteresis_margin":0.15,"min_confidence":0.5,
                "window_us":5000000,"hop_us":1000000}"#,
            ),
        ]);

        assert!(matches!(
            stage(&bytes, &dir.path().join("staging")),
            Err(ModelError::Tuning(_))
        ));
    }

    #[test]
    fn a_model_trained_for_other_receivers_is_refused_with_the_counts() {
        // The fixture model was trained for one receiver; an appliance with
        // two of them must not take it, and must say why.
        let dir = tempfile::tempdir().unwrap();
        let bytes = archive(&[(BUNDLE_MODEL, &real_model()), (BUNDLE_SITE, &site_json())]);
        let staged = stage(&bytes, &dir.path().join("staging")).unwrap();

        let outcome = check(&staged, vec!["rx-1".into(), "rx-2".into()]);

        let Err(ModelError::Unusable(message)) = outcome else {
            panic!("a mismatched model was accepted");
        };
        assert!(
            message.contains('2') || message.contains("feature"),
            "{message}"
        );
    }

    /// Stages a real bundle into `staging` under `data`.
    fn staged_at(data: &std::path::Path, name: &str) -> StagedBundle {
        let bytes = archive(&[
            (BUNDLE_MODEL, &real_model()),
            (BUNDLE_SITE, &site_json()),
            (
                BUNDLE_MANIFEST,
                format!(r#"{{"name":"{name}","trained_at":"2026-08-12","sessions":3}}"#).as_bytes(),
            ),
        ]);
        stage(&bytes, &data.join("staging")).unwrap()
    }

    #[test]
    fn a_stored_model_keeps_what_it_says_about_itself() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();

        let id = store(
            &staged_at(data, "campagne-juin"),
            data,
            1_785_600_000_000_000,
        )
        .unwrap();

        // The handle sorts by arrival and still reads as what it is.
        assert!(id.starts_with("20260801T"), "{id}");
        assert!(id.ends_with("-campagne-juin"), "{id}");

        let listed = library(data, Some(&id));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].manifest.as_ref().unwrap().name, "campagne-juin");
        assert_eq!(listed[0].window_us, 5_000_000);
        assert_eq!(listed[0].receivers, 1);
        assert!(listed[0].active);
    }

    #[test]
    fn the_library_keeps_every_model_newest_first() {
        // A site recalibrated twice should be able to go back to the model
        // that was working, not only to the one immediately before.
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let first = store(&staged_at(data, "essai-mai"), data, 1_785_000_000_000_000).unwrap();
        let second = store(
            &staged_at(data, "campagne-juin"),
            data,
            1_785_600_000_000_000,
        )
        .unwrap();

        let listed = library(data, Some(&second));

        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second, "newest first");
        assert!(listed[0].active);
        assert!(!listed[1].active);
        assert_eq!(listed[1].id, first);
    }

    #[test]
    fn activating_a_stored_model_puts_it_where_the_intake_reads() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let id = store(
            &staged_at(data, "campagne-juin"),
            data,
            1_785_600_000_000_000,
        )
        .unwrap();

        let tuning = activate(data, &id).unwrap();

        assert_eq!(tuning.window_us, 5_000_000);
        assert!(data.join(crate::ACTIVE_MODEL).is_file());
    }

    #[test]
    fn a_model_the_appliance_does_not_hold_is_named_in_the_refusal() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            activate(dir.path(), "no-such-model"),
            Err(ModelError::Unknown(_))
        ));
        assert!(matches!(
            remove(dir.path(), "no-such-model"),
            Err(ModelError::Unknown(_))
        ));
    }

    #[test]
    fn an_anonymous_bundle_is_still_stored() {
        // A bundle predating the manifest works; it is simply named after the
        // moment it arrived.
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let bytes = archive(&[(BUNDLE_MODEL, &real_model()), (BUNDLE_SITE, &site_json())]);
        let bundle = stage(&bytes, &data.join("staging")).unwrap();
        assert!(bundle.manifest.is_none());

        let id = store(&bundle, data, 1_785_600_000_000_000).unwrap();

        assert!(id.ends_with("-model"), "{id}");
        assert!(library(data, None)[0].manifest.is_none());
    }

    #[test]
    fn renaming_rewrites_the_manifest_and_leaves_the_handle_alone() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let bytes = archive(&[
            (BUNDLE_MODEL, &real_model()),
            (BUNDLE_SITE, &site_json()),
            (
                BUNDLE_MANIFEST,
                br#"{"name":"campagne-juin","trained_at":"2026-06-24","sessions":3}"#,
            ),
        ]);
        let bundle = stage(&bytes, &data.join("staging")).unwrap();
        let id = store(&bundle, data, 1_785_600_000_000_000).unwrap();

        rename(data, &id, "Campagne de septembre").unwrap();

        let stored = library(data, Some(&id));
        assert_eq!(stored[0].id, id, "the handle is what the config records");
        let manifest = stored[0].manifest.as_ref().unwrap();
        assert_eq!(manifest.name, "Campagne de septembre");
        // Untouched: the appliance cannot know when a run happened, so it
        // must not overwrite what the run said.
        assert_eq!(manifest.trained_at, "2026-06-24");
        assert_eq!(manifest.sessions, 3);
    }

    #[test]
    fn renaming_an_anonymous_bundle_gives_it_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let bytes = archive(&[(BUNDLE_MODEL, &real_model()), (BUNDLE_SITE, &site_json())]);
        let bundle = stage(&bytes, &data.join("staging")).unwrap();
        let id = store(&bundle, data, 1_785_600_000_000_000).unwrap();

        rename(data, &id, "Modèle de secours").unwrap();

        assert_eq!(
            library(data, None)[0].manifest.as_ref().unwrap().name,
            "Modèle de secours"
        );
    }

    #[test]
    fn renaming_a_model_the_appliance_does_not_hold_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            rename(dir.path(), "no-such-model", "whatever"),
            Err(ModelError::Unknown(_))
        ));
    }
}
