use anyhow::{anyhow, bail, Result};
use serde_json::Value as Json;
use std::collections::HashMap;

use common::log::{debug, error};
use mm2_rpc::data::legacy::Mm2RpcResult;
use mm2_rpc::data::version2::MmRpcRequest;

use super::init_activation_scheme::get_activation_scheme_path;
use crate::helpers::read_json_file;
use crate::logging::{error_anyhow, error_bail};
use crate::rpc_data::{bch, ActivationMethod, ActivationRequestLegacy, ActivationV2Params, V2ActivationMethod};

#[derive(Default)]
pub(crate) struct ActivationScheme {
    scheme: HashMap<String, Json>,
}

impl ActivationScheme {
    pub(crate) fn get_activation_method(&self, coin: &str) -> Result<ActivationMethod> {
        let method_json = self
            .scheme
            .get(coin)
            .ok_or_else(|| error_anyhow!("Coin is not in activation scheme data: {}", coin))?
            .clone();

        if let Some(_mmrpc) = method_json.get("mmrpc") {
            let Some(params) = method_json.get("params") else {
                error_bail!("Todo")
            };
            let Some(method) = method_json.get("method") else {
                error_bail!("Todo")
            };

            match serde_json::from_value(method.clone()) {
                Ok(V2ActivationMethod::EnableBchWithTokens) => {
                    let request: MmRpcRequest<V2ActivationMethod, ActivationV2Params> =
                        serde_json::from_value(method_json.clone()).map_err(|error| {
                            error_anyhow!("Failed to deserialize json data: {:?}, error: {}", method_json, error)
                        })?;

                    Ok(ActivationMethod::V2(request))
                },
                Err(error) => {
                    error_bail!("error: {}", error)
                },
            }
        } else {
            let method: ActivationRequestLegacy = serde_json::from_value(method_json.clone()).map_err(|error| {
                error_anyhow!("Failed to deserialize json data: {:?}, error: {}", method_json, error)
            })?;

            Ok(ActivationMethod::Legacy(method))
        }
    }

    fn init(&mut self) -> Result<()> {
        let mut scheme_source: Vec<Json> = Self::load_json_file()?;
        self.scheme = scheme_source
            .iter_mut()
            .filter_map(Self::get_coin_activation_command)
            .collect();
        Ok(())
    }

    fn get_coin_activation_command(element: &mut Json) -> Option<(String, Json)> {
        Self::get_coin_activation_command_impl(element).ok()
    }

    fn get_coin_activation_command_impl(element: &mut Json) -> Result<(String, Json)> {
        let coin = element
            .get_mut("coin")
            .ok_or_else(|| error_anyhow!("Failed to get coin pair, no coin value"))?
            .as_str()
            .ok_or_else(|| error_anyhow!("Failed to get coin pair, coin is not str"))?
            .to_string();
        let mut command = element
            .get_mut("command")
            .ok_or_else(|| error_anyhow!("Failed to get coin pair, no command value"))?
            .take();
        command
            .as_object_mut()
            .ok_or_else(|| error_anyhow!("Failed to get coin pair, command is not object"))?
            .remove("userpass");
        Ok((coin, command))
    }

    fn load_json_file() -> Result<Vec<Json>> {
        let activation_scheme_path = get_activation_scheme_path()?;
        debug!("Start reading activation_scheme from: {activation_scheme_path:?}");

        let mut activation_scheme: Json = read_json_file(&activation_scheme_path)?;

        let Json::Array(results) = activation_scheme
            .get_mut("results")
            .ok_or_else(|| error_anyhow!("Failed to load activation scheme json file, no results section"))?
            .take()
        else {
            error_bail!("Failed to load activation scheme json file, wrong format")
        };
        Ok(results)
    }
}

pub(crate) fn get_activation_scheme() -> Result<ActivationScheme> {
    let mut activation_scheme = ActivationScheme::default();
    activation_scheme.init()?;
    Ok(activation_scheme)
}
