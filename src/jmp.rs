use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::PathBuf;

use itertools::Itertools;
use serde::de::{self, Error, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashAlgorithm {
    Sha256,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Fingerprint {
    pub algorithm: HashAlgorithm,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locator {
    Size(usize),
    Entry(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    Bzip2,
    Gzip,
    Lzma,
    Xz,
    Zlib,
    Zstd,
}

#[derive(Debug)]
pub enum ArchiveType {
    Zip,
    Tar,
    CompressedTar(Compression),
}

impl Serialize for ArchiveType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ArchiveType::Zip => serializer.serialize_str("zip"),
            ArchiveType::Tar => serializer.serialize_str("tar"),
            ArchiveType::CompressedTar(Compression::Bzip2) => serializer.serialize_str("tar.bz2"),
            ArchiveType::CompressedTar(Compression::Gzip) => serializer.serialize_str("tar.gz"),
            ArchiveType::CompressedTar(Compression::Lzma) => serializer.serialize_str("tar.lzma"),
            ArchiveType::CompressedTar(Compression::Xz) => serializer.serialize_str("tar.xz"),
            ArchiveType::CompressedTar(Compression::Zlib) => serializer.serialize_str("tar.Z"),
            ArchiveType::CompressedTar(Compression::Zstd) => serializer.serialize_str("tar.zst"),
        }
    }
}

struct ArchiveTypeVisitor;

impl<'de> Visitor<'de> for ArchiveTypeVisitor {
    type Value = ArchiveType;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "one of: zip, tar, tbz2, tar.bz2, tgz, tar.gz, tlz, tar.lzma, tar.xz, tar.Z, tzst or \
            tar.zst"
        )
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        // These values are derived from the `-a` extensions described by GNU tar here:
        // https://www.gnu.org/software/tar/manual/html_node/gzip.html#gzip
        match value {
            "zip" => Ok(ArchiveType::Zip),
            "tar" => Ok(ArchiveType::Tar),
            "tbz2" | "tar.bz2" => Ok(ArchiveType::CompressedTar(Compression::Bzip2)),
            "tgz" | "tar.gz" => Ok(ArchiveType::CompressedTar(Compression::Gzip)),
            "tlz" | "tar.lzma" => Ok(ArchiveType::CompressedTar(Compression::Lzma)),
            "tar.xz" => Ok(ArchiveType::CompressedTar(Compression::Xz)),
            "tar.Z" => Ok(ArchiveType::CompressedTar(Compression::Zlib)),
            "tzst" | "tar.zst" => Ok(ArchiveType::CompressedTar(Compression::Zstd)),
            _ => Err(de::Error::invalid_value(Unexpected::Str(value), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for ArchiveType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(ArchiveTypeVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Scie {
    pub version: String,
    pub root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Blob {
    #[serde(flatten)]
    pub locator: Locator,
    pub fingerprint: Fingerprint,
    pub name: String,
    #[serde(default)]
    pub always_extract: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Archive {
    #[serde(flatten)]
    pub locator: Locator,
    pub fingerprint: Fingerprint,
    pub archive_type: ArchiveType,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub always_extract: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub enum File {
    Archive(Archive),
    Blob(Blob),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cmd {
    pub exe: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub additional_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub scie: Scie,
    pub files: Vec<File>,
    pub command: Cmd,
    #[serde(default)]
    pub additional_commands: HashMap<String, Cmd>,
}

const MAXIMUM_CONFIG_SIZE: usize = 0xFFFF;

// See "4.3.6 Overall .ZIP file format:" and "4.3.16  End of central directory record:"
// in https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT for Zip file format facts
// leveraged here.

const EOCD_SIGNATURE: (&u8, &u8, &u8, &u8) = (&0x06, &0x05, &0x4b, &0x50);

pub fn end_of_zip(data: &[u8], maximum_trailer_size: usize) -> Result<usize, String> {
    #[allow(clippy::too_many_arguments)]
    let eocd_struct = structure!("<HHHHIIH");

    let eocd_size = eocd_struct.size();
    // N.B.: The variable length comment field can be up to 0xFFFF big.
    let maximum_eocd_size = eocd_size + 0xFFFF;
    let max_scan = maximum_eocd_size + maximum_trailer_size;

    let offset_from_eof = data
        .iter()
        .rev()
        .take(max_scan)
        .tuple_windows::<(_, _, _, _)>()
        .position(|chunk| EOCD_SIGNATURE == chunk)
        .ok_or_else(|| {
            format!(
                "Failed to find application zip end of central directory record within the last \
                {} bytes of the file. Invalid NCE.",
                max_scan
            )
        })?;
    let eocd_start = data.len() - offset_from_eof;
    let eocd_end = eocd_start + eocd_size;
    let (
        _disk_no,
        _cd_disk_no,
        _disk_cd_record_count,
        _total_cd_record_count,
        _cd_size,
        _cd_offset,
        zip_comment_size,
    ) = eocd_struct
        .unpack(&data[eocd_start..eocd_end])
        .map_err(|e| format!("{}", e))?;
    Ok(eocd_end + (zip_comment_size as usize))
}

pub fn load(data: &[u8]) -> Result<Config, String> {
    let end_of_zip = end_of_zip(data, MAXIMUM_CONFIG_SIZE)?;
    serde_json::from_slice(&data[end_of_zip..]).map_err(|e| format!("{}", e))
}

#[cfg(test)]
mod tests {
    use super::{
        Archive, ArchiveType, Blob, Cmd, Compression, Config, File, Fingerprint, HashAlgorithm,
        Locator, Scie,
    };

    #[test]
    fn test_serialized_form() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&Config {
                scie: Scie {
                    version: "0.1.0".to_string(),
                    root: "~/.nce".into(),
                },
                files: vec![
                    File::Blob(Blob {
                        locator: Locator::Size(1137),
                        fingerprint: Fingerprint {
                            algorithm: HashAlgorithm::Sha256,
                            hash: "abc".into()
                        },
                        name: "pants-client".into(),
                        always_extract: true
                    }),
                    File::Archive(Archive {
                        locator: Locator::Size(123),
                        fingerprint: Fingerprint {
                            algorithm: HashAlgorithm::Sha256,
                            hash: "345".into()
                        },
                        archive_type: ArchiveType::CompressedTar(Compression::Zstd),
                        name: Some("python".into()),
                        always_extract: false
                    }),
                    File::Archive(Archive {
                        locator: Locator::Size(42),
                        fingerprint: Fingerprint {
                            algorithm: HashAlgorithm::Sha256,
                            hash: "def".into()
                        },
                        archive_type: ArchiveType::Zip,
                        name: None,
                        always_extract: false
                    })
                ],
                command: Cmd {
                    exe: "bob/exe".into(),
                    args: Default::default(),
                    env: Default::default(),
                    additional_files: Default::default()
                },
                additional_commands: Default::default()
            })
            .unwrap()
        )
    }

    #[test]
    fn test_deserialize_defaults() {
        eprintln!(
            "{:#?}",
            serde_json::from_str::<Config>(
                r#"
            {
              "scie": {
                "version": "0.1.0",
                "root": "~/.nce"
              },
              "files": [
                {
                  "type": "blob",
                  "name": "pants-client",
                  "size": 1,
                  "fingerprint": {
                    "algorithm": "sha256",
                    "hash": "789"
                  }
                },
                {
                  "type": "archive",
                  "size": 1137,
                  "fingerprint": {
                    "algorithm": "sha256",
                    "hash": "abc"
                  },
                  "archive_type": "tar.gz"
                },
                {
                  "type": "archive",
                  "name": "app",
                  "size": 42,
                  "fingerprint": {
                    "algorithm": "sha256",
                    "hash": "xyz"
                  },
                  "archive_type": "zip"
                }
              ],
              "command": {
                  "env": {
                    "PEX_VERBOSE": "1"
                  },
                  "exe":"{python}/bin/python",
                  "args": [
                    "{app}"
                  ]
              }
            }
        "#
            )
            .unwrap()
        )
    }
}