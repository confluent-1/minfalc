use std::collections::BTreeMap;

use async_trait::async_trait;
use fedimint_core::config::{
    ServerModuleConfig, ServerModuleConsensusConfig, TypedServerModuleConfig,
};
use fedimint_core::core::ModuleInstanceId;
use fedimint_core::db::{DatabaseTransaction, DatabaseVersion, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::module::audit::Audit;
use fedimint_core::module::{
    Amounts, ApiEndpoint, CORE_CONSENSUS_VERSION, CoreConsensusVersion, InputMeta,
    ModuleConsensusVersion, ModuleInit, SupportedModuleApiVersions, TransactionItemAmounts,
};
use fedimint_core::{Amount, InPoint, OutPoint, PeerId, push_db_pair_items};
pub use fedimint_falcon_common as common;
use fedimint_falcon_common::config::{
    FalconClientConfig, FalconConfig, FalconConfigConsensus, FalconConfigPrivate,
};
use fedimint_falcon_common::{
    FalconCommonInit, FalconConsensusItem, FalconInput, FalconInputError, FalconModuleTypes,
    FalconOutput, FalconOutputError, FalconOutputOutcome, MODULE_CONSENSUS_VERSION,
    account_id_from_key,
};
use fedimint_server_core::config::PeerHandleOps;
use fedimint_server_core::migration::ServerModuleDbMigrationFn;
use fedimint_server_core::{
    ConfigGenModuleArgs, ServerModule, ServerModuleInit, ServerModuleInitArgs,
};
use futures::StreamExt;
use strum::IntoEnumIterator;

pub mod db;
use db::{
    DbKeyPrefix, FalconAccountKey, FalconInputAuditKey, FalconInputAuditPrefix,
    FalconOutputAuditKey, FalconOutputAuditPrefix, FalconOutputKey,
};

#[derive(Debug, Clone)]
pub struct FalconInit;

impl ModuleInit for FalconInit {
    type Common = FalconCommonInit;

    async fn dump_database(
        &self,
        dbtx: &mut DatabaseTransaction<'_>,
        prefix_names: Vec<String>,
    ) -> Box<dyn Iterator<Item = (String, Box<dyn erased_serde::Serialize + Send>)> + '_> {
        let mut items: BTreeMap<String, Box<dyn erased_serde::Serialize + Send>> = BTreeMap::new();
        let filtered_prefixes = DbKeyPrefix::iter().filter(|f| {
            prefix_names.is_empty() || prefix_names.contains(&f.to_string().to_lowercase())
        });
        for table in filtered_prefixes {
            match table {
                DbKeyPrefix::Output => {}
                DbKeyPrefix::Account => {}
                DbKeyPrefix::InputAudit => {
                    push_db_pair_items!(
                        dbtx,
                        FalconInputAuditPrefix,
                        FalconInputAuditKey,
                        Amount,
                        items,
                        "Falcon Input Audit"
                    );
                }
                DbKeyPrefix::OutputAudit => {
                    push_db_pair_items!(
                        dbtx,
                        FalconOutputAuditPrefix,
                        FalconOutputAuditKey,
                        Amount,
                        items,
                        "Falcon Output Audit"
                    );
                }
            }
        }
        Box::new(items.into_iter())
    }
}

#[async_trait]
impl ServerModuleInit for FalconInit {
    type Module = FalconModule;

    fn versions(&self, _core: CoreConsensusVersion) -> &[ModuleConsensusVersion] {
        &[MODULE_CONSENSUS_VERSION]
    }

    fn supported_api_versions(&self) -> SupportedModuleApiVersions {
        SupportedModuleApiVersions::from_raw(
            (CORE_CONSENSUS_VERSION.major, CORE_CONSENSUS_VERSION.minor),
            (
                MODULE_CONSENSUS_VERSION.major,
                MODULE_CONSENSUS_VERSION.minor,
            ),
            &[(0, 0)],
        )
    }

    async fn init(&self, args: &ServerModuleInitArgs<Self>) -> anyhow::Result<Self::Module> {
        Ok(FalconModule::new(args.cfg().to_typed()?))
    }

    fn trusted_dealer_gen(
        &self,
        peers: &[PeerId],
        _args: &ConfigGenModuleArgs,
    ) -> BTreeMap<PeerId, ServerModuleConfig> {
        peers
            .iter()
            .map(|&peer| {
                let config = FalconConfig {
                    private: FalconConfigPrivate,
                    consensus: FalconConfigConsensus,
                };
                (peer, config.to_erased())
            })
            .collect()
    }

    async fn distributed_gen(
        &self,
        _peers: &(dyn PeerHandleOps + Send + Sync),
        _args: &ConfigGenModuleArgs,
    ) -> anyhow::Result<ServerModuleConfig> {
        Ok(FalconConfig {
            private: FalconConfigPrivate,
            consensus: FalconConfigConsensus,
        }
        .to_erased())
    }

    fn get_client_config(
        &self,
        _config: &ServerModuleConsensusConfig,
    ) -> anyhow::Result<FalconClientConfig> {
        Ok(FalconClientConfig)
    }

    fn validate_config(
        &self,
        _identity: &PeerId,
        _config: ServerModuleConfig,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_database_migrations(
        &self,
    ) -> BTreeMap<DatabaseVersion, ServerModuleDbMigrationFn<FalconModule>> {
        BTreeMap::new()
    }
}

#[derive(Debug)]
pub struct FalconModule {
    pub cfg: FalconConfig,
}

impl FalconModule {
    pub fn new(cfg: FalconConfig) -> Self {
        Self { cfg }
    }
}

#[async_trait]
impl ServerModule for FalconModule {
    type Common = FalconModuleTypes;
    type Init = FalconInit;

    async fn consensus_proposal(
        &self,
        _dbtx: &mut DatabaseTransaction<'_>,
    ) -> Vec<FalconConsensusItem> {
        Vec::new()
    }

    async fn process_consensus_item<'a, 'b>(
        &'a self,
        _dbtx: &mut DatabaseTransaction<'b>,
        _consensus_item: FalconConsensusItem,
        _peer_id: PeerId,
    ) -> anyhow::Result<()> {
        anyhow::bail!("The falcon-transfer module does not use consensus items");
    }

    async fn process_input<'a, 'b, 'c>(
        &'a self,
        dbtx: &mut DatabaseTransaction<'c>,
        input: &'b FalconInput,
        in_point: InPoint,
    ) -> Result<InputMeta, FalconInputError> {
        // Look up the registered public key for this account.
        let falcon_pub_key = dbtx
            .get_value(&FalconAccountKey(input.account_id))
            .await
            .ok_or(FalconInputError::AccountNotFound(input.account_id))?;

        dbtx.insert_entry(&FalconInputAuditKey(in_point), &input.amount)
            .await;

        Ok(InputMeta {
            amount: TransactionItemAmounts {
                amounts: Amounts::new_bitcoin(input.amount),
                fees: Amounts::ZERO,
            },
            pub_key: input.pub_key,
            falcon_pub_key: Some(falcon_pub_key),
        })
    }

    async fn process_output<'a, 'b>(
        &'a self,
        dbtx: &mut DatabaseTransaction<'b>,
        output: &'a FalconOutput,
        out_point: OutPoint,
    ) -> Result<TransactionItemAmounts, FalconOutputError> {
        match output {
            FalconOutput::Transfer { amount, recipient_account_id, .. } => {
                let utxo_key = FalconOutputKey(*recipient_account_id);
                let existing: Amount = dbtx.get_value(&utxo_key).await.unwrap_or(Amount::ZERO);
                dbtx.insert_entry(&utxo_key, &(existing + *amount))
                    .await;
                dbtx.insert_entry(&FalconOutputAuditKey(out_point), amount)
                    .await;

                Ok(TransactionItemAmounts {
                    amounts: Amounts::new_bitcoin(*amount),
                    fees: Amounts::ZERO,
                })
            }
            FalconOutput::Register { pub_key_bytes } => {
                let account_id = account_id_from_key(pub_key_bytes);
                dbtx.insert_entry(&FalconAccountKey(account_id), pub_key_bytes)
                    .await;

                Ok(TransactionItemAmounts {
                    amounts: Amounts::ZERO,
                    fees: Amounts::ZERO,
                })
            }
        }
    }

    async fn output_status(
        &self,
        _dbtx: &mut DatabaseTransaction<'_>,
        _out_point: OutPoint,
    ) -> Option<FalconOutputOutcome> {
        // Benchmark: always report Transfer outcome. Register outcomes are computed
        // client-side from account_id_from_key() and don't need polling.
        Some(FalconOutputOutcome::Transfer)
    }

    async fn audit(
        &self,
        dbtx: &mut DatabaseTransaction<'_>,
        audit: &mut Audit,
        module_instance_id: ModuleInstanceId,
    ) {
        audit
            .add_items(dbtx, module_instance_id, &FalconInputAuditPrefix, |_, v| {
                v.msats as i64
            })
            .await;
        audit
            .add_items(dbtx, module_instance_id, &FalconOutputAuditPrefix, |_, v| {
                -(v.msats as i64)
            })
            .await;
    }

    fn api_endpoints(&self) -> Vec<ApiEndpoint<Self>> {
        Vec::new()
    }
}
