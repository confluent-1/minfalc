/// Falcon-512 client helpers.
///
/// Provides utilities for the spammer binary to construct and sign Falcon-512
/// transfer transactions and register keypairs with the federation.
use fedimint_core::core::{DynInput, DynOutput, ModuleInstanceId};
use fedimint_core::secp256k1;
use fedimint_core::transaction::{FalconSigWithKey, Transaction, TransactionSignature};
use fedimint_falcon_common::{FalconInput, FalconOutput};
use pqcrypto_falcon::falcon512;
use pqcrypto_traits::sign::{DetachedSignature as _, PublicKey as _};
use rand::Rng;

pub use fedimint_falcon_common as common;
pub use fedimint_falcon_common::account_id_from_key;

/// Derive a deterministic secp256k1 placeholder public key from a raw key blob.
///
/// `InputMeta.pub_key` must be secp256k1. We hash the key bytes to get a stable
/// 32-byte seed and derive a key from it.
pub fn secp_placeholder_from_falcon(falcon_pk: &[u8]) -> secp256k1::PublicKey {
    use fedimint_core::bitcoin::hashes::{Hash, sha256};
    let hash = sha256::Hash::hash(falcon_pk);
    let secp = secp256k1::Secp256k1::new();
    let secret_key = secp256k1::SecretKey::from_slice(hash.as_ref())
        .expect("sha256 output is a valid secp256k1 scalar with overwhelming probability");
    secp256k1::PublicKey::from_secret_key(&secp, &secret_key)
}

/// Build a zero-input registration transaction for a Falcon-512 keypair.
///
/// The server will store `pub_key_bytes → account_id` upon processing.
/// Account ID is deterministic: `u32::from_le_bytes(sha256(pub_key_bytes)[0..4])`.
pub fn build_register_tx(
    module_instance_id: ModuleInstanceId,
    pub_key_bytes: Vec<u8>,
) -> Transaction {
    let output = FalconOutput::Register { pub_key_bytes };
    let dyn_output = DynOutput::from_typed(module_instance_id, output);
    let nonce: [u8; 8] = rand::thread_rng().r#gen();
    Transaction {
        inputs: vec![],
        outputs: vec![dyn_output],
        nonce,
        signatures: TransactionSignature::NaiveMultisig(vec![]),
    }
}

/// Sign a transaction with Falcon-512 keypairs (one per input).
///
/// Replaces any existing signature with a `FalconMultisig` variant.
pub fn sign_transaction_falcon(
    tx: &Transaction,
    keypairs: &[(falcon512::PublicKey, falcon512::SecretKey)],
) -> Transaction {
    let txid = tx.tx_hash();
    let pairs: Vec<FalconSigWithKey> = keypairs
        .iter()
        .map(|(_, sk)| {
            let sig = falcon512::detached_sign(txid.as_ref(), sk);
            FalconSigWithKey {
                signature: sig.as_bytes().to_vec(),
            }
        })
        .collect();

    Transaction {
        inputs: tx.inputs.clone(),
        outputs: tx.outputs.clone(),
        nonce: tx.nonce,
        signatures: TransactionSignature::FalconMultisig(pairs),
    }
}

/// Build a Falcon-512 transfer transaction using a pre-registered account_id.
pub fn build_transfer_tx_falcon(
    module_instance_id: ModuleInstanceId,
    amount: fedimint_core::Amount,
    keypair: &(falcon512::PublicKey, falcon512::SecretKey),
) -> Transaction {
    let falcon_pk_bytes = keypair.0.as_bytes().to_vec();
    let account_id = account_id_from_key(&falcon_pk_bytes);
    let secp_key = secp_placeholder_from_falcon(&falcon_pk_bytes);

    let input = FalconInput {
        amount,
        unit: fedimint_core::module::AmountUnit::BITCOIN,
        pub_key: secp_key,
        account_id,
    };
    let output = FalconOutput::Transfer {
        amount,
        unit: fedimint_core::module::AmountUnit::BITCOIN,
        recipient_account_id: account_id,
    };

    let dyn_input = DynInput::from_typed(module_instance_id, input);
    let dyn_output = DynOutput::from_typed(module_instance_id, output);
    let nonce: [u8; 8] = rand::thread_rng().r#gen();

    let unsigned = Transaction {
        inputs: vec![dyn_input],
        outputs: vec![dyn_output],
        nonce,
        signatures: TransactionSignature::FalconMultisig(vec![]),
    };

    sign_transaction_falcon(&unsigned, std::slice::from_ref(keypair))
}

/// Approximate bytes per Falcon-512 transfer transaction after Phase B.
/// Input: ~4B account_id + overhead; Output: ~4B account_id; Sig: ~690B.
pub const FALCON512_TRANSFER_BYTES_APPROX: usize = 750;
