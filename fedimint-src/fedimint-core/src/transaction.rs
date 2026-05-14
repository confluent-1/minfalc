use std::fmt;

use bitcoin::hashes::Hash;
use bitcoin::hex::DisplayHex as _;
use fedimint_core::core::{DynInput, DynOutput};
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::module::SerdeModuleEncoding;
use fedimint_core::{Amount, TransactionId};
use pqcrypto_falcon::falcon512;
use pqcrypto_traits::sign::{DetachedSignature as _, PublicKey as _};
use thiserror::Error;

use crate::config::ALEPH_BFT_UNIT_BYTE_LIMIT;
use crate::core::{DynInputError, DynOutputError};

/// An atomic value transfer operation within the Fedimint system and consensus
///
/// The mint enforces that the total value of the outputs equals the total value
/// of the inputs, to prevent creating funds out of thin air. In some cases, the
/// value of the inputs and outputs can both be 0 e.g. when creating an offer to
/// a Lightning Gateway.
#[derive(Clone, Eq, PartialEq, Hash, Encodable, Decodable)]
pub struct Transaction {
    /// [`DynInput`]s consumed by the transaction
    pub inputs: Vec<DynInput>,
    /// [`DynOutput`]s created as a result of the transaction
    pub outputs: Vec<DynOutput>,
    /// No defined meaning, can be used to send the otherwise exactly same
    /// transaction multiple times if the module inputs and outputs don't
    /// introduce enough entropy.
    ///
    /// In the future the nonce can be used for grinding a tx hash that fulfills
    /// certain PoW requirements.
    pub nonce: [u8; 8],
    /// signatures for all the public keys of the inputs
    pub signatures: TransactionSignature,
}

impl fmt::Debug for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transaction")
            .field("txid", &self.tx_hash())
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .field("nonce", &self.nonce)
            .field("signatures", &self.signatures)
            .finish()
    }
}

pub type SerdeTransaction = SerdeModuleEncoding<Transaction>;

impl Transaction {
    /// Maximum size that a transaction can have while still fitting into an
    /// AlephBFT unit. Subtracting 32 bytes is overly conservative, even in the
    /// worst case the CI serialization around the transaction should never add
    /// that much overhead. But since the byte limit is 50kb right now a few
    /// bytes more or less won't make a difference and we can afford the safety
    /// margin.
    ///
    /// A realistic value would be 7:
    ///  * 1 byte for length of vector of CIs
    ///  * 1 byte for the CI enum variant
    ///  * 5 byte for the CI enum variant length
    pub const MAX_TX_SIZE: usize = ALEPH_BFT_UNIT_BYTE_LIMIT - 32;

    /// Hash of the transaction (excluding the signature).
    ///
    /// Transaction signature commits to this hash.
    /// To generate it without already having a signature use
    /// [`Self::tx_hash_from_parts`].
    pub fn tx_hash(&self) -> TransactionId {
        Self::tx_hash_from_parts(&self.inputs, &self.outputs, self.nonce)
    }

    /// Generate the transaction hash.
    pub fn tx_hash_from_parts(
        inputs: &[DynInput],
        outputs: &[DynOutput],
        nonce: [u8; 8],
    ) -> TransactionId {
        let mut engine = TransactionId::engine();
        inputs
            .consensus_encode(&mut engine)
            .expect("write to hash engine can't fail");
        outputs
            .consensus_encode(&mut engine)
            .expect("write to hash engine can't fail");
        nonce
            .consensus_encode(&mut engine)
            .expect("write to hash engine can't fail");
        TransactionId::from_engine(engine)
    }

    /// Validate the signatures (Schnorr or Falcon-512) signed over the `tx_hash`.
    ///
    /// For `NaiveMultisig`: verifies Schnorr signatures against the provided secp256k1 keys.
    /// For `FalconMultisig`: `falcon_keys[i]` must contain the Falcon-512 public key for
    /// input `i`, sourced from `InputMeta.falcon_pub_key`.
    pub fn validate_signatures(
        &self,
        pub_keys: &[secp256k1::PublicKey],
        falcon_keys: &[Option<Vec<u8>>],
    ) -> Result<(), TransactionError> {
        match &self.signatures {
            TransactionSignature::NaiveMultisig(sigs) => {
                if pub_keys.len() != sigs.len() {
                    return Err(TransactionError::InvalidWitnessLength);
                }
                let txid = self.tx_hash();
                let msg = secp256k1::Message::from_digest_slice(&txid[..])
                    .expect("txid has right length");
                for (pk, signature) in pub_keys.iter().zip(sigs) {
                    if secp256k1::global::SECP256K1
                        .verify_schnorr(signature, &msg, &pk.x_only_public_key().0)
                        .is_err()
                    {
                        return Err(TransactionError::InvalidSignature {
                            tx: self.consensus_encode_to_hex(),
                            hash: self.tx_hash().consensus_encode_to_hex(),
                            sig: signature.consensus_encode_to_hex(),
                            key: pk.consensus_encode_to_hex(),
                        });
                    }
                }
                Ok(())
            }
            TransactionSignature::FalconMultisig(pairs) => {
                if pairs.len() != falcon_keys.len() {
                    return Err(TransactionError::InvalidWitnessLength);
                }
                let txid = self.tx_hash();
                for (pair, maybe_key) in pairs.iter().zip(falcon_keys) {
                    let raw_key = maybe_key.as_deref().ok_or(TransactionError::InvalidWitnessLength)?;
                    let pk = falcon512::PublicKey::from_bytes(raw_key).map_err(|_| {
                        TransactionError::InvalidSignature {
                            tx: self.consensus_encode_to_hex(),
                            hash: self.tx_hash().consensus_encode_to_hex(),
                            sig: hex::encode(&pair.signature),
                            key: hex::encode(raw_key),
                        }
                    })?;
                    let sig =
                        falcon512::DetachedSignature::from_bytes(&pair.signature).map_err(|_| {
                            TransactionError::InvalidSignature {
                                tx: self.consensus_encode_to_hex(),
                                hash: self.tx_hash().consensus_encode_to_hex(),
                                sig: hex::encode(&pair.signature),
                                key: hex::encode(raw_key),
                            }
                        })?;
                    falcon512::verify_detached_signature(&sig, txid.as_ref(), &pk).map_err(
                        |_| TransactionError::InvalidSignature {
                            tx: self.consensus_encode_to_hex(),
                            hash: self.tx_hash().consensus_encode_to_hex(),
                            sig: hex::encode(&pair.signature),
                            key: hex::encode(raw_key),
                        },
                    )?;
                }
                Ok(())
            }
            TransactionSignature::Default { variant, .. } => {
                Err(TransactionError::UnsupportedSignatureScheme { variant: *variant })
            }
        }
    }
}

/// A Falcon-512 detached signature. The matching public key is carried in the
/// corresponding `FalconInput.falcon_pub_key` and passed via `InputMeta`.
#[derive(Clone, Eq, PartialEq, Hash, Encodable, Decodable)]
pub struct FalconSigWithKey {
    /// Raw Falcon-512 detached signature bytes (up to 809 bytes)
    pub signature: Vec<u8>,
}

#[derive(Clone, Eq, PartialEq, Hash, Encodable, Decodable)]
pub enum TransactionSignature {
    NaiveMultisig(Vec<fedimint_core::secp256k1::schnorr::Signature>),
    /// Post-quantum Falcon-512 signatures. Public keys come from `InputMeta.falcon_pub_key`.
    FalconMultisig(Vec<FalconSigWithKey>),
    #[encodable_default]
    Default {
        variant: u64,
        bytes: Vec<u8>,
    },
}

impl fmt::Debug for TransactionSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NaiveMultisig(multi) => {
                f.debug_struct("NaiveMultisig")
                    .field("len", &multi.len())
                    .finish()?;
            }
            Self::FalconMultisig(pairs) => {
                f.debug_struct("FalconMultisig")
                    .field("len", &pairs.len())
                    .finish()?;
            }
            Self::Default { variant, bytes } => {
                f.debug_struct(stringify!($name))
                    .field("variant", variant)
                    .field("bytes", &bytes.as_hex())
                    .finish()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error, Encodable, Decodable, Clone, Eq, PartialEq)]
pub enum TransactionError {
    /// Transaction was not balanced
    ///
    /// Note: since this type existed before multi-unit amounts were implemented
    /// and can't change shape, the unit of the imbalance is not specified.
    #[error("The transaction is unbalanced (in={inputs}, out={outputs}, fee={fee})")]
    UnbalancedTransaction {
        inputs: Amount,
        outputs: Amount,
        fee: Amount,
    },
    #[error("The transaction's signature is invalid: tx={tx}, hash={hash}, sig={sig}, key={key}")]
    InvalidSignature {
        tx: String,
        hash: String,
        sig: String,
        key: String,
    },
    #[error("The transaction's signature scheme is not supported: variant={variant}")]
    UnsupportedSignatureScheme { variant: u64 },
    #[error("The transaction did not have the correct number of signatures")]
    InvalidWitnessLength,
    #[error("The transaction had an invalid input: {}", .0)]
    Input(DynInputError),
    #[error("The transaction had an invalid output: {}", .0)]
    Output(DynOutputError),
}

/// The transaction caused an overflow.
///
/// We can't add a new variant to transaction errors, so we define a special
/// case for the retroactively added overflow error type. In a second iteration
/// of the transaction submission API this should become a separate error
/// variant.
pub const TRANSACTION_OVERFLOW_ERROR: TransactionError = TransactionError::UnbalancedTransaction {
    inputs: Amount::ZERO,
    outputs: Amount::ZERO,
    fee: Amount::ZERO,
};

#[derive(Debug, Encodable, Decodable, Clone, Eq, PartialEq)]
pub struct TransactionSubmissionOutcome(pub Result<TransactionId, TransactionError>);
