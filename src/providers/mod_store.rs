use std::collections::HashSet;
use std::path::Path;

use snafu::prelude::*;
use tracing::*;

use crate::providers::*;
use crate::state::config::ConfigWrapper;

pub struct ModStore {
    providers: Providers,
    cache: ProviderCache,
    blob_cache: BlobCache,
}

#[derive(Debug, Default)]
pub struct ResolvedMods {
    pub mods: Vec<(ModSpecification, ModInfo)>,
    pub errors: Vec<(ModSpecification, ProviderError)>,
}

impl ModStore {
    pub fn new<P: AsRef<Path>>(
        cache_path: P,
        parameters: &HashMap<String, HashMap<String, String>>,
    ) -> Result<Self, ProviderError> {
        let mut providers = HashMap::new();
        for prov in Self::get_provider_factories() {
            let params = parameters.get(prov.id).cloned().unwrap_or_default();
            if prov.parameters.iter().all(|p| params.contains_key(p.id)) {
                let Ok(provider) = (prov.new)(&params) else {
                    return Err(ProviderError::InitProviderFailed { id: prov.id });
                };
                providers.insert(prov.id, provider);
            }
        }

        let cache_metadata_path = cache_path.as_ref().join("cache.json");

        let cache = read_cache_metadata_or_default(&cache_metadata_path)?;
        let cache = ConfigWrapper::new(&cache_metadata_path, cache);
        cache
            .save()
            .map_err(|error| ProviderError::CacheSaveFailed {
                path: cache_metadata_path,
                message: error.to_string(),
            })?;

        Ok(Self {
            providers: RwLock::new(providers),
            cache: Arc::new(RwLock::new(cache)),
            blob_cache: BlobCache::new(cache_path.as_ref().join("blobs")),
        })
    }

    pub fn get_provider_factories() -> impl Iterator<Item = &'static ProviderFactory> {
        inventory::iter::<ProviderFactory>()
    }

    pub fn add_provider(
        &self,
        provider_factory: &ProviderFactory,
        parameters: &HashMap<String, String>,
    ) -> Result<(), ProviderError> {
        let provider = (provider_factory.new)(parameters)?;
        self.providers
            .write()
            .unwrap()
            .insert(provider_factory.id, provider);
        Ok(())
    }

    pub async fn add_provider_checked(
        &self,
        provider_factory: &ProviderFactory,
        parameters: &HashMap<String, String>,
    ) -> Result<(), ProviderError> {
        let provider = (provider_factory.new)(parameters)?;
        provider.check().await?;
        self.providers
            .write()
            .unwrap()
            .insert(provider_factory.id, provider);
        Ok(())
    }

    pub fn get_provider(&self, url: &str) -> Result<Arc<dyn ModProvider>, ProviderError> {
        let factory = Self::get_provider_factories()
            .find(|f| (f.can_provide)(url))
            .context(ProviderNotFoundSnafu {
                url: url.to_string(),
            })?;
        let lock = self.providers.read().unwrap();
        Ok(match lock.get(factory.id) {
            Some(e) => e.clone(),
            None => NoProviderSnafu {
                url: url.to_string(),
                factory,
            }
            .fail()?,
        })
    }

    pub async fn resolve_mods(
        &self,
        mods: &[ModSpecification],
        update: bool,
    ) -> Result<HashMap<ModSpecification, ModInfo>, ProviderError> {
        let resolved = self.resolve_mods_partial(mods, update).await;
        if let Some((_, error)) = resolved.errors.into_iter().next() {
            return Err(error);
        }
        Ok(resolved.mods.into_iter().collect())
    }

    pub async fn resolve_mods_partial(
        &self,
        mods: &[ModSpecification],
        update: bool,
    ) -> ResolvedMods {
        use futures::stream::{self, StreamExt};
        let mut queued = HashSet::new();
        let mut pending: Vec<_> = mods
            .iter()
            .filter(|spec| queued.insert((*spec).clone()))
            .cloned()
            .collect();
        let mut result = ResolvedMods::default();
        while !pending.is_empty() {
            let batch = stream::iter(pending.into_iter().map(|spec| async move {
                let resolved = self.resolve_mod(spec.clone(), update).await;
                (spec, resolved)
            }))
            .buffered(5)
            .collect::<Vec<_>>()
            .await;
            pending = Vec::new();
            for (spec, resolved) in batch {
                match resolved {
                    Ok((original, info)) => {
                        queued.insert(info.spec.clone());
                        let mut dependencies = info.suggested_dependencies.clone();
                        dependencies.sort();
                        pending.extend(
                            dependencies
                                .into_iter()
                                .filter(|dep| queued.insert(dep.clone())),
                        );
                        result.mods.push((original, info));
                    }
                    Err(error) => result.errors.push((spec, error)),
                }
            }
        }
        result
    }

    pub async fn resolve_mod(
        &self,
        original_spec: ModSpecification,
        update: bool,
    ) -> Result<(ModSpecification, ModInfo), ProviderError> {
        let mut spec = original_spec.clone();
        loop {
            match self
                .get_provider(&spec.url)?
                .resolve_mod(&spec, update, self.cache.clone())
                .await?
            {
                ModResponse::Resolve(m) => {
                    return Ok((original_spec, m));
                }
                ModResponse::Redirect(redirected_spec) => spec = redirected_spec,
            };
        }
    }

    pub async fn fetch_mods(
        &self,
        mods: &[&ModResolution],
        update: bool,
        tx: Option<Sender<FetchProgress>>,
    ) -> Result<Vec<PathBuf>, ProviderError> {
        use futures::stream::{self, StreamExt, TryStreamExt};

        stream::iter(
            mods.iter()
                .map(|res| self.fetch_mod(res, update, tx.clone())),
        )
        .boxed() // without this the future becomes !Send https://github.com/rust-lang/rust/issues/104382
        .buffer_unordered(5)
        .try_collect::<Vec<_>>()
        .await
    }

    pub async fn fetch_mods_ordered(
        &self,
        mods: &[&ModResolution],
        update: bool,
        tx: Option<Sender<FetchProgress>>,
    ) -> Result<Vec<PathBuf>, ProviderError> {
        use futures::stream::{self, StreamExt, TryStreamExt};

        stream::iter(
            mods.iter()
                .map(|res| self.fetch_mod(res, update, tx.clone())),
        )
        .boxed() // without this the future becomes !Send https://github.com/rust-lang/rust/issues/104382
        .buffered(5)
        .try_collect::<Vec<_>>()
        .await
    }

    pub async fn fetch_mod(
        &self,
        res: &ModResolution,
        update: bool,
        tx: Option<Sender<FetchProgress>>,
    ) -> Result<PathBuf, ProviderError> {
        self.get_provider(&res.url.0)?
            .fetch_mod(
                res,
                update,
                self.cache.clone(),
                &self.blob_cache.clone(),
                tx,
            )
            .await
    }

    pub async fn update_cache(&self) -> Result<(), ProviderError> {
        let providers = self.providers.read().unwrap().clone();
        let mut failures = Vec::new();
        for (name, provider) in providers.iter() {
            info!("updating cache for {name} provider");
            if let Err(error) = provider.update_cache(self.cache.clone()).await {
                failures.push(format!("{name}: {error}"));
            }
        }
        if let Err(error) = self.cache.read().unwrap().save() {
            failures.push(format!("Updated metadata could not be saved: {error}"));
        }
        if !failures.is_empty() {
            failures.sort();
            return Err(ProviderError::PartialUpdate {
                failures: failures.join("\n"),
            });
        }
        Ok(())
    }

    pub fn get_mod_info(&self, spec: &ModSpecification) -> Option<ModInfo> {
        self.get_provider(&spec.url)
            .ok()?
            .get_mod_info(spec, self.cache.clone())
    }

    pub fn is_pinned(&self, spec: &ModSpecification) -> bool {
        self.get_provider(&spec.url)
            .unwrap()
            .is_pinned(spec, self.cache.clone())
    }

    pub fn get_version_name(&self, spec: &ModSpecification) -> Option<String> {
        self.get_provider(&spec.url)
            .unwrap()
            .get_version_name(spec, self.cache.clone())
    }
}
