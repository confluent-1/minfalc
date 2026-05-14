use std::fmt;

use fedimint_core::core::{Decoder, ModuleInstanceId, ModuleKind};
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::module::{AmountUnit, CommonModuleInit, ModuleCommon, ModuleConsensusVersion};
use fedimint_core::secp256k1::PublicKey;
use fedimint_core::{Amount, plugin_types_trait_impl_common};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod config;

pub const KIND: ModuleKind = ModuleKind::from_static_str("falcon-transfer");
pub const MODULE_CONSENSUS_VERSION: ModuleConsensusVersion = ModuleConsensusVersion::new(1, 0);

/// Compute the 4-byte account_id for a key (first 4 bytes of sha256).
pub fn account_id_from_key(key_bytes: &[u8]) -> u32 {
    use fedimint_core::bitcoin::hashes::{Hash, sha256};
    let hash = sha256::Hash::hash(key_bytes);
    u32::from_le_bytes(hash.to_byte_array()[0..4].try_into().expect("4 bytes"))
}

/// A transfer input spending from an account registered with the federation.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, Encodable, Decodable)]
pub struct FalconInput {
    pub amount: Amount,
    pub unit: AmountUnit,
    /// secp256k1 placeholder for InputMeta — real auth is Falcon at tx level.
    pub pub_key: PublicKey,
    /// 4-byte account identifier (registered via FalconOutput::Register).
    pub account_id: u32,
}

/// A module output — either a value transfer or a one-time key registration.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, Encodable, Decodable)]
pub enum FalconOutput {
    /// Transfer value to a registered account.
    Transfer {
        amount: Amount,
        unit: AmountUnit,
        /// Recipient account (registered via Register output).
        recipient_account_id: u32,
    },
    /// Register a Falcon-512 (or secp) public key, getting back a 4-byte account_id.
    /// One-time cost: ~897 bytes for Falcon keys, 33 bytes for secp.
    Register {
        pub_key_bytes: Vec<u8>,
    },
}

impl FalconOutput {
    pub fn amount(&self) -> Amount {
        match self {
            FalconOutput::Transfer { amount, .. } => *amount,
            FalconOutput::Register { .. } => Amount::ZERO,
        }
    }
}

/// Outcome returned after an output is processed.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, Encodable, Decodable)]
pub enum FalconOutputOutcome {
    Transfer,
    /// The account_id assigned to the registered key.
    Register { account_id: u32 },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, Encodable, Decodable)]
pub struct FalconConsensusItem;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Error, Encodable, Decodable)]
pub enum FalconInputError {
    #[error("UTXO not found")]
    UtxoNotFound,
    #[error("Not enough funds")]
    NotEnoughFunds,
    #[error("Account not found: {0}")]
    AccountNotFound(u32),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Error, Encodable, Decodable)]
pub enum FalconOutputError {}

pub struct FalconModuleTypes;

plugin_types_trait_impl_common!(
    KIND,
    FalconModuleTypes,
    config::FalconClientConfig,
    FalconInput,
    FalconOutput,
    FalconOutputOutcome,
    FalconConsensusItem,
    FalconInputError,
    FalconOutputError
);

#[derive(Debug)]
pub struct FalconCommonInit;

impl CommonModuleInit for FalconCommonInit {
    const CONSENSUS_VERSION: ModuleConsensusVersion = MODULE_CONSENSUS_VERSION;
    const KIND: ModuleKind = KIND;

    type ClientConfig = config::FalconClientConfig;

    fn decoder() -> Decoder {
        FalconModuleTypes::decoder_builder().build()
    }
}

impl fmt::Display for config::FalconClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FalconClientConfig")
    }
}
impl fmt::Display for FalconInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FalconInput {} (account {})", self.amount, self.account_id)
    }
}
impl fmt::Display for FalconOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FalconOutput::Transfer { amount, recipient_account_id, .. } => {
                write!(f, "FalconOutput::Transfer {} → account {}", amount, recipient_account_id)
            }
            FalconOutput::Register { pub_key_bytes } => {
                write!(f, "FalconOutput::Register ({} bytes)", pub_key_bytes.len())
            }
        }
    }
}
impl fmt::Display for FalconOutputOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FalconOutputOutcome::Transfer => write!(f, "FalconOutputOutcome::Transfer"),
            FalconOutputOutcome::Register { account_id } => {
                write!(f, "FalconOutputOutcome::Register({})", account_id)
            }
        }
    }
}
impl fmt::Display for FalconConsensusItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FalconConsensusItem")
    }
}
