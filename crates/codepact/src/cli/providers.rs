use super::{artifacts, discovery};
use codepact::foundation::{self, ProviderIssue, ProviderPackage, MAX_METADATA_BYTES};
use codepact::{adoption, SourceFile};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const MAX_INVENTORY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Discovered {
    pub(super) packages: Vec<ProviderPackage>,
    pub(super) issues: Vec<ProviderIssue>,
    pub(super) files: BTreeSet<PathBuf>,
}

enum Candidate {
    Package(PathBuf),
    Module,
    Opaque,
    Namespace,
}

fn directory_entries(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(root).map_err(|error| {
        format!(
            "cannot inspect dependency directory {}: {error}",
            root.display()
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries.take(discovery::MAX_DIRECTORY_ENTRIES + 1) {
        paths.push(entry.map_err(|error| error.to_string())?.path());
    }
    if paths.len() > discovery::MAX_DIRECTORY_ENTRIES {
        return Err("dependency root directory-entry budget exceeded".into());
    }
    paths.sort();
    Ok(paths)
}

fn native_member(paths: &[PathBuf], stem: &str) -> bool {
    paths.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.strip_prefix(stem).is_some_and(|suffix| {
                    suffix.starts_with('.') && (suffix.ends_with(".so") || suffix.ends_with(".pyd"))
                })
            })
    })
}

fn candidate(root: &Path, entries: &[PathBuf], name: &str) -> Result<Option<Candidate>, String> {
    let directory = root.join(name);
    let module = root.join(format!("{name}.py"));
    let mut namespace = false;
    if let Ok(metadata) = fs::symlink_metadata(&directory) {
        if metadata.is_symlink() {
            return Ok(Some(Candidate::Opaque));
        }
        if metadata.is_dir() {
            let initializer = directory.join("__init__.py");
            if native_member(&directory_entries(&directory)?, "__init__") {
                return Ok(Some(Candidate::Opaque));
            }
            match fs::symlink_metadata(initializer) {
                Ok(metadata) if metadata.is_file() => {
                    return Ok(Some(Candidate::Package(directory)))
                }
                Ok(_) => return Ok(Some(Candidate::Opaque)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => namespace = true,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    if native_member(entries, name) {
        return Ok(Some(Candidate::Opaque));
    }
    if let Ok(metadata) = fs::symlink_metadata(module) {
        return Ok(Some(if metadata.is_file() {
            Candidate::Module
        } else {
            Candidate::Opaque
        }));
    }
    Ok(namespace.then_some(Candidate::Namespace))
}

#[derive(Default)]
struct Inventory {
    records: Vec<(PathBuf, artifacts::Record)>,
}

impl Inventory {
    fn read(entries: &[PathBuf]) -> Result<Self, String> {
        let mut records = Vec::new();
        let mut remaining = MAX_INVENTORY_BYTES;
        for directory in entries.iter().filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".dist-info"))
        }) {
            let path = directory.join("RECORD");
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("cannot inspect installed RECORD: {error}")),
                Ok(_) => {}
            }
            let raw = artifacts::read_regular(&path, remaining.min(artifacts::MAX_RECORD_BYTES))
                .map_err(|error| {
                    format!(
                        "installed RECORD discovery failed or exceeded its byte budget: {error}"
                    )
                })?;
            remaining -= raw.len();
            records.push((
                directory.clone(),
                artifacts::read_record(&raw, artifacts::RecordLayout::Installed)?,
            ));
        }
        Ok(Self { records })
    }

    fn owner(&self, name: &str) -> Result<(&Path, &artifacts::Record), String> {
        let owners = self
            .records
            .iter()
            .filter(|(_, record)| record.contains(name))
            .collect::<Vec<_>>();
        let [(directory, record)] = owners.as_slice() else {
            let reason = if owners.is_empty() {
                "no installed distribution owns"
            } else {
                "multiple installed distributions claim"
            };
            return Err(format!("{reason} {name}"));
        };
        Ok((directory, record))
    }
}

fn installed_package(
    root: &Path,
    package_root: &Path,
    scope: &str,
    inventory: &Inventory,
    target: &str,
    expected_unmarked: bool,
) -> Result<Option<(ProviderPackage, BTreeSet<PathBuf>)>, String> {
    let marker = package_root.join(foundation::MARKER_FILENAME);
    let manifest_path = package_root.join(foundation::MANIFEST_FILENAME);
    let marked = match fs::symlink_metadata(&marker) {
        Ok(_) if artifacts::read_regular(&marker, 1)?.is_empty() => true,
        Ok(_) => return Err("installed py.codepact marker must be empty".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.to_string()),
    };
    let has_manifest = match fs::symlink_metadata(&manifest_path) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.to_string()),
    };
    if !marked && has_manifest {
        return Err("installed manifest is missing its py.codepact marker".into());
    }
    if !marked && !expected_unmarked {
        return Ok(None);
    }
    let (info, record) = inventory.owner(&format!("{scope}/__init__.py"))?;
    if !has_manifest && record.contains(&format!("{scope}/{}", foundation::MANIFEST_FILENAME)) {
        return Err("installed contract manifest listed in RECORD is missing".into());
    }
    let mut files = BTreeSet::from([
        info.join("METADATA"),
        info.join("WHEEL"),
        info.join("RECORD"),
    ]);
    let metadata = artifacts::read_text(&info.join("METADATA"), 64 * 1024)?;
    let wheel = artifacts::read_text(&info.join("WHEEL"), 64 * 1024)?;
    if expected_unmarked {
        let directory = info
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid .dist-info path")?;
        record.verify_member(&format!("{directory}/METADATA"), metadata.as_bytes())?;
        record.verify_member(&format!("{directory}/WHEEL"), wheel.as_bytes())?;
    }
    let (distribution, version) = artifacts::metadata_identity(&metadata)?;
    if !artifacts::pure_wheel(&wheel) {
        return Err("selected provider is outside the regular pure-Python wheel profile".into());
    }
    let direct_url = info.join("direct_url.json");
    if fs::symlink_metadata(&direct_url).is_ok() {
        let metadata: serde_json::Value =
            serde_json::from_str(&artifacts::read_text(&direct_url, 64 * 1024)?)
                .map_err(|error| format!("invalid installed direct_url.json: {error}"))?;
        if metadata["dir_info"]["editable"] == true {
            return Err("editable install is outside the provider profile".into());
        }
        files.insert(direct_url);
    }
    let mut package = ProviderPackage {
        distribution,
        version,
        import_scope: scope.into(),
        sources: Vec::new(),
        marked,
        manifest: None,
    };
    for (exists, path, relative) in [
        (marked, marker, package.marker_path()),
        (has_manifest, manifest_path.clone(), package.manifest_path()),
    ] {
        if exists {
            if inventory.owner(&relative)?.0 != info {
                return Err(format!("installed contract metadata {relative} has a different owner from the selected source"));
            }
            files.insert(path);
        }
    }
    if has_manifest {
        let content = artifacts::read_text(&manifest_path, MAX_METADATA_BYTES)?;
        record.verify_member(&package.marker_path(), b"")?;
        record.verify_member(&package.manifest_path(), content.as_bytes())?;
        package.manifest = Some(content);
    }
    let paths = discovery::discover_provider_sources(package_root)
        .map_err(|error| format!("provider file discovery: {error}"))?;
    let mut remaining = if !marked && !has_manifest && expected_unmarked {
        foundation::reviewed_library_source_limit(scope)
            .unwrap_or(codepact::DEFAULT_MAX_SOURCE_BYTES)
    } else {
        codepact::DEFAULT_MAX_SOURCE_BYTES
    };
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "provider source is outside the selected root")?
            .to_str()
            .ok_or("provider source path is not UTF-8")?
            .replace('\\', "/");
        let module = foundation::module_for_source(&relative, target);
        if module.is_none()
            && scope == "etlantic_sqlmodel"
            && !marked
            && expected_unmarked
            && relative.starts_with("etlantic_sqlmodel/migrations/versions/")
            && relative
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with(|ch: char| ch.is_ascii_digit()))
        {
            let source = artifacts::read_text(&path, remaining)?;
            if inventory.owner(&relative)?.0 != info {
                return Err("ETLantic migration has a different installed owner".into());
            }
            record.verify_member(&relative, source.as_bytes())?;
            remaining -= source.len();
            files.insert(path);
            continue;
        }
        let module = module.ok_or_else(|| {
            format!("provider source {relative} has an unresolved module identity")
        })?;
        if !record.contains(&relative) || inventory.owner(&relative)?.0 != info {
            return Err(format!(
                "selected provider does not uniquely own source file {relative}"
            ));
        }
        let source = artifacts::read_text(&path, remaining)?;
        if expected_unmarked {
            record.verify_member(&relative, source.as_bytes())?;
        }
        remaining -= source.len();
        files.insert(path);
        package
            .sources
            .push(SourceFile::new(relative, module, source));
    }
    if expected_unmarked {
        let prefix = format!("{scope}/");
        if let Some(path) = record.paths().find(|path| {
            path.starts_with(&prefix) && path.ends_with(".py") && !files.contains(&root.join(path))
        }) {
            return Err(format!(
                "installed source file {path} listed in RECORD is missing"
            ));
        }
    }
    Ok(Some((package, files)))
}

enum ResolvedImport<'a> {
    Package {
        root_index: usize,
        root: &'a Path,
        entries: &'a [PathBuf],
        directory: PathBuf,
    },
    SingleModule,
    Opaque,
    Namespace,
    Missing,
}

/// Select the first importable identity before considering its metadata.
fn resolve_import<'a>(
    roots: &'a [PathBuf],
    root_entries: &'a [Result<Vec<PathBuf>, String>],
    name: &str,
) -> Result<ResolvedImport<'a>, String> {
    let mut namespace = false;
    for (root_index, (root, entries)) in roots.iter().zip(root_entries).enumerate() {
        let entries = entries.as_ref().map_err(Clone::clone)?;
        match candidate(root, entries, name)? {
            None => continue,
            Some(Candidate::Namespace) => namespace = true,
            Some(Candidate::Package(directory)) => {
                return Ok(ResolvedImport::Package {
                    root_index,
                    root,
                    entries,
                    directory,
                });
            }
            Some(Candidate::Module) => return Ok(ResolvedImport::SingleModule),
            Some(Candidate::Opaque) => return Ok(ResolvedImport::Opaque),
        }
    }
    Ok(if namespace {
        ResolvedImport::Namespace
    } else {
        ResolvedImport::Missing
    })
}

pub(super) fn discover(
    roots: &[PathBuf],
    project_root: &Path,
    files: &[SourceFile],
    target: &str,
    models: &[foundation::ProjectResourceModel],
    reviewed_libraries: bool,
) -> Discovered {
    if roots.is_empty() && models.is_empty() {
        return Discovered::default();
    }
    let mut needed = files
        .iter()
        .flat_map(|file| foundation::static_imports(file, target))
        .filter_map(|module| module.split('.').next().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let direct = needed.clone();
    if reviewed_libraries {
        needed.extend(
            direct
                .iter()
                .flat_map(|scope| foundation::reviewed_library_dependencies(scope))
                .map(|scope| (*scope).to_owned()),
        );
    }
    let project_modules = files
        .iter()
        .filter_map(|file| file.module.split('.').next())
        .collect::<BTreeSet<_>>();
    let mut result = Discovered::default();
    let project_entries = directory_entries(project_root).unwrap_or_default();
    let root_entries = roots
        .iter()
        .map(|path| directory_entries(path))
        .collect::<Vec<_>>();
    let mut inventories = BTreeMap::<usize, Result<Inventory, String>>::new();
    for name in needed {
        if project_modules.contains(name.as_str())
            || candidate(project_root, &project_entries, &name)
                .ok()
                .flatten()
                .is_some_and(|candidate| !matches!(candidate, Candidate::Namespace))
        {
            continue;
        }
        let reviewed =
            reviewed_libraries && foundation::reviewed_library_source_limit(&name).is_some();
        let expected = models.iter().any(|model| {
            model
                .factory
                .strip_prefix(&name)
                .is_some_and(|remainder| remainder.starts_with('.'))
        });
        let outcome = (|| match resolve_import(roots, &root_entries, &name)? {
            ResolvedImport::Package {
                root_index,
                root,
                entries,
                directory,
            } => {
                let has_metadata = [foundation::MARKER_FILENAME, foundation::MANIFEST_FILENAME]
                    .iter()
                    .any(|filename| fs::symlink_metadata(directory.join(filename)).is_ok());
                if !has_metadata && !expected && !reviewed {
                    return Ok(None);
                }
                let inventory = inventories
                    .entry(root_index)
                    .or_insert_with(|| Inventory::read(entries))
                    .as_ref()
                    .map_err(Clone::clone)?;
                installed_package(
                    root,
                    &directory,
                    &name,
                    inventory,
                    target,
                    expected || reviewed,
                )
            }
            ResolvedImport::SingleModule if expected || (reviewed && direct.contains(&name)) => {
                Err("selected single-module provider is outside the regular package model".into())
            }
            ResolvedImport::Opaque => {
                Err("selected provider has an unsupported native, symbolic or opaque layout".into())
            }
            ResolvedImport::Namespace => {
                Err("selected namespace provider is outside the regular package profile".into())
            }
            ResolvedImport::Missing if expected => {
                Err("project contract has no selected installed provider".into())
            }
            ResolvedImport::Missing if reviewed && direct.contains(&name) => {
                Err("reviewed library has no selected installed provider".into())
            }
            ResolvedImport::SingleModule | ResolvedImport::Missing => Ok(None),
        })();
        match outcome {
            Ok(Some((package, files))) => {
                result.packages.push(package);
                result.files.extend(files);
            }
            Ok(None) => {}
            Err(reason) => result.issues.push(ProviderIssue {
                import_scope: name,
                reason,
            }),
        }
    }
    result
}

pub(super) struct ProjectInputs {
    pub(super) models: Vec<foundation::ProjectResourceModel>,
    pub(super) lock: Option<String>,
    pub(super) files: BTreeSet<PathBuf>,
}

pub(super) fn project_inputs(
    config: &adoption::EffectiveConfig,
    target: &str,
) -> Result<ProjectInputs, String> {
    let mut files = BTreeSet::new();
    let mut models = Vec::new();
    let mut remaining = MAX_METADATA_BYTES;
    for path in &config.project_contracts {
        let raw = artifacts::read_text(path, remaining)?;
        remaining -= raw.len();
        models.extend(
            foundation::parse_project_models(&raw, target)
                .map_err(|error| format!("{}: {error}", path.display()))?,
        );
        files.insert(path.clone());
    }
    foundation::validate_project_models(&models, target)?;
    let lock = config
        .uv_lock
        .as_ref()
        .map(|path| {
            artifacts::read_text(path, MAX_METADATA_BYTES).inspect(|_| {
                files.insert(path.clone());
            })
        })
        .transpose()?;
    Ok(ProjectInputs {
        models,
        lock,
        files,
    })
}
