use crate::providers::ModInfo;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const ENTRY: &str = "mint-installation.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledMod {
    pub identity: String,
    pub resolution: String,
    pub local_stamp: Option<(u64, u128)>,
}

impl InstalledMod {
    pub fn from_info(info: &ModInfo) -> Self {
        Self {
            identity: info.spec.url.clone(),
            resolution: info.resolution.url.0.clone(),
            local_stamp: None,
        }
    }

    pub fn read_local_stamp(&mut self) -> std::io::Result<()> {
        if !self.resolution.starts_with("https://") && !self.resolution.starts_with("http://") {
            let metadata = std::fs::metadata(&self.resolution)?;
            let modified = metadata
                .modified()?
                .duration_since(UNIX_EPOCH)
                .map_err(std::io::Error::other)?;
            self.local_stamp = Some((metadata.len(), modified.as_nanos()));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstallationRecord {
    pub version: u32,
    pub mods: Vec<InstalledMod>,
}

impl InstallationRecord {
    pub fn from_mods(mods: &[(ModInfo, PathBuf)]) -> std::io::Result<Self> {
        let mods = mods
            .iter()
            .map(|(info, _)| {
                let mut entry = InstalledMod::from_info(info);
                entry.read_local_stamp()?;
                Ok(entry)
            })
            .collect::<std::io::Result<Vec<_>>>()?;
        Ok(Self { version: 1, mods })
    }
}

#[derive(Debug, Clone)]
pub enum InstalledState {
    NotInstalled,
    Known(Vec<InstalledMod>),
    Unknown(String),
}

pub fn read_installation(game_pak: &Path) -> InstalledState {
    fn read(game_pak: &Path) -> Result<InstalledState, Box<dyn std::error::Error>> {
        let installation =
            mint_lib::DRGInstallation::from_pak_path(game_pak).map_err(|e| e.to_string())?;
        let file = match File::open(installation.paks_path().join("mods_P.pak")) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(InstalledState::NotInstalled);
            }
            Err(error) => return Err(error.into()),
        };
        let mut reader = BufReader::new(file);
        let pak = repak::PakBuilder::new().reader(&mut reader)?;
        if !pak.files().iter().any(|p| p == ENTRY) {
            return Ok(InstalledState::Unknown(
                "This installation predates version tracking. Install mods to track changes."
                    .into(),
            ));
        }
        let record: InstallationRecord = serde_json::from_slice(&pak.get(ENTRY, &mut reader)?)?;
        if record.version != 1 {
            return Err("Unsupported installation record version".into());
        }
        Ok(InstalledState::Known(record.mods))
    }
    read(game_pak).unwrap_or_else(|e| {
        InstalledState::Unknown(format!("Could not read installation status: {e}"))
    })
}

/// Path, size and modification time of the installed mods pak.
type PakStamp = (PathBuf, u64, SystemTime);

fn pak_stamp(game_pak: &Path) -> Option<PakStamp> {
    let installation = mint_lib::DRGInstallation::from_pak_path(game_pak).ok()?;
    let path = installation.paks_path().join("mods_P.pak");
    let metadata = std::fs::metadata(&path).ok()?;
    Some((path, metadata.len(), metadata.modified().ok()?))
}

/// Reads the installation record again only after the installed pak changed. Large paks have
/// large indexes, so periodic status checks must not parse them while nothing changes.
#[derive(Default)]
pub struct InstallationCache {
    last: Option<(PakStamp, InstalledState)>,
}

impl InstallationCache {
    pub fn read(&mut self, game_pak: &Path) -> InstalledState {
        let stamp = pak_stamp(game_pak);
        if let (Some(stamp), Some((last, state))) = (&stamp, &self.last)
            && stamp == last
        {
            return state.clone();
        }
        let state = read_installation(game_pak);
        self.last = stamp.map(|stamp| (stamp, state.clone()));
        state
    }

    /// Forgets the last read, for example after mods were installed or removed.
    pub fn clear(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufWriter;

    #[test]
    fn installed_record_round_trips_and_legacy_invalid_future_records_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let paks = dir.path().join("FSD/Content/Paks");
        std::fs::create_dir_all(&paks).unwrap();
        std::fs::create_dir_all(dir.path().join("FSD/Binaries/Win64")).unwrap();
        let game = paks.join("FSD-WindowsNoEditor.pak");
        std::fs::write(&game, b"game path fixture").unwrap();
        let installed = paks.join("mods_P.pak");
        assert!(matches!(
            read_installation(&game),
            InstalledState::NotInstalled
        ));
        let entry = InstalledMod {
            identity: "https://mod.io/g/drg/m/example#1".into(),
            resolution: "https://mod.io/g/drg/m/example#1/10".into(),
            local_stamp: None,
        };
        for data in [
            None,
            Some(b"bad json".to_vec()),
            Some(br#"{"version":2,"mods":[]}"#.to_vec()),
            Some(
                serde_json::to_vec(&InstallationRecord {
                    version: 1,
                    mods: vec![entry.clone()],
                })
                .unwrap(),
            ),
        ] {
            let mut pak = repak::PakBuilder::new().writer(
                BufWriter::new(File::create(&installed).unwrap()),
                repak::Version::V11,
                "../../../".into(),
                None,
            );
            if let Some(data) = &data {
                pak.write_file(ENTRY, false, data).unwrap();
            }
            pak.write_index().unwrap();
            let bytes = std::fs::read(&installed).unwrap();
            match read_installation(&game) {
                InstalledState::Known(mods) => assert_eq!(mods, std::slice::from_ref(&entry)),
                InstalledState::Unknown(_) => assert!(
                    !data
                        .as_ref()
                        .is_some_and(|d| String::from_utf8_lossy(d).contains("https://"))
                ),
                InstalledState::NotInstalled => {
                    panic!("Existing pak must not be treated as absent")
                }
            }
            assert_eq!(std::fs::read(&installed).unwrap(), bytes);
        }
    }

    #[test]
    fn installation_cache_reads_the_pak_again_only_after_it_changed() {
        let dir = tempfile::tempdir().unwrap();
        let paks = dir.path().join("FSD/Content/Paks");
        std::fs::create_dir_all(&paks).unwrap();
        let game = paks.join("FSD-WindowsNoEditor.pak");
        std::fs::write(&game, b"game path fixture").unwrap();
        let installed = paks.join("mods_P.pak");
        let write = |mods: &[InstalledMod]| {
            let mut pak = repak::PakBuilder::new().writer(
                BufWriter::new(File::create(&installed).unwrap()),
                repak::Version::V11,
                "../../../".into(),
                None,
            );
            let record = InstallationRecord {
                version: 1,
                mods: mods.to_vec(),
            };
            pak.write_file(ENTRY, false, serde_json::to_vec(&record).unwrap())
                .unwrap();
            pak.write_index().unwrap();
        };
        let entry = |n: u32| InstalledMod {
            identity: format!("https://mod.io/g/drg/m/example#{n}"),
            resolution: format!("https://mod.io/g/drg/m/example#{n}/10"),
            local_stamp: None,
        };
        let known = |state: InstalledState| match state {
            InstalledState::Known(mods) => mods,
            other => panic!("unexpected state {other:?}"),
        };
        let mut cache = InstallationCache::default();
        assert!(matches!(cache.read(&game), InstalledState::NotInstalled));
        write(&[entry(1)]);
        assert_eq!(known(cache.read(&game)), [entry(1)]);
        // An unchanged pak is not parsed again, so even a damaged one keeps its last state.
        let modified = std::fs::metadata(&installed).unwrap().modified().unwrap();
        let len = std::fs::metadata(&installed).unwrap().len() as usize;
        std::fs::write(&installed, vec![0; len]).unwrap();
        File::options()
            .write(true)
            .open(&installed)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(known(cache.read(&game)), [entry(1)]);
        cache.clear();
        assert!(matches!(cache.read(&game), InstalledState::Unknown(_)));
        // A new installation is a different file, so it is read again.
        write(&[entry(1), entry(2)]);
        assert_eq!(known(cache.read(&game)), [entry(1), entry(2)]);
        std::fs::remove_file(&installed).unwrap();
        assert!(matches!(cache.read(&game), InstalledState::NotInstalled));
    }

    #[test]
    fn changed_local_files_change_the_installation_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local.pak");
        std::fs::write(&path, b"old").unwrap();
        let mut entry = InstalledMod {
            identity: path.to_string_lossy().into(),
            resolution: path.to_string_lossy().into(),
            local_stamp: None,
        };
        entry.read_local_stamp().unwrap();
        let old = entry.clone();
        std::fs::write(&path, b"changed content").unwrap();
        entry.read_local_stamp().unwrap();
        assert_ne!(entry, old);
    }
}
