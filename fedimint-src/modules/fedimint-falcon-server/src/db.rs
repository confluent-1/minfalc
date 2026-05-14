use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::{Amount, InPoint, OutPoint, impl_db_lookup, impl_db_record};
use serde::Serialize;
use strum_macros::EnumIter;

#[repr(u8)]
#[derive(Clone, EnumIter, Debug)]
pub enum DbKeyPrefix {
    /// account_id → UTXO balance
    Output = 0x00,
    InputAudit = 0x01,
    OutputAudit = 0x02,
    /// account_id → raw public key bytes
    Account = 0x03,
}

impl std::fmt::Display for DbKeyPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// UTXO store: account_id → Amount
#[derive(Debug, Clone, Encodable, Decodable, Eq, PartialEq, Hash, Serialize)]
pub struct FalconOutputKey(pub u32);

#[derive(Debug, Encodable, Decodable)]
pub struct FalconOutputPrefix;

impl_db_record!(
    key = FalconOutputKey,
    value = Amount,
    db_prefix = DbKeyPrefix::Output,
);
impl_db_lookup!(key = FalconOutputKey, query_prefix = FalconOutputPrefix);

#[derive(Debug, Clone, Encodable, Decodable, Eq, PartialEq, Hash, Serialize)]
pub struct FalconInputAuditKey(pub InPoint);

#[derive(Debug, Encodable, Decodable)]
pub struct FalconInputAuditPrefix;

impl_db_record!(
    key = FalconInputAuditKey,
    value = Amount,
    db_prefix = DbKeyPrefix::InputAudit,
);
impl_db_lookup!(
    key = FalconInputAuditKey,
    query_prefix = FalconInputAuditPrefix
);

#[derive(Debug, Clone, Encodable, Decodable, Eq, PartialEq, Hash, Serialize)]
pub struct FalconOutputAuditKey(pub OutPoint);

#[derive(Debug, Encodable, Decodable)]
pub struct FalconOutputAuditPrefix;

impl_db_record!(
    key = FalconOutputAuditKey,
    value = Amount,
    db_prefix = DbKeyPrefix::OutputAudit,
);
impl_db_lookup!(
    key = FalconOutputAuditKey,
    query_prefix = FalconOutputAuditPrefix
);

/// Key registry: account_id → raw public key bytes (Falcon-512 or secp).
#[derive(Debug, Clone, Encodable, Decodable, Eq, PartialEq, Hash, Serialize)]
pub struct FalconAccountKey(pub u32);

#[derive(Debug, Encodable, Decodable)]
pub struct FalconAccountPrefix;

impl_db_record!(
    key = FalconAccountKey,
    value = Vec<u8>,
    db_prefix = DbKeyPrefix::Account,
);
impl_db_lookup!(key = FalconAccountKey, query_prefix = FalconAccountPrefix);
