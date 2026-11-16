use super::discovery::{discover_python_files, module_from_relative_path};
use codepact::{adoption, is_python_identifier, SourceFile, DEFAULT_MAX_SOURCE_BYTES};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

pub(super) struct LoadedInputs {
    pub(super) files: Vec<SourceFile>,
    pub(super) physical_files: BTreeSet<PathBuf>,
    pub(super) provider_distributions: BTreeSet<String>,
}

struct ProviderSource {
    path: PathBuf,
    module: String,
    logical_path: String,
    distribution: String,
}

pub(super) fn normalized_distribution(value: &str) -> Option<String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return None;
    }
    let mut normalized = String::new();
    let mut separator = false;
    for character in value.chars() {
        if matches!(character, '.' | '_' | '-') {
            if !separator {
                normalized.push('-');
            }
            separator = true;
        } else {
            normalized.push(character.to_ascii_lowercase());
            separator = false;
        }
    }
    Some(normalized)
}

fn csv_first_field(line: &str) -> Option<String> {
    if let Some(mut rest) = line.strip_prefix('"') {
        let mut output = String::new();
        loop {
            let character = rest.chars().next()?;
            rest = &rest[character.len_utf8()..];
            if character == '"' {
                if let Some(after_quote) = rest.strip_prefix('"') {
                    output.push('"');
                    rest = after_quote;
                } else {
                    return rest.starts_with(',').then_some(output);
                }
            } else {
                output.push(character);
            }
        }
    } else {
        Some(line.split(',').next()?.to_owned())
    }
}

fn owner_for_provider_files(
    root: &Path,
    marker: &Path,
    source: &Path,
) -> Result<Option<(String, String)>, String> {
    let marker_relative = marker
        .strip_prefix(root)
        .map_err(|_| "provider marker is outside its dependency root".to_owned())?
        .to_string_lossy()
        .replace('\\', "/");
    let source_relative = source
        .strip_prefix(root)
        .map_err(|_| "provider source is outside its dependency root".to_owned())?
        .to_string_lossy()
        .replace('\\', "/");
    let entries = fs::read_dir(root)
        .map_err(|error| format!("cannot inspect dependency root {}: {error}", root.display()))?;
    let mut owners = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let info = entry.path();
        if !info
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".dist-info"))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&info)
            .map_err(|error| format!("cannot inspect {}: {error}", info.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let record_path = info.join("RECORD");
        let record_metadata = match fs::symlink_metadata(&record_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("cannot inspect {}: {error}", record_path.display())),
        };
        if record_metadata.file_type().is_symlink() || !record_metadata.is_file() {
            continue;
        }
        if record_metadata.len() > 16 * 1024 * 1024 {
            return Err(format!(
                "installed package record is too large: {}",
                record_path.display()
            ));
        }
        let record = fs::read_to_string(&record_path)
            .map_err(|error| format!("cannot read {}: {error}", record_path.display()))?;
        let mut owns_marker = false;
        let mut owns_source = false;
        for line in record.lines() {
            let Some(path) = csv_first_field(line) else {
                continue;
            };
            owns_marker |= path == marker_relative;
            owns_source |= path == source_relative;
        }
        if !owns_marker {
            continue;
        }
        if !owns_source {
            return Err(format!(
                "provider source {} is not listed in the owner RECORD",
                source_relative
            ));
        }
        let metadata_path = info.join("METADATA");
        let file_metadata = fs::symlink_metadata(&metadata_path)
            .map_err(|error| format!("cannot inspect {}: {error}", metadata_path.display()))?;
        if file_metadata.file_type().is_symlink()
            || !file_metadata.is_file()
            || file_metadata.len() > 64 * 1024
        {
            return Err(format!(
                "invalid installed package metadata: {}",
                metadata_path.display()
            ));
        }
        let metadata = fs::read_to_string(&metadata_path)
            .map_err(|error| format!("cannot read {}: {error}", metadata_path.display()))?;
        let name = metadata
            .lines()
            .find_map(|line| line.strip_prefix("Name: "));
        let version = metadata
            .lines()
            .find_map(|line| line.strip_prefix("Version: "));
        let (Some(name), Some(version)) = (name, version) else {
            return Err(format!(
                "installed package metadata is missing Name or Version: {}",
                metadata_path.display()
            ));
        };
        let name = normalized_distribution(name)
            .ok_or_else(|| format!("invalid distribution name in {}", metadata_path.display()))?;
        let wheel_path = info.join("WHEEL");
        let wheel_file = fs::symlink_metadata(&wheel_path)
            .map_err(|error| format!("cannot inspect {}: {error}", wheel_path.display()))?;
        if wheel_file.file_type().is_symlink()
            || !wheel_file.is_file()
            || wheel_file.len() > 64 * 1024
        {
            return Err(format!("invalid wheel metadata: {}", wheel_path.display()));
        }
        let wheel = fs::read_to_string(&wheel_path)
            .map_err(|error| format!("cannot read {}: {error}", wheel_path.display()))?;
        let mut purelib = None;
        let mut tags = Vec::new();
        for line in wheel.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                if key.eq_ignore_ascii_case("Root-Is-Purelib") {
                    purelib = Some(value.eq_ignore_ascii_case("true"));
                } else if key.eq_ignore_ascii_case("Tag") {
                    tags.push(value);
                }
            }
        }
        let pure_python = purelib == Some(true)
            && !tags.is_empty()
            && !tags.iter().any(|tag| {
                let mut components = tag.split('-');
                components.next().is_none()
                    || components.next() != Some("none")
                    || components.next() != Some("any")
                    || components.next().is_some()
            });
        owners.push((name, version.to_owned(), pure_python));
    }
    owners.sort();
    match owners.as_slice() {
        [(name, version, true)] => Ok(Some((name.clone(), version.clone()))),
        [(_, _, false)] => Ok(None),
        [] => Err(format!(
            "cannot establish installed distribution ownership for marker {}",
            marker.display()
        )),
        _ => Err(format!(
            "multiple installed distributions claim marker {}",
            marker.display()
        )),
    }
}

fn provider_marker(source: &Path, root: &Path) -> Option<PathBuf> {
    let mut directory = source.parent()?;
    loop {
        if !directory.starts_with(root) {
            return None;
        }
        let initializer = directory.join("__init__.py");
        let marker = directory.join("py.codepact");
        let init_metadata = fs::symlink_metadata(&initializer).ok()?;
        if init_metadata.file_type().is_symlink() || !init_metadata.is_file() {
            return None;
        }
        if let Ok(metadata) = fs::symlink_metadata(&marker) {
            if !metadata.file_type().is_symlink() && metadata.is_file() {
                return Some(marker);
            }
        }
        if directory == root {
            return None;
        }
        directory = directory.parent()?;
    }
}

fn first_level_imports(root: &Path, python_version: &str) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    let entries = fs::read_dir(root)
        .map_err(|error| format!("cannot inspect dependency root {}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            let candidate = if metadata.is_dir() {
                // A namespace portion does not stop the search for a regular package.
                let initializer = path.join("__init__.py");
                match fs::symlink_metadata(&initializer) {
                    Ok(metadata) if metadata.is_file() => Some(name.to_owned()),
                    Ok(_) => None,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(format!("cannot inspect {}: {error}", initializer.display()));
                    }
                }
            } else if metadata.is_file() && name.ends_with(".py") {
                name.strip_suffix(".py").map(str::to_owned)
            } else {
                None
            };
            if candidate
                .as_deref()
                .is_some_and(|name| codepact::is_python_identifier(name, python_version))
            {
                names.insert(candidate.expect("checked name"));
            }
        }
    }
    Ok(names)
}

fn discover_marked_provider_sources(
    roots: &[PathBuf],
    python_version: &str,
    project_modules: &BTreeSet<String>,
) -> Result<Vec<ProviderSource>, String> {
    let mut shadowed_top_levels = project_modules
        .iter()
        .filter_map(|module| module.split('.').next().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let mut selected_modules = BTreeSet::new();
    let mut providers = Vec::new();
    for root in roots {
        adoption::reject_symlink_components(root, "dependency root")?;
        let root = fs::canonicalize(root).map_err(|error| {
            format!("cannot resolve dependency root {}: {error}", root.display())
        })?;
        let mut paths = Vec::new();
        discover_python_files(
            &root,
            &mut paths,
            codepact::DEFAULT_MAX_INPUT_FILES,
            &|_| true,
        )?;
        for path in paths {
            let Some(marker) = provider_marker(&path, &root) else {
                continue;
            };
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| "provider file is outside its dependency root".to_owned())?;
            let module = module_from_relative_path(relative, python_version)?;
            let top_level = module.split('.').next().unwrap_or_default();
            if shadowed_top_levels.contains(top_level) || !selected_modules.insert(module.clone()) {
                continue;
            }
            let ownership = match owner_for_provider_files(&root, &marker, &path) {
                Ok(ownership) => ownership,
                // Unresolved publications are relevant only if a project
                // import reaches them. The semantic resolver reports that
                // missing contract at the caller.
                Err(_) => continue,
            };
            let Some((distribution, version)) = ownership else {
                continue;
            };
            let relative_path = relative.to_string_lossy().replace('\\', "/");
            providers.push(ProviderSource {
                path,
                module,
                logical_path: format!("@dependency/{distribution}@{version}/{relative_path}"),
                distribution,
            });
        }
        shadowed_top_levels.extend(first_level_imports(&root, python_version)?);
    }
    Ok(providers)
}

pub(super) fn resolve_root(
    effective: Option<&adoption::EffectiveConfig>,
    explicit_root: Option<PathBuf>,
    inspecting: bool,
    cwd: &Path,
) -> Result<Option<PathBuf>, String> {
    if let Some(config) = effective {
        return super::discovery::resolve_project_root(&config.root).map(Some);
    }
    if let Some(root) = explicit_root {
        let root = if root.is_absolute() {
            root
        } else {
            cwd.join(root)
        };
        adoption::reject_symlink_components(&root, "project root")?;
        return fs::canonicalize(&root)
            .map(Some)
            .map_err(|error| format!("cannot resolve module root {}: {error}", root.display()));
    }
    Ok((!inspecting).then(|| cwd.to_path_buf()))
}

pub(super) fn collect_files(
    mut paths: Vec<PathBuf>,
    root: Option<&Path>,
    python_version: &str,
    checking: bool,
    inspecting: bool,
    effective: Option<&adoption::EffectiveConfig>,
    cwd: &Path,
) -> Result<LoadedInputs, String> {
    if inspecting && paths.is_empty() {
        return Err("inspect-contracts requires at least one file".into());
    }
    if checking {
        let project_root = root.expect("checks resolve an effective project root");
        let mut discovered = Vec::new();
        discover_python_files(
            project_root,
            &mut discovered,
            codepact::DEFAULT_MAX_INPUT_FILES,
            &|path| effective.is_none_or(|config| adoption::included(config, project_root, path)),
        )?;
        if paths.is_empty() && discovered.is_empty() {
            return Err(format!(
                "check found no Python files selected under {}",
                project_root.display()
            ));
        }
        paths.extend(discovered);
        paths.sort();
        paths.dedup();
    }

    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    let mut modules = BTreeSet::new();
    let mut physical_files = BTreeSet::new();
    let mut provider_distributions = BTreeSet::new();
    for path in paths {
        let physical_path = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        if checking {
            adoption::reject_symlink_components(&physical_path, "source input")?;
        }
        let canonical_path = fs::canonicalize(&physical_path)
            .map_err(|error| format!("cannot read {}: {error}", physical_path.display()))?;
        if physical_files.contains(&canonical_path) {
            continue;
        }
        if physical_files.len() >= codepact::DEFAULT_MAX_INPUT_FILES {
            return Err("project input exceeds the 10,000 file limit".into());
        }
        physical_files.insert(canonical_path.clone());
        let logical_path = match root {
            Some(root) => canonical_path
                .strip_prefix(root)
                .map(Path::to_path_buf)
                .map_err(|_| {
                    format!(
                        "file {} is outside module root {}",
                        canonical_path.display(),
                        root.display()
                    )
                })?,
            None => canonical_path
                .file_name()
                .map(PathBuf::from)
                .ok_or_else(|| "cannot derive module name from source path".to_owned())?,
        };
        if checking
            && effective.is_some_and(|config| {
                !adoption::included(config, root.expect("check root exists"), &canonical_path)
            })
        {
            return Err(format!(
                "explicit input {} is excluded or skipped by project configuration",
                logical_path.display()
            ));
        }
        let module = match root {
            Some(_) => module_from_relative_path(&logical_path, python_version)?,
            None => logical_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .filter(|stem| is_python_identifier(stem, python_version))
                .ok_or_else(|| {
                    format!("cannot derive module name from {}", logical_path.display())
                })?,
        };
        if !modules.insert(module.clone()) {
            return Err(format!(
                "multiple files map to module {module}; provide a suitable --root"
            ));
        }
        let metadata = fs::metadata(&canonical_path)
            .map_err(|error| format!("cannot inspect {}: {error}", canonical_path.display()))?;
        let remaining = DEFAULT_MAX_SOURCE_BYTES.saturating_sub(total_bytes);
        if metadata.len() > remaining as u64 {
            return Err(format!(
                "source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded before reading {}",
                canonical_path.display()
            ));
        }
        let mut source = String::new();
        let mut input = File::open(&canonical_path)
            .map_err(|error| format!("cannot read {}: {error}", canonical_path.display()))?
            .take(remaining as u64 + 1);
        input.read_to_string(&mut source).map_err(|error| {
            format!(
                "cannot read UTF-8 Python source {}: {error}",
                canonical_path.display()
            )
        })?;
        if source.len() > remaining {
            return Err(format!(
                "source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded while reading {}",
                canonical_path.display()
            ));
        }
        total_bytes += source.len();
        files.push(SourceFile::new(
            logical_path.to_string_lossy().replace('\\', "/"),
            module,
            source,
        ));
    }

    if checking {
        if let Some(config) = effective {
            let project_modules = modules.clone();
            let providers = discover_marked_provider_sources(
                &config.dependency_roots,
                python_version,
                &project_modules,
            )?;
            for provider in providers {
                if modules.contains(&provider.module) {
                    continue;
                }
                if physical_files.len() >= codepact::DEFAULT_MAX_INPUT_FILES {
                    return Err("project and provider inputs exceed the 10,000 file limit".into());
                }
                adoption::reject_symlink_components(&provider.path, "provider source")?;
                let file_metadata = fs::symlink_metadata(&provider.path).map_err(|error| {
                    format!(
                        "cannot inspect provider source {}: {error}",
                        provider.path.display()
                    )
                })?;
                if file_metadata.file_type().is_symlink() || !file_metadata.is_file() {
                    return Err(format!(
                        "provider source is not a regular file: {}",
                        provider.path.display()
                    ));
                }
                let remaining = DEFAULT_MAX_SOURCE_BYTES.saturating_sub(total_bytes);
                if file_metadata.len() > remaining as u64 {
                    return Err(format!(
                        "project and provider source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded before reading {}",
                        provider.path.display()
                    ));
                }
                let mut source = String::new();
                File::open(&provider.path)
                    .map_err(|error| format!("cannot read provider source: {error}"))?
                    .take(remaining as u64 + 1)
                    .read_to_string(&mut source)
                    .map_err(|error| {
                        format!(
                            "cannot read UTF-8 provider source {}: {error}",
                            provider.path.display()
                        )
                    })?;
                if source.len() > remaining {
                    return Err(format!(
                        "project and provider source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded while reading {}",
                        provider.path.display()
                    ));
                }
                total_bytes += source.len();
                modules.insert(provider.module.clone());
                physical_files.insert(provider.path.clone());
                provider_distributions.insert(provider.distribution.clone());
                files.push(SourceFile::new(
                    provider.logical_path,
                    provider.module,
                    source,
                ));
            }
        }
    }

    Ok(LoadedInputs {
        files,
        physical_files,
        provider_distributions,
    })
}
