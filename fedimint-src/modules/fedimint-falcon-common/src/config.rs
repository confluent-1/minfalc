use fedimint_core::core::ModuleKind;
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::plugin_types_trait_impl_config;
use serde::{Deserialize, Serialize};

use crate::FalconCommonInit;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FalconConfig {
    pub private: FalconConfigPrivate,
    pub consensus: FalconConfigConsensus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Encodable, Decodable, Hash)]
pub struct FalconClientConfig;

#[derive(Clone, Debug, Serialize, Deserialize, Decodable, Encodable)]
pub struct FalconConfigConsensus;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FalconConfigPrivate;

plugin_types_trait_impl_config!(
    FalconCommonInit,
    FalconConfig,
    FalconConfigPrivate,
    FalconConfigConsensus,
    FalconClientConfig
);
