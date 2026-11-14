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

    Ok(LoadedInputs {
        files,
        physical_files,
    })
}
