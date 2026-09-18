use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use rand::RngCore;

use crate::error::{AppError, redacted_internal_from, redacted_storage_from};

pub const BACKUP_MAGIC: &[u8; 9] = b"SKATBKUP1";
pub const BACKUP_FORMAT_VERSION: u32 = 2;
pub const BACKUP_FILE_EXTENSION: &str = "skatbackup";
pub const MIN_PASSPHRASE_LEN: usize = 12;
pub const MAX_ENCRYPTED_BACKUP_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_TAR_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_TAR_ENTRIES: usize = 10_000;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

pub fn validate_passphrase(passphrase: &str) -> Result<(), AppError> {
    if passphrase.trim().len() < MIN_PASSPHRASE_LEN {
        return Err(AppError::validation(
            format!(
                "Backup passphrase must be at least {MIN_PASSPHRASE_LEN} characters"
            ),
            "passphrase",
        ));
    }
    Ok(())
}

pub fn is_encrypted_backup_file(path: &std::path::Path) -> bool {
    if path.extension().and_then(|ext| ext.to_str()) == Some(BACKUP_FILE_EXTENSION) {
        return true;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 9];
    file.read_exact(&mut magic).is_ok() && &magic == BACKUP_MAGIC
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], AppError> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(redacted_internal_from)?;
    Ok(key)
}

pub fn encrypt_bytes(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    validate_passphrase(passphrase)?;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(redacted_internal_from)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| AppError::validation("Backup encryption failed", "passphrase"))?;

    let mut output = Vec::with_capacity(
        BACKUP_MAGIC.len() + 4 + SALT_LEN + NONCE_LEN + ciphertext.len(),
    );
    output.extend_from_slice(BACKUP_MAGIC);
    output.extend_from_slice(&BACKUP_FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

pub fn decrypt_bytes(passphrase: &str, encrypted: &[u8]) -> Result<Vec<u8>, AppError> {
    if encrypted.len() < BACKUP_MAGIC.len() + 4 + SALT_LEN + NONCE_LEN + 16 {
        return Err(AppError::validation("Backup file is too small", "backupPath"));
    }
    if &encrypted[..BACKUP_MAGIC.len()] != BACKUP_MAGIC {
        return Err(AppError::validation("Unsupported backup format", "backupPath"));
    }

    let version = u32::from_le_bytes(
        encrypted[BACKUP_MAGIC.len()..BACKUP_MAGIC.len() + 4]
            .try_into()
            .map_err(|_| AppError::validation("Invalid backup header", "backupPath"))?,
    );
    if version != BACKUP_FORMAT_VERSION {
        return Err(AppError::validation(
            format!("Unsupported backup format version {version}"),
            "backupPath",
        ));
    }

    let header_end = BACKUP_MAGIC.len() + 4;
    let salt = &encrypted[header_end..header_end + SALT_LEN];
    let nonce_bytes = &encrypted[header_end + SALT_LEN..header_end + SALT_LEN + NONCE_LEN];
    let ciphertext = &encrypted[header_end + SALT_LEN + NONCE_LEN..];

    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(redacted_internal_from)?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::validation("Incorrect backup passphrase", "passphrase"))
}

fn collect_archive_entries(
    root: &Path,
    relative: &Path,
    entries: &mut Vec<(PathBuf, bool)>,
) -> Result<(), AppError> {
    let directory = root.join(relative);
    let metadata = fs::symlink_metadata(&directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::validation(
            "Backup archive source must be a directory",
            "backupPath",
        ));
    }

    let mut children = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    for child in children {
        let child_relative = relative.join(child.file_name());
        let file_type = child.file_type()?;
        if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
            return Err(AppError::validation(
                "Backup archive rejected symlink or special file",
                "backupPath",
            ));
        }
        entries.push((child_relative.clone(), file_type.is_dir()));
        if file_type.is_dir() {
            collect_archive_entries(root, &child_relative, entries)?;
        }
    }
    Ok(())
}

fn archive_header(path: &Path, is_directory: bool, size: u64) -> Result<tar::Header, AppError> {
    let mut header = tar::Header::new_gnu();
    header.set_path(path).map_err(redacted_storage_from)?;
    header.set_size(size);
    header.set_mode(if is_directory { 0o700 } else { 0o600 });
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    if is_directory {
        header.set_entry_type(tar::EntryType::Directory);
    }
    header.set_cksum();
    Ok(header)
}

pub fn create_tar_archive(root: &Path) -> Result<Vec<u8>, AppError> {
    let mut entries = Vec::new();
    collect_archive_entries(root, Path::new(""), &mut entries)?;

    let mut buffer = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut buffer);
        for (relative, is_directory) in entries {
            if is_directory {
                let header = archive_header(&relative, true, 0)?;
                builder.append(&header, io::empty())?;
            } else {
                let source = root.join(&relative);
                let metadata = fs::symlink_metadata(&source)?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(AppError::validation(
                        "Backup archive rejected symlink or special file",
                        "backupPath",
                    ));
                }
                let header = archive_header(&relative, false, metadata.len())?;
                builder.append(&header, fs::File::open(source)?)?;
            }
        }
        builder.finish()?;
    }
    Ok(buffer)
}

pub fn extract_tar_archive(bytes: &[u8], destination: &Path) -> Result<(), AppError> {
    if bytes.len() > MAX_TAR_ARCHIVE_BYTES {
        return Err(AppError::validation(
            "Backup archive exceeds maximum allowed size",
            "backupPath",
        ));
    }
    std::fs::create_dir_all(destination)?;
    let dest_root = destination
        .canonicalize()
        .unwrap_or_else(|_| destination.to_path_buf());

    let mut archive = tar::Archive::new(bytes);
    let entries = archive
        .entries()
        .map_err(redacted_storage_from)?;

    for (entry_count, entry) in entries.enumerate() {
        if entry_count >= MAX_TAR_ENTRIES {
            return Err(AppError::validation(
                "Backup archive contains too many entries",
                "backupPath",
            ));
        }

        let mut entry = entry.map_err(redacted_storage_from)?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(AppError::validation(
                "Backup archive rejected symlink or special file",
                "backupPath",
            ));
        }

        let entry_path = entry
            .path()
            .map_err(redacted_storage_from)?
            .into_owned();

        if entry_path.is_absolute()
            || entry_path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(AppError::validation(
                "Backup archive entry path is invalid",
                "backupPath",
            ));
        }

        let out_path = dest_root.join(&entry_path);
        let parent = out_path
            .parent()
            .ok_or_else(|| AppError::validation("Backup archive entry path is invalid", "backupPath"))?;
        let parent = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());
        if !parent.starts_with(&dest_root) {
            return Err(AppError::validation(
                "Backup archive entry escapes destination",
                "backupPath",
            ));
        }

        entry
            .unpack_in(&dest_root)
            .map_err(redacted_storage_from)?;
    }
    Ok(())
}

pub fn backup_plaintext_is_sqlite(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SQLite format 3")
}
