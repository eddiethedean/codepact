use std::fs;
use std::path::{Path, PathBuf};

pub(super) const MAX_DISCOVERY_ENTRIES: usize = 100_000;
pub(super) const MAX_DIRECTORY_ENTRIES: usize = 20_000;
pub(super) const MAX_DISCOVERY_DEPTH: usize = 256;

#[derive(Clone, Copy)]
pub(super) struct DiscoveryLimits {
    pub(super) entries: usize,
    pub(super) entries_per_directory: usize,
    pub(super) depth: usize,
}

const DEFAULT_DISCOVERY_LIMITS: DiscoveryLimits = DiscoveryLimits {
    entries: MAX_DISCOVERY_ENTRIES,
    entries_per_directory: MAX_DIRECTORY_ENTRIES,
    depth: MAX_DISCOVERY_DEPTH,
};

pub(super) fn discover_python_files(
    root: &Path,
    output: &mut Vec<PathBuf>,
    budget: usize,
    selected: &impl Fn(&Path) -> bool,
) -> Result<(), String> {
    discover_python_files_with_limits(root, output, budget, selected, DEFAULT_DISCOVERY_LIMITS)
}

pub(super) fn discover_python_files_with_limits(
    root: &Path,
    output: &mut Vec<PathBuf>,
    budget: usize,
    selected: &impl Fn(&Path) -> bool,
    limits: DiscoveryLimits,
) -> Result<(), String> {
    let mut visited_entries = 0;
    discover_python_files_inner(
        root,
        output,
        budget,
        selected,
        limits,
        &mut visited_entries,
        0,
    )
}

fn discover_python_files_inner(
    root: &Path,
    output: &mut Vec<PathBuf>,
    budget: usize,
    selected: &impl Fn(&Path) -> bool,
    limits: DiscoveryLimits,
    visited_entries: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > limits.depth {
        return Err(format!(
            "project discovery exceeds the {} directory-depth limit",
            limits.depth
        ));
    }
    let entries = fs::read_dir(root)
        .map_err(|error| format!("cannot read project directory {}: {error}", root.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        if *visited_entries >= limits.entries {
            return Err(format!(
                "project discovery exceeds the {} directory-entry traversal limit",
                limits.entries
            ));
        }
        if paths.len() >= limits.entries_per_directory {
            return Err(format!(
                "project directory {} exceeds the {} entry limit",
                root.display(),
                limits.entries_per_directory
            ));
        }
        let entry = entry.map_err(|error| error.to_string())?;
        *visited_entries += 1;
        paths.push(entry.path());
    }
    paths.sort();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir()
            && !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".git" | ".venv" | "venv" | "target" | "build" | "dist" | "__pycache__")
            )
        {
            discover_python_files_inner(
                &path,
                output,
                budget,
                selected,
                limits,
                visited_entries,
                depth + 1,
            )?;
        } else if metadata.is_file()
            && path.extension().is_some_and(|extension| extension == "py")
            && selected(&path)
        {
            if output.len() >= budget {
                return Err(format!(
                    "project discovery exceeds the {budget} selected Python file limit"
                ));
            }
            output.push(path);
        }
    }
    Ok(())
}

pub(super) fn module_from_relative_path(
    path: &Path,
    python_version: &str,
) -> Result<String, String> {
    let mut components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let Some(last) = components.last_mut() else {
        return Err(format!(
            "cannot derive a Python module name from {}",
            path.display()
        ));
    };
    if last == "__init__.py" {
        components.pop();
        if components.is_empty() {
            return Err(format!(
                "root-level __init__.py needs a parent package name: {}",
                path.display()
            ));
        }
    } else if let Some(stem) = last.strip_suffix(".py") {
        *last = stem.to_string();
    } else {
        return Err(format!("expected a .py file: {}", path.display()));
    }
    if components
        .iter()
        .any(|part| !codepact::is_python_identifier(part, python_version))
    {
        return Err(format!(
            "path does not map to valid Python module components: {}",
            path.display()
        ));
    }
    Ok(components.join("."))
}

pub(super) fn resolve_project_root(root: &Path) -> Result<PathBuf, String> {
    codepact::adoption::reject_symlink_components(root, "project root")?;
    fs::canonicalize(root)
        .map_err(|error| format!("cannot resolve project root {}: {error}", root.display()))
}
