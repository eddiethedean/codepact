//! Bounded reads and wheel integrity checks shared by installed and archive inputs.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use codepact::foundation::{self, regular_relative_path, MAX_METADATA_BYTES};
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::Path,
};

const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Record {
    entries: BTreeMap<String, (String, String)>,
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

pub(super) enum RecordLayout {
    Archive,
    Installed,
}

pub(super) fn read_record(content: &[u8], layout: RecordLayout) -> Result<Record, String> {
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
            if matches!(layout, RecordLayout::Installed) {
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
    pub(super) fn contains(&self, member: &str) -> bool {
        self.entries.contains_key(member)
    }

    pub(super) fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

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
    let record = read_record(record, RecordLayout::Archive)?;
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

pub(super) fn read_verified_wheel(path: &Path) -> Result<VerifiedWheel, String> {
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
    Ok(VerifiedWheel {
        distribution,
        version,
        members,
    })
}

pub(super) struct VerifiedWheel {
    pub(super) distribution: String,
    pub(super) version: String,
    members: BTreeMap<String, Vec<u8>>,
}

impl VerifiedWheel {
    pub(super) fn paths(&self) -> impl Iterator<Item = &str> {
        self.members.keys().map(String::as_str)
    }

    pub(super) fn member(&self, path: &str) -> Option<&[u8]> {
        self.members.get(path).map(Vec::as_slice)
    }

    pub(super) fn text(&self, path: &str) -> Result<&str, String> {
        wheel_text(&self.members, path)
    }

    pub(super) fn members(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.members
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::{read_record, RecordLayout};

    #[test]
    fn installed_records_can_list_scripts_outside_the_import_root() {
        let content = b"example/__init__.py,hash,1\n../../../bin/example,hash,1\n";
        let archive = read_record(content, RecordLayout::Archive);
        assert_eq!(
            archive.err().as_deref(),
            Some("invalid wheel RECORD path \"../../../bin/example\"")
        );
        let installed = read_record(content, RecordLayout::Installed).expect("installed inventory");
        assert_eq!(
            installed.paths().collect::<Vec<_>>(),
            ["example/__init__.py"]
        );
    }
}
