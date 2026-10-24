//! Thin OpenMLS facade for small OMCP groups (≤32) — PROTOCOL §5.1 / §6.2.
//!
//! Ciphersuite: `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`.
//! Application messages are encrypted under MLS epoch secrets via OpenMLS.
//! PQ hybrid mix is layered via [`super::mls_pq`] (provisional; see extensions/mls-pq.md).

use openmls::prelude::tls_codec::Deserialize;
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;

use crate::error::{OmcpError, Result};
use crate::mls::mls_pq::{
    encapsulate_to_members, mix_exporter, MlsPqKemKeyPair, MLS_PQ_EXPORTER_LABEL,
};

/// RFC 9420 suite required by PROTOCOL §5.1.
pub const OMCP_MLS_CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// Soft cap for M3 small groups (PROTOCOL milestone text).
pub const MAX_SMALL_GROUP_MEMBERS: usize = 32;

/// One peer’s MLS + OMCP-MLS-PQ state.
/// `(extension_bytes, kem_shared_secrets, hybrid_exporter)` for one epoch.
pub type PqEpochSecrets = (Vec<u8>, Vec<[u8; 32]>, [u8; 32]);

pub struct OmcpMlsMember {
    pub identity: Vec<u8>,
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    credential_with_key: CredentialWithKey,
    /// Key package for others to add this member (refreshed after consume).
    key_package: Option<KeyPackageBundle>,
    group: Option<MlsGroup>,
    /// ML-KEM key for OMCP-MLS-PQ (out-of-band with credential).
    pub pq_kem: MlsPqKemKeyPair,
    join_config: MlsGroupJoinConfig,
}

impl OmcpMlsMember {
    /// Create a member with a fresh credential, signature keys, key package, and PQ KEM.
    pub fn generate(identity: impl Into<Vec<u8>>) -> Result<Self> {
        let identity = identity.into();
        let provider = OpenMlsRustCrypto::default();
        let credential = BasicCredential::new(identity.clone());
        let signer = SignatureKeyPair::new(OMCP_MLS_CIPHERSUITE.signature_algorithm())
            .map_err(|_| OmcpError::Mls("signature keygen"))?;
        signer
            .store(provider.storage())
            .map_err(|_| OmcpError::Mls("store signer"))?;
        let credential_with_key = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.to_public_vec().into(),
        };
        let key_package = KeyPackage::builder()
            .build(
                OMCP_MLS_CIPHERSUITE,
                &provider,
                &signer,
                credential_with_key.clone(),
            )
            .map_err(|_| OmcpError::Mls("key package"))?;
        let join_config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();
        Ok(Self {
            identity,
            provider,
            signer,
            credential_with_key,
            key_package: Some(key_package),
            group: None,
            pq_kem: MlsPqKemKeyPair::generate()?,
            join_config,
        })
    }

    /// Create a new MLS group with this member as sole creator.
    pub fn create_group(&mut self) -> Result<()> {
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(OMCP_MLS_CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build();
        let group = MlsGroup::new(
            &self.provider,
            &self.signer,
            &config,
            self.credential_with_key.clone(),
        )
        .map_err(|_| OmcpError::Mls("create group"))?;
        self.group = Some(group);
        Ok(())
    }

    fn group_ref(&self) -> Result<&MlsGroup> {
        self.group.as_ref().ok_or(OmcpError::Mls("no group"))
    }

    /// Current epoch number.
    pub fn epoch(&self) -> Result<u64> {
        Ok(self.group_ref()?.epoch().as_u64())
    }

    /// Number of members in the local group state.
    pub fn member_count(&self) -> Result<usize> {
        Ok(self.group_ref()?.members().count())
    }

    /// Own leaf index.
    pub fn own_leaf_index(&self) -> Result<u32> {
        Ok(self.group_ref()?.own_leaf_index().u32())
    }

    /// Take (and clear) this member’s current key package for an Add.
    pub fn take_key_package(&mut self) -> Result<KeyPackage> {
        let bundle = self
            .key_package
            .take()
            .ok_or(OmcpError::Mls("key package already used"))?;
        Ok(bundle.key_package().clone())
    }

    /// Refresh key package after the previous one was consumed.
    pub fn refresh_key_package(&mut self) -> Result<()> {
        let key_package = KeyPackage::builder()
            .build(
                OMCP_MLS_CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential_with_key.clone(),
            )
            .map_err(|_| OmcpError::Mls("refresh key package"))?;
        self.key_package = Some(key_package);
        Ok(())
    }

    /// Add members by key package; returns serialized commit + welcome bytes.
    pub fn add_members(&mut self, key_packages: &[KeyPackage]) -> Result<(Vec<u8>, Vec<u8>)> {
        if key_packages.is_empty() {
            return Err(OmcpError::Mls("no key packages"));
        }
        let (commit, welcome) = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            if group.members().count() + key_packages.len() > MAX_SMALL_GROUP_MEMBERS {
                return Err(OmcpError::Mls("group would exceed 32"));
            }
            let (commit, welcome, _group_info) = group
                .add_members(&self.provider, &self.signer, key_packages)
                .map_err(|_| OmcpError::Mls("add_members"))?;
            (commit, welcome)
        };
        self.group
            .as_mut()
            .unwrap()
            .merge_pending_commit(&self.provider)
            .map_err(|_| OmcpError::Mls("merge add commit"))?;
        let commit_bytes = commit
            .to_bytes()
            .map_err(|_| OmcpError::Mls("serialize commit"))?;
        let welcome_bytes = welcome
            .to_bytes()
            .map_err(|_| OmcpError::Mls("serialize welcome"))?;
        Ok((commit_bytes, welcome_bytes))
    }

    /// Join from a Welcome message (serialized `MlsMessageOut` bytes).
    pub fn join_from_welcome(&mut self, welcome_bytes: &[u8]) -> Result<()> {
        let msg = MlsMessageIn::tls_deserialize_exact(welcome_bytes)
            .map_err(|_| OmcpError::Mls("welcome deserialize"))?;
        let welcome = match msg.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err(OmcpError::Mls("not a welcome")),
        };
        let staged =
            StagedWelcome::new_from_welcome(&self.provider, &self.join_config, welcome, None)
                .map_err(|_| OmcpError::Mls("staged welcome"))?;
        let group = staged
            .into_group(&self.provider)
            .map_err(|_| OmcpError::Mls("into group"))?;
        self.group = Some(group);
        Ok(())
    }

    /// Process a commit/proposal from another member (serialized MLS message).
    pub fn process_protocol_message(&mut self, bytes: &[u8]) -> Result<()> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(|_| OmcpError::Mls("message deserialize"))?;
        let protocol = msg
            .try_into_protocol_message()
            .map_err(|_| OmcpError::Mls("not protocol message"))?;
        let processed = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            group
                .process_message(&self.provider, protocol)
                .map_err(|_| OmcpError::Mls("process message"))?
        };
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                self.group
                    .as_mut()
                    .unwrap()
                    .merge_staged_commit(&self.provider, *staged)
                    .map_err(|_| OmcpError::Mls("merge staged"))?;
            }
            ProcessedMessageContent::ApplicationMessage(_) => {
                return Err(OmcpError::Mls("expected commit, got app"));
            }
            ProcessedMessageContent::ProposalMessage(_)
            | ProcessedMessageContent::ExternalJoinProposalMessage(_)
            | ProcessedMessageContent::OwnPendingCommit
            | ProcessedMessageContent::OwnPrivateMessage => {}
        }
        Ok(())
    }

    /// Remove members by leaf index; returns serialized commit (no welcome).
    pub fn remove_members(&mut self, leaf_indices: &[u32]) -> Result<Vec<u8>> {
        let indices: Vec<LeafNodeIndex> = leaf_indices
            .iter()
            .map(|&i| LeafNodeIndex::new(i))
            .collect();
        let commit = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            let (commit, _welcome, _info) = group
                .remove_members(&self.provider, &self.signer, &indices)
                .map_err(|_| OmcpError::Mls("remove_members"))?;
            commit
        };
        self.group
            .as_mut()
            .unwrap()
            .merge_pending_commit(&self.provider)
            .map_err(|_| OmcpError::Mls("merge remove"))?;
        commit
            .to_bytes()
            .map_err(|_| OmcpError::Mls("serialize remove commit"))
    }

    /// Encrypt an application plaintext under the current MLS epoch.
    pub fn encrypt_app_message(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
        let msg = group
            .create_message(&self.provider, &self.signer, plaintext)
            .map_err(|_| OmcpError::Mls("create_message"))?;
        msg.to_bytes()
            .map_err(|_| OmcpError::Mls("serialize app message"))
    }

    /// Decrypt an application message; returns plaintext bytes.
    pub fn decrypt_app_message(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(|_| OmcpError::Mls("app deserialize"))?;
        let protocol = msg
            .try_into_protocol_message()
            .map_err(|_| OmcpError::Mls("not protocol message"))?;
        let processed = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            group
                .process_message(&self.provider, protocol)
                .map_err(|_| OmcpError::Mls("process app"))?
        };
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => Ok(app.into_bytes()),
            _ => Err(OmcpError::Mls("expected application message")),
        }
    }

    /// Export MLS secret then mix OMCP-MLS-PQ shared secrets (PROTOCOL §6.2).
    pub fn hybrid_exporter(&self, kem_shared_secrets: &[[u8; 32]]) -> Result<[u8; 32]> {
        let group = self.group_ref()?;
        let epoch = group.epoch().as_u64();
        let context = epoch.to_be_bytes();
        let mls_exporter = group
            .export_secret(self.provider.crypto(), MLS_PQ_EXPORTER_LABEL, &context, 32)
            .map_err(|_| OmcpError::Mls("export_secret"))?;
        mix_exporter(&mls_exporter, kem_shared_secrets)
    }

    /// Build PQ encapsulations to `others` for the current epoch.
    ///
    /// Returns `(extension_bytes, kem_shared_secrets, hybrid_exporter)`.
    pub fn build_pq_epoch_secrets(
        &self,
        others: &[(u32, [u8; crate::handshake::MLKEM768_EK_LEN])],
    ) -> Result<PqEpochSecrets> {
        let epoch = self.epoch()?;
        let (ext, secrets) = encapsulate_to_members(epoch, others)?;
        let hybrid = self.hybrid_exporter(&secrets)?;
        Ok((ext.encode()?, secrets, hybrid))
    }

    /// Collect (leaf_index, identity) for debugging/tests.
    pub fn member_identities(&self) -> Result<Vec<(u32, Vec<u8>)>> {
        let g = self.group_ref()?;
        Ok(g.members()
            .map(|m| (m.index.u32(), m.credential.serialized_content().to_vec()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_add_remove_encrypt_three_members() {
        let mut alice = OmcpMlsMember::generate(b"alice".to_vec()).unwrap();
        let mut bob = OmcpMlsMember::generate(b"bob".to_vec()).unwrap();
        let mut charlie = OmcpMlsMember::generate(b"charlie".to_vec()).unwrap();

        alice.create_group().unwrap();
        assert_eq!(alice.member_count().unwrap(), 1);

        let bob_kp = bob.take_key_package().unwrap();
        let (commit1, welcome_bob) = alice.add_members(&[bob_kp]).unwrap();
        bob.join_from_welcome(&welcome_bob).unwrap();
        assert_eq!(alice.member_count().unwrap(), 2);
        assert_eq!(bob.member_count().unwrap(), 2);

        let charlie_kp = charlie.take_key_package().unwrap();
        let (commit2, welcome_charlie) = alice.add_members(&[charlie_kp]).unwrap();
        bob.process_protocol_message(&commit2).unwrap();
        charlie.join_from_welcome(&welcome_charlie).unwrap();
        assert_eq!(alice.member_count().unwrap(), 3);
        assert_eq!(bob.member_count().unwrap(), 3);
        assert_eq!(charlie.member_count().unwrap(), 3);

        let ct = alice.encrypt_app_message(b"hello MLS group").unwrap();
        let p_b = bob.decrypt_app_message(&ct).unwrap();
        let p_c = charlie.decrypt_app_message(&ct).unwrap();
        assert_eq!(p_b, b"hello MLS group");
        assert_eq!(p_c, b"hello MLS group");

        let others = [
            (bob.own_leaf_index().unwrap(), *bob.pq_kem.public_bytes()),
            (
                charlie.own_leaf_index().unwrap(),
                *charlie.pq_kem.public_bytes(),
            ),
        ];
        let (_ext, secrets, hybrid) = alice.build_pq_epoch_secrets(&others).unwrap();
        let classical = alice.hybrid_exporter(&[]).unwrap();
        assert_ne!(hybrid, classical);
        assert_eq!(secrets.len(), 2);

        let charlie_leaf = charlie.own_leaf_index().unwrap();
        let rem = alice.remove_members(&[charlie_leaf]).unwrap();
        bob.process_protocol_message(&rem).unwrap();
        assert_eq!(alice.member_count().unwrap(), 2);
        assert_eq!(bob.member_count().unwrap(), 2);

        let _ = commit1;
    }
}
