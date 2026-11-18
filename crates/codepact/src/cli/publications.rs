use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use codepact::foundation::{
    self, module_for_source, regular_relative_path, ProviderPackage, MAX_METADATA_BYTES,
};
use codepact::SourceFile;
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Record {
    pub(super) entries: BTreeMap<String, (String, String)>,
}

pub(super) fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    codepact::adoption::reject_symlink_components(path, "contract input")?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(format!(
            "{} must be a regular file within the {limit} byte input limit",
            path.display()
        ));
    }
    let mut content = Vec::new();
    File::open(path)
        .and_then(|file| file.take(limit as u64 + 1).read_to_end(&mut content))
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if content.len() > limit {
        return Err(format!(
            "{} exceeds the {limit} byte input limit",
            path.display()
        ));
    }
    Ok(content)
}

pub(super) fn read_text(path: &Path, limit: usize) -> Result<String, String> {
    String::from_utf8(read_regular(path, limit)?)
        .map_err(|_| format!("{} is not UTF-8", path.display()))
}

pub(super) fn read_record(content: &[u8], installed: bool) -> Result<Record, String> {
    let mut csv = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(content);
    let mut record = Record::default();
    for row in csv.records() {
        let row = row.map_err(|error| format!("malformed RECORD: {error}"))?;
        let [path, digest, size] = [row.get(0), row.get(1), row.get(2)];
        let (Some(path), Some(digest), Some(size)) = (path, digest, size) else {
            return Err("RECORD requires exactly three columns".into());
        };
        if row.len() != 3 {
            return Err("RECORD requires exactly three columns".into());
        }
        if !regular_relative_path(path) {
            if installed {
                continue;
            }
            return Err(format!("invalid wheel RECORD path {path:?}"));
        }
        if record
            .entries
            .insert(path.into(), (digest.into(), size.into()))
            .is_some()
        {
            return Err(format!("duplicate RECORD member {path}"));
        }
    }
    Ok(record)
}

impl Record {
    pub(super) fn verify_member(&self, name: &str, data: &[u8]) -> Result<(), String> {
        let (checksum, size) = self
            .entries
            .get(name)
            .ok_or_else(|| format!("{name} is missing from RECORD"))?;
        let hash_matches = checksum
            .split_once('=')
            .is_some_and(|(algorithm, expected)| {
                let actual = match algorithm {
                    "sha256" => URL_SAFE_NO_PAD.encode(Sha256::digest(data)),
                    "sha384" => URL_SAFE_NO_PAD.encode(Sha384::digest(data)),
                    "sha512" => URL_SAFE_NO_PAD.encode(Sha512::digest(data)),
                    _ => return false,
                };
                expected == actual
            });
        if !hash_matches || size.parse::<usize>().ok() != Some(data.len()) {
            return Err(format!("RECORD hash or size mismatch for {name}"));
        }
        Ok(())
    }
}

pub(super) fn metadata_identity(content: &str) -> Result<(String, String), String> {
    fn field<'a>(content: &'a str, name: &str) -> Result<&'a str, String> {
        let mut values = content
            .lines()
            .take_while(|line| !line.is_empty())
            .filter_map(|line| {
                let (field, value) = line.split_once(':')?;
                field.eq_ignore_ascii_case(name).then_some(value.trim())
            });
        let value = values
            .next()
            .ok_or_else(|| format!("METADATA is missing {name}"))?;
        if value.is_empty() || values.next().is_some() {
            return Err(format!("METADATA has an invalid or duplicate {name}"));
        }
        Ok(value)
    }
    let distribution = foundation::normalized_distribution(field(content, "Name")?)
        .ok_or("invalid distribution name in METADATA")?;
    let version = field(content, "Version")?.to_owned();
    Ok((distribution, version))
}

pub(super) fn pure_wheel(content: &str) -> bool {
    let mut pure = None;
    let mut tags = 0usize;
    for (key, value) in content.lines().filter_map(|line| line.split_once(':')) {
        let value = value.trim();
        if key.eq_ignore_ascii_case("Root-Is-Purelib") {
            if pure.replace(value.eq_ignore_ascii_case("true")).is_some() {
                return false;
            }
        } else if key.eq_ignore_ascii_case("Tag") {
            let Some((python, abi_platform)) = value.split_once('-') else {
                return false;
            };
            if python.is_empty() || abi_platform != "none-any" {
                return false;
            }
            tags += 1;
        }
    }
    pure == Some(true) && tags > 0
}

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

fn archive_members(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let bytes = read_regular(path, MAX_ARCHIVE_BYTES)?;
    let mut wheel = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("invalid wheel archive: {error}"))?;
    if wheel.has_duplicate_names() || wheel.len() > codepact::DEFAULT_MAX_INPUT_FILES {
        return Err("wheel contains duplicate names or exceeds the archive entry limit".into());
    }
    let mut members = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut remaining = MAX_ARCHIVE_BYTES;
    for index in 0..wheel.len() {
        let member = wheel
            .by_index(index)
            .map_err(|error| format!("cannot read wheel member: {error}"))?;
        let name =
            std::str::from_utf8(member.name_raw()).map_err(|_| "wheel member path is not UTF-8")?;
        let canonical = if member.is_dir() {
            name.strip_suffix('/').unwrap_or(name)
        } else {
            name
        };
        if !regular_relative_path(canonical)
            || !names.insert(canonical.to_owned())
            || member.is_symlink()
        {
            return Err(format!(
                "invalid, duplicate or symbolic wheel member {name:?}"
            ));
        }
        if member.is_dir() {
            continue;
        }
        if !member.is_file() || member.size() > remaining as u64 {
            return Err("wheel exceeds the expanded archive byte limit".into());
        }
        let name = name.to_owned();
        let mut content = Vec::new();
        member
            .take(remaining as u64 + 1)
            .read_to_end(&mut content)
            .map_err(|error| format!("cannot decode wheel member {name}: {error}"))?;
        remaining = remaining
            .checked_sub(content.len())
            .ok_or("wheel exceeds the expanded archive byte limit")?;
        members.insert(name, content);
    }
    Ok(members)
}

fn wheel_text<'a>(members: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Result<&'a str, String> {
    let data = members
        .get(name)
        .ok_or_else(|| format!("wheel is missing {name}"))?;
    if data.len() > MAX_METADATA_BYTES {
        return Err(format!("wheel metadata {name} exceeds the byte limit"));
    }
    std::str::from_utf8(data).map_err(|_| format!("wheel member {name} is not UTF-8"))
}

fn verify_record(members: &BTreeMap<String, Vec<u8>>, path: &str) -> Result<(), String> {
    let record = members.get(path).ok_or("wheel is missing RECORD")?;
    if record.len() > MAX_RECORD_BYTES {
        return Err("wheel RECORD exceeds the byte limit".into());
    }
    let record = read_record(record, false)?;
    for (member, data) in members {
        if member == path {
            if !record
                .entries
                .get(member)
                .is_some_and(|(hash, size)| hash.is_empty() && size.is_empty())
            {
                return Err(
                    "wheel RECORD must describe its own row with empty hash and size".into(),
                );
            }
        } else {
            record.verify_member(member, data)?;
        }
    }
    if record.entries.len() != members.len() {
        return Err("wheel RECORD lists files missing from the archive".into());
    }
    Ok(())
}

fn verify_wheel(path: &Path, requested_target: Option<&str>) -> Result<serde_json::Value, String> {
    let members = archive_members(path)?;
    let metadata = members
        .keys()
        .filter_map(|path| path.strip_suffix("/METADATA"))
        .filter(|path| path.ends_with(".dist-info") && !path.contains('/'))
        .collect::<Vec<_>>();
    let [metadata] = metadata.as_slice() else {
        return Err("wheel must contain exactly one top-level .dist-info/METADATA".into());
    };
    let (distribution, version) =
        metadata_identity(wheel_text(&members, &format!("{metadata}/METADATA"))?)?;
    let dist_info = metadata.strip_suffix(".dist-info").unwrap();
    let (metadata_dist, metadata_version) = dist_info
        .split_once('-')
        .ok_or("invalid wheel .dist-info identity")?;
    let mut filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".whl"))
        .ok_or("wheel verification expects a .whl file")?
        .split('-');
    if foundation::normalized_distribution(metadata_dist).as_deref() != Some(&distribution)
        || metadata_version != version
        || filename
            .next()
            .and_then(foundation::normalized_distribution)
            .as_deref()
            != Some(&distribution)
        || filename.next() != Some(&version)
    {
        return Err(
            "wheel filename, .dist-info directory and METADATA disagree on distribution identity"
                .into(),
        );
    }
    if !pure_wheel(wheel_text(&members, &format!("{metadata}/WHEEL"))?) {
        return Err("contract verification requires a regular pure-Python wheel".into());
    }
    if let Some(native) = members
        .keys()
        .find(|path| super::discovery::is_native_extension(Path::new(path)))
    {
        return Err(format!(
            "native extension {native} is outside the pure-Python provider profile"
        ));
    }
    verify_record(&members, &format!("{metadata}/RECORD"))?;
    let paths = members
        .keys()
        .filter(|name| name.ends_with(&format!("/{}", foundation::MANIFEST_FILENAME)))
        .collect::<Vec<_>>();
    let [manifest_path] = paths.as_slice() else {
        return Err("wheel must contain exactly one published contract manifest".into());
    };
    let content = wheel_text(&members, manifest_path)?;
    let decoded = foundation::decode_manifest(content)?;
    let target = requested_target.unwrap_or(&decoded.python_version);
    let mut package = ProviderPackage {
        distribution,
        version,
        import_scope: decoded.import_scope,
        sources: Vec::new(),
        marked: true,
        manifest: Some(content.to_owned()),
    };
    if package.manifest_path() != **manifest_path
        || !members
            .get(&package.marker_path())
            .is_some_and(Vec::is_empty)
    {
        return Err(
            "wheel manifest and empty py.codepact marker must match the declared import scope"
                .into(),
        );
    }
    let prefix = format!("{}/", package.import_scope.replace('.', "/"));
    let mut remaining = codepact::DEFAULT_MAX_SOURCE_BYTES;
    for (path, data) in &members {
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
