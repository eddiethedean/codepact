//! Identity, layout, and digest of the source snapshot belonging to a provider.

use crate::SourceFile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MANIFEST_FILENAME: &str = "codepact.contracts.json";
pub const MARKER_FILENAME: &str = "py.codepact";
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;
/// The selected regular package from an installed distribution, with paths
/// relative to the import root. Hosts establish file ownership and read files.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderPackage {
    pub distribution: String,
    pub version: String,
    pub import_scope: String,
    pub sources: Vec<SourceFile>,
    pub marked: bool,
    pub manifest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFile {
    pub path: String,
    pub sha256: String,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn normalized_distribution(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
    {
        return None;
    }
    let mut output = String::new();
    for byte in bytes {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {
                output.push(byte.to_ascii_lowercase().into())
            }
            b'-' | b'_' | b'.' if !output.ends_with('-') => output.push('-'),
            b'-' | b'_' | b'.' => {}
            _ => return None,
        }
    }
    Some(output)
}

pub fn regular_relative_path(path: &str) -> bool {
    !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|component| !matches!(component, "" | "." | ".."))
}

pub(in crate::foundation) fn version_is_literal(version: &str) -> bool {
    version.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && version.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'!' | b'+' | b'-' | b'_')
        })
}

pub(in crate::foundation) fn qualified(name: &str, target: &str) -> bool {
    !name.is_empty()
        && name
            .split('.')
            .all(|part| crate::is_python_identifier(part, target))
}

fn package_path(scope: &str) -> String {
    scope.replace('.', "/")
}

impl ProviderPackage {
    pub fn marker_path(&self) -> String {
        format!("{}/{}", package_path(&self.import_scope), MARKER_FILENAME)
    }

    pub fn manifest_path(&self) -> String {
        format!("{}/{}", package_path(&self.import_scope), MANIFEST_FILENAME)
    }

    pub(crate) fn validate(&self, target: &str) -> Result<(), String> {
        if normalized_distribution(&self.distribution).as_deref() != Some(&self.distribution)
            || !version_is_literal(&self.version)
            || !qualified(&self.import_scope, target)
        {
            return Err("invalid provider distribution, exact version or import scope".into());
        }
        let mut paths = BTreeSet::new();
        let mut modules = BTreeSet::new();
        let prefix = format!("{}/", package_path(&self.import_scope));
        if self.sources.len() > crate::DEFAULT_MAX_INPUT_FILES
            || self
                .sources
                .iter()
                .map(|file| file.source.len())
                .sum::<usize>()
                > self
                    .reviewed_library_limit()
                    .unwrap_or(crate::DEFAULT_MAX_SOURCE_BYTES)
        {
            return Err("provider source discovery exceeded the file or byte budget".into());
        }
        for file in &self.sources {
            if !regular_relative_path(&file.path)
                || !file.path.starts_with(&prefix)
                || module_for_source(&file.path, target).as_deref() != Some(&file.module)
                || !paths.insert(&file.path)
                || !modules.insert(&file.module)
            {
                return Err(
                    "provider sources contain an invalid or duplicate path/module identity".into(),
                );
            }
        }
        if !paths.contains(&format!("{prefix}__init__.py")) {
            return Err("the provider import scope must be a regular Python package".into());
        }
        for file in &self.sources {
            if file
                .path
                .strip_prefix(&prefix)
                .unwrap()
                .rsplit_once('/')
                .is_some_and(|(subdir, _)| {
                    let mut package = prefix.clone();
                    subdir.split('/').any(|component| {
                        package.push_str(component);
                        package.push('/');
                        !paths.contains(&format!("{package}__init__.py"))
                    })
                })
            {
                return Err("provider source reaches an unsupported namespace package".into());
            }
        }
        Ok(())
    }

    pub(crate) fn reviewed_library_limit(&self) -> Option<usize> {
        (!self.marked && self.manifest.is_none()).then_some(())?;
        crate::foundation::reviewed_library_source_limit(&self.import_scope)
    }

    pub(crate) fn sources_for_analysis(&self) -> Vec<SourceFile> {
        self.sources
            .iter()
            .map(|file| {
                SourceFile::new(
                    format!(
                        "@dependency/{}@{}/{}",
                        self.distribution, self.version, file.path
                    ),
                    &file.module,
                    &file.source,
                )
            })
            .collect()
    }

    pub(super) fn bindings(&self) -> Vec<ArtifactFile> {
        let mut files = self
            .sources
            .iter()
            .map(|file| ArtifactFile {
                path: file.path.clone(),
                sha256: sha256(file.source.as_bytes()),
            })
            .collect::<Vec<_>>();
        if self.marked {
            files.push(ArtifactFile {
                path: self.marker_path(),
                sha256: sha256(b""),
            });
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        files
    }

    pub(crate) fn digest(&self) -> String {
        sha256(
            &serde_json::to_vec(&(
                &self.distribution,
                &self.version,
                &self.import_scope,
                self.bindings(),
                &self.manifest,
            ))
            .expect("provider digest inputs are JSON values"),
        )
    }
}

pub fn module_for_source(path: &str, target: &str) -> Option<String> {
    if !regular_relative_path(path) {
        return None;
    }
    let without_suffix = path.strip_suffix(".py")?;
    let path = without_suffix
        .strip_suffix("/__init__")
        .unwrap_or(without_suffix);
    path.split('/')
        .all(|name| crate::is_python_identifier(name, target))
        .then(|| path.replace('/', "."))
}
