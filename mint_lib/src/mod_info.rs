use std::collections::{BTreeSet, HashMap};

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
        match (
            dependency_identity(&self.url),
            dependency_identity(&other.url),
        ) {
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

/// mod.io slug, mod ID and file ID that dependency matching compares.
fn dependency_identity(value: &str) -> Option<(String, Option<u64>, Option<u64>)> {
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

/// Answers dependency questions about a list of specs without comparing every pair.
///
/// Two specs can only match when they share a URL, a mod.io mod ID or a slug, so lookups check
/// [`ModSpecification::satisfies_dependency`] against those candidates alone.
pub struct DependencyIndex<K> {
    entries: Vec<(K, ModSpecification)>,
    by_url: HashMap<String, Vec<usize>>,
    by_id: HashMap<u64, Vec<usize>>,
    by_slug: HashMap<String, Vec<usize>>,
}

impl<K> DependencyIndex<K> {
    pub fn new(entries: impl IntoIterator<Item = (K, ModSpecification)>) -> Self {
        let mut index = Self {
            entries: entries.into_iter().collect(),
            by_url: HashMap::new(),
            by_id: HashMap::new(),
            by_slug: HashMap::new(),
        };
        for (i, (_, spec)) in index.entries.iter().enumerate() {
            index.by_url.entry(spec.url.clone()).or_default().push(i);
            if let Some((slug, id, _)) = dependency_identity(&spec.url) {
                if let Some(id) = id {
                    index.by_id.entry(id).or_default().push(i);
                }
                index.by_slug.entry(slug).or_default().push(i);
            }
        }
        index
    }

    fn candidates(&self, spec: &ModSpecification) -> impl Iterator<Item = &(K, ModSpecification)> {
        let mut found: Vec<usize> = self.by_url.get(&spec.url).cloned().unwrap_or_default();
        if let Some((slug, id, _)) = dependency_identity(&spec.url) {
            if let Some(id) = id {
                found.extend(self.by_id.get(&id).into_iter().flatten());
            }
            found.extend(self.by_slug.get(&slug).into_iter().flatten());
        }
        found.sort_unstable();
        found.dedup();
        found.into_iter().map(|i| &self.entries[i])
    }

    /// Whether the index was built from exactly these entries, so it can be reused.
    pub fn is_for(&self, entries: &[(K, ModSpecification)]) -> bool
    where
        K: PartialEq,
    {
        self.entries == entries
    }

    /// Whether `spec` satisfies an entry that `skip` does not exclude, that is
    /// `spec.satisfies_dependency(entry)`.
    pub fn satisfies_other(&self, spec: &ModSpecification, skip: impl Fn(&K) -> bool) -> bool {
        self.candidates(spec)
            .any(|(key, other)| !skip(key) && spec.satisfies_dependency(other))
    }

    /// Whether an entry satisfies the dependency `dependency`, that is
    /// `entry.satisfies_dependency(dependency)`.
    pub fn provides(&self, dependency: &ModSpecification) -> bool {
        self.candidates(dependency)
            .any(|(_, spec)| spec.satisfies_dependency(dependency))
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

    #[test]
    fn dependency_index_matches_comparing_every_pair() {
        let specs: Vec<_> = [
            "https://mod.io/g/drg/m/tool",
            "https://mod.io/g/drg/m/tool/",
            "https://mod.io/g/drg/m/tool#1",
            "https://mod.io/g/drg/m/tool#1/10",
            "https://mod.io/g/drg/m/tool#1/11",
            "https://mod.io/g/drg/m/renamed#1/10",
            "https://mod.io/g/drg/m/tool#12",
            "https://mod.io/g/drg/m/tool-extra",
            "https://mod.io/g/drg/m/tool/#description",
            "https://mod.io/g/drg/m/tool#1/10/2",
            "https://mod.io/g/drg/m/tool?x=1",
            "HTTPS://MOD.IO/g/drg/m/tool#1",
            "http://mod.io/g/drg/m/tool#1",
            "https://example.org/a",
            "https://example.org/ab",
            r"C:\mods\local.pak",
            "local.pak",
            "local.pak",
            "",
        ]
        .into_iter()
        .map(|url| ModSpecification::new(url.into()))
        .collect();
        let index = DependencyIndex::new(specs.iter().cloned().enumerate());
        for (key, spec) in specs.iter().enumerate() {
            let pairs = specs
                .iter()
                .enumerate()
                .any(|(other, s)| other != key && spec.satisfies_dependency(s));
            assert_eq!(
                index.satisfies_other(spec, |k| *k == key),
                pairs,
                "{}",
                spec.url
            );
            let provided = specs.iter().any(|s| s.satisfies_dependency(spec));
            assert_eq!(index.provides(spec), provided, "{}", spec.url);
        }
        let entries: Vec<_> = specs.iter().cloned().enumerate().collect();
        assert!(index.is_for(&entries));
        assert!(!index.is_for(&entries[1..]));
        let mut changed = entries.clone();
        changed[0].1 = ModSpecification::new("https://mod.io/g/drg/m/other".into());
        assert!(!index.is_for(&changed));
        for missing in [
            "https://mod.io/g/drg/m/other#99",
            "https://mod.io/g/drg/m/tool#1/12",
        ] {
            let missing = ModSpecification::new(missing.into());
            let provided = specs.iter().any(|s| s.satisfies_dependency(&missing));
            assert_eq!(index.provides(&missing), provided, "{}", missing.url);
            assert_eq!(
                index.satisfies_other(&missing, |_| false),
                specs.iter().any(|s| missing.satisfies_dependency(s))
            );
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
