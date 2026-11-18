//! Applicability of an exact-version project model for a selected provider.

use super::publication::PublishedSymbol;
use super::{
    lock::LockVersions, provider_location, publication::SelectedPublication,
    validate_project_models, FoundationInputs, ProjectResourceModel, ProviderOrigin,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ApplicableFactory {
    pub model: ProjectResourceModel,
    pub origin: ProviderOrigin,
}

pub(super) struct ModelApplicability {
    lock: Option<Result<LockVersions, String>>,
    validation: Result<(), String>,
}

impl ModelApplicability {
    pub(super) fn new(inputs: &FoundationInputs, target: &str) -> Self {
        Self {
            lock: inputs.uv_lock.as_deref().map(LockVersions::parse),
            validation: validate_project_models(&inputs.project_models, target),
        }
    }

    /// An agreeing publication remains the chosen source of effects unless
    /// the project explicitly requests an override.
    pub(super) fn select_factory(
        &self,
        model: &ProjectResourceModel,
        selected: &SelectedPublication,
        publication: Option<&PublishedSymbol>,
    ) -> Result<Option<ApplicableFactory>, String> {
        self.validation.as_ref().map_err(Clone::clone)?;
        let package = &selected.package;
        if model.distribution != package.distribution || model.version != package.version {
            return Err(format!(
                "project model expects {}=={} but the selected provider is {}=={}",
                model.distribution, model.version, package.distribution, package.version
            ));
        }
        if let Some(lock) = &self.lock {
            lock.as_ref()
                .map_err(Clone::clone)?
                .verify(&package.distribution, &package.version)?;
        }
        let definition = selected.source_exports.get(&model.factory)
            .filter(|definition| definition.symbol_kind == "function")
            .ok_or("project model factory does not resolve to a synchronous public function in this artifact")?;
        let class = selected.source_exports.values().any(|definition| {
            definition.canonical_name == model.resource && definition.symbol_kind == "resource"
        });
        if !class {
            return Err(
                "project model resource does not resolve to a class in this artifact".into(),
            );
        }
        if !selected
            .cleanup_methods
            .get(&model.resource)
            .is_some_and(|methods| methods.contains(&model.close_method))
        {
            return Err(format!("project model cleanup method {}.{} does not resolve to a supported synchronous instance method", model.resource, model.close_method));
        }
        let existing = publication.and_then(|symbol| {
            symbol
                .contracts
                .iter()
                .find(|fact| fact.kind == "owned-return")
        });
        match (existing, model.override_publication) {
            (Some(fact), false) => {
                if fact.resource.as_ref() != Some(&model.resource)
                    || fact.close_method.as_ref() != Some(&model.close_method)
                {
                    return Err("project model conflicts with the published factory effect".into());
                }
                return Ok(None);
            }
            (None, true) => {
                return Err(
                    "project model override has no applicable published factory effect to replace"
                        .into(),
                );
            }
            _ => {}
        }
        Ok(Some(ApplicableFactory {
            model: model.clone(),
            origin: ProviderOrigin {
                public_symbol: model.factory.clone(),
                canonical_symbol: definition.canonical_name.clone(),
                distribution: package.distribution.clone(),
                version: package.version.clone(),
                artifact_sha256: selected.digest.clone(),
                location: existing.map(|fact| provider_location(package, &fact.evidence)),
                replaced_origin: existing.map(|_| selected.origin.to_owned()),
            },
        }))
    }
}
