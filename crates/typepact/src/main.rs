use std::{
    env,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    process::ExitCode,
};
use typepact::{inspect, is_python_identifier, SourceFile, DEFAULT_MAX_SOURCE_BYTES};

fn usage() {
    eprintln!("Usage: typepact inspect-contracts [--root DIR] [--python-version 3.11|3.12|3.13] [--json] FILE...\n       typepact --version\n\nInspect declarations without importing or executing Python source.\nWithout --root, each file's stem is its logical module name; use --root for packages and multi-file imports.");
}

fn module_from_relative_path(path: &Path, python_version: &str) -> Result<String, String> {
    let mut components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
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
        .any(|part| !is_python_identifier(part, python_version))
    {
        return Err(format!(
            "path does not map to valid Python module components: {}",
            path.display()
        ));
    }
    Ok(components.join("."))
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    if command == "--version" || command == "-V" {
        println!("typepact {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if command == "--help" || command == "-h" {
        usage();
        return ExitCode::SUCCESS;
    }
    if command != "inspect-contracts" {
        eprintln!("Unknown command: {command}");
        usage();
        return ExitCode::from(2);
    }
    let mut python_version = "3.11".to_string();
    let mut json = true;
    let mut root = None;
    let mut paths = Vec::new();
    let mut it = args;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--python-version" => {
                let Some(value) = it.next() else {
                    eprintln!("--python-version requires a value");
                    return ExitCode::from(2);
                };
                python_version = value;
            }
            "--root" => {
                let Some(value) = it.next() else {
                    eprintln!("--root requires a directory");
                    return ExitCode::from(2);
                };
                root = Some(PathBuf::from(value));
            }
            "--json" => json = true,
            "--help" | "-h" => {
                usage();
                return ExitCode::SUCCESS;
            }
            _ if arg.starts_with('-') => {
                eprintln!("Unknown option: {arg}");
                usage();
                return ExitCode::from(2);
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if paths.is_empty() {
        eprintln!("inspect-contracts requires at least one file");
        return ExitCode::from(2);
    }

    let cwd = match env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let root = match root {
        Some(root) => {
            let root = if root.is_absolute() {
                root
            } else {
                cwd.join(root)
            };
            match fs::canonicalize(&root) {
                Ok(root) => Some(root),
                Err(error) => {
                    eprintln!("cannot resolve module root {}: {error}", root.display());
                    return ExitCode::from(2);
                }
            }
        }
        None => None,
    };

    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    let mut modules = std::collections::BTreeSet::new();
    for path in paths {
        let physical_path = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        let canonical_path = match fs::canonicalize(&physical_path) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("cannot read {}: {error}", physical_path.display());
                return ExitCode::from(2);
            }
        };
        let logical_path = match &root {
            Some(root) => match canonical_path.strip_prefix(root) {
                Ok(relative) => relative.to_path_buf(),
                Err(_) => {
                    eprintln!(
                        "file {} is outside module root {}",
                        canonical_path.display(),
                        root.display()
                    );
                    return ExitCode::from(2);
                }
            },
            None => match canonical_path.file_name() {
                Some(name) if name == "__init__.py" => PathBuf::from("__init__.py"),
                Some(name) => PathBuf::from(name),
                None => {
                    eprintln!(
                        "cannot derive a module name from {}",
                        canonical_path.display()
                    );
                    return ExitCode::from(2);
                }
            },
        };
        let module = match if root.is_some() {
            module_from_relative_path(&logical_path, &python_version)
        } else if logical_path == Path::new("__init__.py") {
            canonical_path
                .parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| is_python_identifier(name, &python_version))
                .ok_or_else(|| "cannot derive package module name".to_string())
        } else {
            logical_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .filter(|stem| is_python_identifier(stem, &python_version))
                .ok_or_else(|| format!("cannot derive module name from {}", logical_path.display()))
        } {
            Ok(module) => module,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        };
        if !modules.insert(module.clone()) {
            eprintln!("multiple files map to module {module}; provide a suitable --root");
            return ExitCode::from(2);
        }

        let metadata = match fs::metadata(&canonical_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                eprintln!("cannot inspect {}: {error}", canonical_path.display());
                return ExitCode::from(2);
            }
        };
        let remaining = DEFAULT_MAX_SOURCE_BYTES.saturating_sub(total_bytes);
        if metadata.len() > remaining as u64 {
            eprintln!(
                "source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded before reading {}",
                canonical_path.display()
            );
            return ExitCode::from(2);
        }
        let mut source = String::new();
        let mut file = match File::open(&canonical_path) {
            Ok(file) => file.take(remaining as u64 + 1),
            Err(error) => {
                eprintln!("cannot read {}: {error}", canonical_path.display());
                return ExitCode::from(2);
            }
        };
        if let Err(error) = file.read_to_string(&mut source) {
            eprintln!(
                "cannot read UTF-8 Python source {}: {error}",
                canonical_path.display()
            );
            return ExitCode::from(2);
        }
        if source.len() > remaining {
            eprintln!(
                "source byte limit of {DEFAULT_MAX_SOURCE_BYTES} exceeded while reading {}",
                canonical_path.display()
            );
            return ExitCode::from(2);
        }
        total_bytes += source.len();
        files.push(SourceFile::new(
            logical_path.to_string_lossy().into_owned(),
            module,
            source,
        ));
    }

    let out = inspect(&files, &python_version);
    if json {
        match serde_json::to_string_pretty(&out) {
            Ok(serialized) => println!("{serialized}"),
            Err(error) => {
                eprintln!("serialization failure: {error}");
                return ExitCode::from(2);
            }
        }
    }
    if out
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
        || out.coverage.status != "complete-for-supported-subset"
    {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
