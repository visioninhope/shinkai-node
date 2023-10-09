use super::{db::Topic, db_errors::ShinkaiDBError, ShinkaiDB};
use crate::schemas::identity::{DeviceIdentity, Identity, IdentityType, StandardIdentity, StandardIdentityType};
use chrono::Utc;
use ed25519_dalek::{PublicKey as SignaturePublicKey, SecretKey as SignatureStaticKey};
use mupdf::device;
use rocksdb::{Error, IteratorMode, Options, WriteBatch};
use serde_json::to_vec;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::shinkai_message::shinkai_message_schemas::IdentityPermissions;
use shinkai_message_primitives::shinkai_utils::encryption::{
    encryption_public_key_to_string, encryption_public_key_to_string_ref, string_to_encryption_public_key,
};
use shinkai_message_primitives::shinkai_utils::signatures::{
    signature_public_key_to_string, signature_public_key_to_string_ref, string_to_signature_public_key,
};
use x25519_dalek::{PublicKey as EncryptionPublicKey, StaticSecret as EncryptionStaticKey};

impl ShinkaiDB {
    pub fn write_symmetric_key(&self, hex_blake3_hash: &str, private_key: &[u8]) -> Result<(), ShinkaiDBError> {
        // Get the ColumnFamily handle for MessageBoxSymmetricKeys
        let cf =
            self.db
                .cf_handle(Topic::MessageBoxSymmetricKeys.as_str())
                .ok_or(ShinkaiDBError::ColumnFamilyNotFound(
                    Topic::MessageBoxSymmetricKeys.as_str().to_string(),
                ))?;

        // Write the private key to the database with the public key as the key
        self.db
            .put_cf(cf, hex_blake3_hash, private_key)
            .map_err(|_| ShinkaiDBError::FailedFetchingValue)
    }

    pub fn read_symmetric_key(&self, hex_blake3_hash: &str) -> Result<Vec<u8>, ShinkaiDBError> {
        // Get the ColumnFamily handle for MessageBoxSymmetricKeys
        let cf =
            self.db
                .cf_handle(Topic::MessageBoxSymmetricKeys.as_str())
                .ok_or(ShinkaiDBError::ColumnFamilyNotFound(
                    Topic::MessageBoxSymmetricKeys.as_str().to_string(),
                ))?;

        // Read the private key from the database using the public key
        match self.db.get_cf(cf, hex_blake3_hash)? {
            Some(private_key) => Ok(private_key),
            None => Err(ShinkaiDBError::DataNotFound),
        }
    }

    // TODO: Use ProfileBatching so it's associated with a specific profile
    pub fn create_files_message_inbox(&mut self, hex_blake3_has: String) -> Result<(), Error> {
        // Create Options for ColumnFamily
        let mut cf_opts = Options::default();
        cf_opts.create_if_missing(true);
        cf_opts.create_missing_column_families(true);

        // Create ColumnFamilyDescriptors for encrypted inbox
        let cf_name_encrypted_inbox = hex_blake3_has.clone();

        // Create column families
        self.db.create_cf(&cf_name_encrypted_inbox, &cf_opts)?;

        // Start a write batch
        let mut batch = WriteBatch::default();

        // Add encrypted inbox name to the list in the 'inbox' topic
        let cf_inbox = self
            .db
            .cf_handle(Topic::TempFilesInbox.as_str())
            .expect("to be able to access Topic::TempFilesInbox");
        batch.put_cf(cf_inbox, &hex_blake3_has, &cf_name_encrypted_inbox);

        // Add current time to MessageBoxSymmetricKeysTimes with public_key as the key
        let current_time = Utc::now().to_rfc3339();
        let cf_times = self
            .db
            .cf_handle(Topic::MessageBoxSymmetricKeysTimes.as_str())
            .expect("to be able to access Topic::MessageBoxSymmetricKeysTimes");
        batch.put_cf(cf_times, &hex_blake3_has, current_time.as_bytes());

        // Commit the write batch
        self.db.write(batch)?;

        Ok(())
    }

    pub fn add_file_to_files_message_inbox(
        &mut self,
        hex_blake3_hash: String,
        file_name: String,
        file_content: Vec<u8>,
    ) -> Result<(), ShinkaiDBError> {
        // Get the name of the encrypted inbox from the 'inbox' topic
        let cf_inbox = self
            .db
            .cf_handle(Topic::TempFilesInbox.as_str())
            .expect("to be able to access Topic::TempFilesInbox");
        let cf_name_encrypted_inbox = self
            .db
            .get_cf(cf_inbox, &hex_blake3_hash)?
            .ok_or(ShinkaiDBError::InboxNotFound(hex_blake3_hash.clone()))?;

        // Check if the column family exists
        let cf_name_encrypted_inbox_str =
            std::str::from_utf8(&cf_name_encrypted_inbox).map_err(|_| ShinkaiDBError::DataNotFound)?; // handle the error appropriately

        if self.db.cf_handle(cf_name_encrypted_inbox_str).is_none() {
            return Err(ShinkaiDBError::InboxNotFound(cf_name_encrypted_inbox_str.to_string()));
        }

        // Start a write batch
        let mut batch = WriteBatch::default();

        // Add the file to the encrypted inbox
        let cf_encrypted_inbox = self
            .db
            .cf_handle(&cf_name_encrypted_inbox_str)
            .ok_or(ShinkaiDBError::FailedFetchingCF)?;
        batch.put_cf(cf_encrypted_inbox, &file_name, &file_content);

        // Commit the write batch
        self.db.write(batch).map_err(|_| ShinkaiDBError::FailedFetchingValue)?;

        Ok(())
    }

    pub fn get_all_files_from_inbox(&self, hex_blake3_hash: String) -> Result<Vec<(String, Vec<u8>)>, ShinkaiDBError> {
        // Get the name of the encrypted inbox from the 'inbox' topic
        let cf_inbox = self
            .db
            .cf_handle(Topic::TempFilesInbox.as_str())
            .expect("to be able to access Topic::TempFilesInbox");
        let cf_name_encrypted_inbox = self
            .db
            .get_cf(cf_inbox, &hex_blake3_hash)?
            .ok_or(ShinkaiDBError::InboxNotFound(hex_blake3_hash.clone()))?;

        // Check if the column family exists
        let cf_name_encrypted_inbox_str =
            std::str::from_utf8(&cf_name_encrypted_inbox).map_err(|_| ShinkaiDBError::DataNotFound)?; // handle the error appropriately

        if self.db.cf_handle(cf_name_encrypted_inbox_str).is_none() {
            return Err(ShinkaiDBError::InboxNotFound(cf_name_encrypted_inbox_str.to_string()));
        }

        // Get an iterator over the column family
        let cf_encrypted_inbox = self
            .db
            .cf_handle(&cf_name_encrypted_inbox_str)
            .ok_or(ShinkaiDBError::FailedFetchingCF)?;
        let iter = self.db.iterator_cf(cf_encrypted_inbox, IteratorMode::Start);

        // Collect all key-value pairs in the column family
        let files: Result<Vec<(String, Vec<u8>)>, _> = iter
            .map(|res| res.map(|(key, value)| (String::from_utf8(key.to_vec()).unwrap(), value.to_vec())))
            .collect();

        files.map_err(|_| ShinkaiDBError::FailedFetchingValue)
    }

    pub fn get_file_from_inbox(&self, hex_blake3_hash: String, file_name: String) -> Result<Vec<u8>, ShinkaiDBError> {
        // Get the name of the encrypted inbox from the 'inbox' topic
        let cf_inbox = self
            .db
            .cf_handle(Topic::TempFilesInbox.as_str())
            .expect("to be able to access Topic::TempFilesInbox");
        let cf_name_encrypted_inbox = self
            .db
            .get_cf(cf_inbox, &hex_blake3_hash)?
            .ok_or(ShinkaiDBError::InboxNotFound(hex_blake3_hash.clone()))?;

        // Check if the column family exists
        let cf_name_encrypted_inbox_str =
            std::str::from_utf8(&cf_name_encrypted_inbox).map_err(|_| ShinkaiDBError::DataNotFound)?; // handle the error appropriately

        if self.db.cf_handle(cf_name_encrypted_inbox_str).is_none() {
            return Err(ShinkaiDBError::InboxNotFound(cf_name_encrypted_inbox_str.to_string()));
        }

        // Get the file from the column family
        let cf_encrypted_inbox = self
            .db
            .cf_handle(&cf_name_encrypted_inbox_str)
            .ok_or(ShinkaiDBError::FailedFetchingCF)?;
        let file_content = self.db.get_cf(cf_encrypted_inbox, file_name)?;

        file_content.ok_or(ShinkaiDBError::DataNotFound)
    }
}
