//! Supported lockfile identity and installed version checks.

use super::publication;
use std::collections::BTreeMap;

pub(super) struct LockVersions(BTreeMap<String, Vec<Option<String>>>);

impl LockVersions {
    pub(super) fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() > publication::MAX_METADATA_BYTES {
            return Err("uv.lock exceeds the metadata byte limit".into());
        }
        let lock: toml::Value =
            toml::from_str(raw).map_err(|error| format!("cannot read uv.lock: {error}"))?;
        if lock.get("version").and_then(toml::Value::as_integer) != Some(1)
            || lock.get("revision").and_then(toml::Value::as_integer) != Some(3)
        {
            return Err("unsupported uv.lock version or revision".into());
        }
        let packages = lock
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("uv.lock has no package inventory")?;
        let mut names = BTreeMap::<String, Vec<Option<String>>>::new();
        for package in packages {
            let name = package
                .get("name")
                .and_then(toml::Value::as_str)
                .and_then(publication::normalized_distribution)
                .ok_or("uv.lock contains an invalid package name")?;
            let supported = package.get("resolution-markers").is_none()
                && package
                    .get("source")
                    .and_then(toml::Value::as_table)
                    .is_some_and(|source| {
                        source.len() == 1
                            && source
                                .get("registry")
                                .and_then(toml::Value::as_str)
                                .is_some()
                    });
            let version = supported
                .then(|| {
                    package
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned)
                })
                .flatten();
            names.entry(name).or_default().push(version);
        }
        Ok(Self(names))
    }

    pub(super) fn verify(&self, distribution: &str, installed: &str) -> Result<(), String> {
        match self.0.get(distribution).map(Vec::as_slice) {
            Some([Some(locked)]) if locked == installed => Ok(()),
            Some([Some(locked)]) => Err(format!(
                "uv.lock version {locked} disagrees with installed {distribution}=={installed}"
            )),
            None => Err(format!("uv.lock has no entry for installed {distribution}")),
            _ => Err(format!(
                "uv.lock entry for {distribution} is ambiguous or conditional"
            )),
        }
    }
}
