use super::{
    artifacts::{read_regular, read_text, read_verified_wheel},
    CommandOutput,
};
use codepact::foundation::{self, module_for_source, ProviderPackage, MAX_METADATA_BYTES};
use codepact::SourceFile;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn staged(
    root: &Path,
    distribution: String,
    version: String,
    scope: String,
    target: &str,
) -> Result<ProviderPackage, String> {
    if scope.is_empty()
        || !scope
            .split('.')
            .all(|part| codepact::is_python_identifier(part, target))
    {
        return Err("invalid regular import scope for contracts export".into());
    }
    codepact::adoption::reject_symlink_components(root, "staged provider root")?;
    let package_root = root.join(scope.replace('.', "/"));
    codepact::adoption::reject_symlink_components(&package_root, "staged package directory")?;
    let paths = super::discovery::discover_provider_sources(&package_root)?;
    let mut remaining = codepact::DEFAULT_MAX_SOURCE_BYTES;
    let mut sources = Vec::new();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "staged source is outside its root")?
            .to_str()
            .ok_or("staged source path is not UTF-8")?
            .replace('\\', "/");
        let module = module_for_source(&relative, target)
            .ok_or_else(|| format!("invalid staged module path {relative}"))?;
        let source = read_text(&path, remaining)?;
        remaining -= source.len();
        sources.push(SourceFile::new(relative, module, source));
    }
    let marker = package_root.join(foundation::MARKER_FILENAME);
    if !read_regular(&marker, 1)?.is_empty() {
        return Err("the staged py.codepact marker must be empty".into());
    }
    Ok(ProviderPackage {
        distribution,
        version,
        import_scope: scope,
        sources,
        marked: true,
        manifest: None,
    })
}

fn wheel_publication(path: &Path) -> Result<(ProviderPackage, String), String> {
    let wheel = read_verified_wheel(path)?;
    let paths = wheel
        .paths()
        .filter(|name| name.ends_with(&format!("/{}", foundation::MANIFEST_FILENAME)))
        .collect::<Vec<_>>();
    let [manifest_path] = paths.as_slice() else {
        return Err("wheel must contain exactly one published contract manifest".into());
    };
    let content = wheel.text(manifest_path)?;
    let decoded = foundation::decode_manifest(content)?;
    let target = decoded.python_version;
    let mut package = ProviderPackage {
        distribution: wheel.distribution.clone(),
        version: wheel.version.clone(),
        import_scope: decoded.import_scope,
        sources: Vec::new(),
        marked: true,
        manifest: Some(content.to_owned()),
    };
    if package.manifest_path() != *manifest_path
        || !wheel
            .member(&package.marker_path())
            .is_some_and(<[u8]>::is_empty)
    {
        return Err(
            "wheel manifest and empty py.codepact marker must match the declared import scope"
                .into(),
        );
    }
    let prefix = format!("{}/", package.import_scope.replace('.', "/"));
    let mut remaining = codepact::DEFAULT_MAX_SOURCE_BYTES;
    for (path, data) in wheel.members() {
        if path.starts_with(&prefix) && path.ends_with(".py") {
            remaining = remaining
                .checked_sub(data.len())
                .ok_or("wheel source exceeds the byte limit")?;
            let module = module_for_source(path, &target)
                .ok_or_else(|| format!("invalid wheel module {path}"))?;
            let text = std::str::from_utf8(data)
                .map_err(|_| format!("wheel source {path} is not UTF-8"))?;
            package.sources.push(SourceFile::new(path, module, text));
        }
    }
    Ok((package, target))
}

fn verify_wheel(path: &Path, requested_target: Option<&str>) -> Result<serde_json::Value, String> {
    let (package, target) = wheel_publication(path)?;
    let verified = foundation::verify_publication(&package, requested_target.unwrap_or(&target))?;
    Ok(serde_json::json!({
        "status": "verified",
        "distribution": verified.distribution,
        "version": verified.version,
        "import_scope": verified.import_scope,
        "manifest_sha256": foundation::sha256(package.manifest.as_deref().unwrap_or_default().as_bytes()),
        "files": verified.files.len(),
        "symbols": verified.symbols.len()
    }))
}

fn diff_wheels(
    before: &Path,
    after: &Path,
    requested_target: Option<&str>,
    json: bool,
) -> Result<CommandOutput, String> {
    let (before, before_target) = wheel_publication(before)?;
    let (after, after_target) = wheel_publication(after)?;
    let before = foundation::capture_publication(&before, &before_target)?;
    let after = foundation::capture_publication(&after, &after_target)?;
    let mut diff = foundation::diff_publications(&before, &after);
    if let Some(target) = requested_target {
        if diff.before.python_version != target || diff.after.python_version != target {
            diff.compatible = false;
            diff.changes.push(foundation::ContractChange {
                public_symbol: "<publication>".into(),
                kind: foundation::ContractChangeKind::InsufficientEvidence,
                detail: format!("one or both publications do not apply to Python {target}"),
            });
            diff.changes.sort();
        }
    }
    let status = u8::from(!diff.compatible);
    let output = if json {
        serde_json::to_string_pretty(&diff).map_err(|error| error.to_string())?
    } else {
        use std::fmt::Write;
        let verdict = if diff.compatible {
            "compatible"
        } else {
            "noncompatible"
        };
        let mut output = format!(
            "{} {} → {} {}: {verdict}",
            diff.before.distribution,
            diff.before.version,
            diff.after.distribution,
            diff.after.version
        );
        for change in &diff.changes {
            use foundation::ContractChangeKind::*;
            let kind = match change.kind {
                Compatible => "compatible",
                NewCallerObligation => "new-caller-obligation",
                WeakenedGuarantee => "weakened-guarantee",
                InsufficientEvidence => "insufficient-evidence",
            };
            let _ = write!(
                output,
                "\n{}: {kind}: {}",
                change.public_symbol, change.detail
            );
        }
        output
    };
    if output.len() >= 32 * 1024 * 1024 {
        return Err("serialized contract comparison exceeds the output byte limit".into());
    }
    Ok(CommandOutput::stdout(output, status))
}

pub(super) fn run(mut args: impl Iterator<Item = String>) -> Result<CommandOutput, String> {
    let action = args
        .next()
        .ok_or("contracts requires export, verify, or diff")?;
    let mut options = BTreeMap::new();
    let mut positional = Vec::new();
    let mut json = false;
    while let Some(arg) = args.next() {
        if action == "diff" && arg == "--json" {
            json = true;
        } else if arg.starts_with("--") {
            if !matches!(
                arg.as_str(),
                "--root" | "--distribution" | "--version" | "--import-scope" | "--python-version"
            ) {
                return Err(format!("unknown contracts option {arg}"));
            }
            let value = args
                .next()
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| format!("missing value for {arg}"))?;
            if options.insert(arg.clone(), value).is_some() {
                return Err(format!("duplicate contracts option {arg}"));
            }
        } else {
            positional.push(arg);
        }
    }
    let python = options.remove("--python-version");
    if python
        .as_deref()
        .is_some_and(|version| !matches!(version, "3.11" | "3.12" | "3.13"))
    {
        return Err("unsupported contracts Python source target".into());
    }
    let value = match action.as_str() {
        "export" if positional.is_empty() => {
            let required = |key: &str| options.get(key).map(String::as_str).ok_or_else(|| format!("contracts export requires {key}"));
            let root = PathBuf::from(required("--root")?);
            let distribution = foundation::normalized_distribution(required("--distribution")?).ok_or("invalid distribution name")?;
            let version = required("--version")?.to_owned();
            let scope = required("--import-scope")?.to_owned();
            let target = python.as_deref().unwrap_or("3.13");
            let package = staged(&root, distribution, version, scope, target)?;
            let manifest = foundation::export_manifest(&package, target)?;
            serde_json::to_value(manifest).map_err(|error| error.to_string())?
        }
        "verify" if positional.len() == 1 && options.is_empty() => verify_wheel(Path::new(&positional[0]), python.as_deref())?,
        "diff" if positional.len() == 2 && options.is_empty() => {
            return diff_wheels(
                Path::new(&positional[0]), Path::new(&positional[1]), python.as_deref(), json,
            );
        }
        _ => return Err("use contracts export --root DIR --distribution NAME --version VERSION --import-scope PACKAGE, contracts verify WHEEL, or contracts diff BEFORE_WHEEL AFTER_WHEEL [--json]".into()),
    };
    let output = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    if output.len() >= MAX_METADATA_BYTES {
        return Err("serialized contract manifest exceeds the metadata byte limit".into());
    }
    Ok(CommandOutput::stdout(output, 0))
}
