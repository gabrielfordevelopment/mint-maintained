use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Tags from mod.io.
#[derive(Debug, Clone)]
pub struct ModioTags {
    pub qol: bool,
    pub gameplay: bool,
    pub audio: bool,
    pub visual: bool,
    pub framework: bool,
    pub versions: BTreeSet<String>,
    pub required_status: RequiredStatus,
    pub approval_status: ApprovalStatus,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequiredStatus {
    RequiredByAll,
    Optional,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ApprovalStatus {
    Verified,
    Approved,
    Sandbox,
}

/// Whether a mod can be resolved by clients or not
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum ResolvableStatus {
    Unresolvable(String),
    Resolvable,
}

/// Returned from ModStore
#[derive(Debug, Clone)]
pub struct ModInfo {
    pub provider: &'static str,
    pub name: String,
    pub spec: ModSpecification,          // unpinned version
    pub versions: Vec<ModSpecification>, // pinned versions TODO make this a different type
    pub resolution: ModResolution,
    pub suggested_require: bool,
    pub suggested_dependencies: Vec<ModSpecification>, // ModResponse
    pub modio_tags: Option<ModioTags>,                 // only available for mods from mod.io
    pub modio_id: Option<u32>,                         // only available for mods from mod.io
}

/// Returned from ModProvider
#[derive(Debug, Clone)]
pub enum ModResponse {
    Redirect(ModSpecification),
    Resolve(ModInfo),
}

/// Points to a mod, optionally a specific version
#[derive(
    Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ModSpecification {
    pub url: String,
}

impl ModSpecification {
    pub fn new(url: String) -> Self {
        Self { url }
    }
    pub fn satisfies_dependency(&self, other: &ModSpecification) -> bool {
        if self.url == other.url {
            return true;
        }
        fn identity(value: &str) -> Option<(String, Option<u64>, Option<u64>)> {
            let url = reqwest::Url::parse(value).ok()?;
            if url.scheme() != "https" || url.host_str()? != "mod.io" || url.query().is_some() {
                return None;
            }
            let slug = url.path().strip_prefix("/g/drg/m/")?.trim_end_matches('/');
            if slug.is_empty() || slug.contains('/') {
                return None;
            }
            let mut ids = url.fragment().unwrap_or_default().split('/');
            let id = ids.next().and_then(|id| id.parse().ok());
            let file = ids.next().and_then(|id| id.parse().ok());
            if ids.next().is_some() {
                return None;
            }
            Some((slug.to_owned(), id, file))
        }
        match (identity(&self.url), identity(&other.url)) {
            (Some((slug, id, file)), Some((other_slug, other_id, other_file))) => {
                let same_mod = match (id, other_id) {
                    (Some(id), Some(other_id)) => id == other_id,
                    _ => slug == other_slug,
                };
                same_mod && other_file.is_none_or(|required| file == Some(required))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn dependency_identity_respects_slug_boundaries_and_pinned_versions() {
        let spec = |tail: &str| ModSpecification::new(format!("https://mod.io/g/drg/m/{tail}"));
        assert!(!spec("tool").satisfies_dependency(&spec("tool-extra")));
        assert!(!spec("tool-extra").satisfies_dependency(&spec("tool")));
        assert!(spec("tool#1/10").satisfies_dependency(&spec("tool#1")));
        assert!(spec("renamed#1/10").satisfies_dependency(&spec("tool#1/10")));
        assert!(!spec("tool#1/11").satisfies_dependency(&spec("tool#1/10")));
        assert!(!spec("tool#1").satisfies_dependency(&spec("tool#1/10")));
        assert!(!spec("tool#12").satisfies_dependency(&spec("tool#1")));
        assert!(spec("tool/#description").satisfies_dependency(&spec("tool")));
        for (left, right) in [
            ("local.pak", "local.pak2"),
            ("https://example.org/a", "https://example.org/ab"),
        ] {
            let left = ModSpecification::new(left.into());
            assert!(left.satisfies_dependency(&left));
            assert!(!left.satisfies_dependency(&ModSpecification::new(right.into())));
        }
    }
}

/// Points to a specific version of a specific mod
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ModResolution {
    pub url: ModIdentifier,
    pub status: ResolvableStatus,
}

impl ModResolution {
    pub fn resolvable(url: ModIdentifier) -> Self {
        Self {
            url,
            status: ResolvableStatus::Resolvable,
        }
    }
    pub fn unresolvable(url: ModIdentifier, name: String) -> Self {
        Self {
            url,
            status: ResolvableStatus::Unresolvable(name),
        }
    }
    /// Used to get the URL if resolvable or just return the mod name if not
    pub fn get_resolvable_url_or_name(&self) -> &str {
        match &self.status {
            ResolvableStatus::Resolvable => &self.url.0,
            ResolvableStatus::Unresolvable(name) => name,
        }
    }
}

/// Mod identifier used for tracking gameplay affecting status.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModIdentifier(pub String);

impl ModIdentifier {
    pub fn new(s: String) -> Self {
        Self(s)
    }
}
impl From<String> for ModIdentifier {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
impl From<&str> for ModIdentifier {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}

/// Stripped down mod info stored in the mod pak to be used in game
#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub version: String,
    pub mods: Vec<MetaMod>,
    pub config: MetaConfig,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct MetaConfig {}
#[derive(Debug, Serialize, Deserialize)]
pub struct MetaMod {
    pub name: String,
    pub version: String,
    pub url: String,
    pub author: String,
    pub approval: ApprovalStatus,
    pub required: bool,
}
impl Meta {
    pub fn to_server_list_string(&self) -> String {
        use itertools::Itertools;

        ["mint".into(), self.version.to_string()]
            .into_iter()
            .chain(
                self.mods
                    .iter()
                    .sorted_by_key(|m| (std::cmp::Reverse(m.approval), &m.name))
                    .take(100)
                    .flat_map(|m| {
                        [
                            match m.approval {
                                ApprovalStatus::Verified => 'V',
                                ApprovalStatus::Approved => 'A',
                                ApprovalStatus::Sandbox => 'S',
                            }
                            .into(),
                            m.name.replace(';', ""),
                        ]
                    }),
            )
            .join(";")
    }
}
