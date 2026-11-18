use super::artifacts::{read_regular, read_text, read_verified_wheel};
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

fn verify_wheel(path: &Path, requested_target: Option<&str>) -> Result<serde_json::Value, String> {
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
    let target = requested_target.unwrap_or(&decoded.python_version);
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
            let module = module_for_source(path, target)
                .ok_or_else(|| format!("invalid wheel module {path}"))?;
            let text = std::str::from_utf8(data)
                .map_err(|_| format!("wheel source {path} is not UTF-8"))?;
            package.sources.push(SourceFile::new(path, module, text));
        }
    }
    let verified = foundation::verify_publication(&package, target)?;
    Ok(serde_json::json!({
        "status": "verified",
        "distribution": verified.distribution,
        "version": verified.version,
        "import_scope": verified.import_scope,
        "manifest_sha256": foundation::sha256(content.as_bytes()),
        "files": verified.files.len(),
        "symbols": verified.symbols.len()
    }))
}

pub(super) fn run(mut args: impl Iterator<Item = String>) -> Result<String, String> {
    let action = args.next().ok_or("contracts requires export or verify")?;
    let mut options = BTreeMap::new();
    let mut positional = Vec::new();
    while let Some(arg) = args.next() {
        if arg.starts_with("--") {
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
        _ => return Err("use contracts export --root DIR --distribution NAME --version VERSION --import-scope PACKAGE or contracts verify WHEEL".into()),
    };
    let output = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    if output.len() >= MAX_METADATA_BYTES {
        return Err("serialized contract manifest exceeds the metadata byte limit".into());
    }
    Ok(output)
}
