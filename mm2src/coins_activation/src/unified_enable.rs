use crate::platform_coin_with_tokens::{init_platform_coin_with_tokens, EnablePlatformCoinWithTokensReq};
use crate::standalone_coin::init_standalone_coin::{init_standalone_coin, InitStandaloneCoinReq};
use crate::{init_erc20_token_activation::InitErc20TokenActivationRequest, init_token::init_token};
use coins::eth::v2_activation::{EthActivationV2Request, EthNode};
use coins::eth::EthCoin;
use coins::lp_coins::CoinProtocol;
use coins::tendermint::{RpcNode, TendermintCoin};
use coins::utxo::qtum::QtumCoin;
use coins::utxo::rpc_clients::electrum_rpc::connection::ElectrumConnectionSettings;
use coins::utxo::utxo_standard::UtxoStandardCoin;
use coins::utxo::{UtxoActivationParams, UtxoRpcMode};
use coins::z_coin::ZCoin;
use ethereum_types::Address;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use rpc_task::rpc_common::InitRpcTaskResponse;
use rpc_task::RpcInitReq;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{self as json, Value as Json};
use std::str::FromStr;

#[derive(Debug, Clone, Deserialize)]
pub struct EnableCoinUnifiedReq {
    /// Asset identifier from coins config; typically the ticker (e.g., "KMD", "ETH", "USDC-ERC20").
    pub asset_id: String,
}

#[derive(Clone, Serialize, SerializeErrorType, Display)]
#[serde(tag = "error_type", content = "error_data")]
pub enum EnableCoinUnifiedError {
    #[display(fmt = "Unsupported or missing protocol for ticker '{ticker}'")]
    UnsupportedProtocol { ticker: String },
    #[display(fmt = "Activation parameters missing in coin config: {_0}")]
    MissingActivationParams(String),
    #[display(fmt = "Invalid activation request derived from config: {_0}")]
    InvalidActivationRequest(String),
    #[display(fmt = "Internal error: {_0}")]
    Internal(String),
    #[display(fmt = "UTXO init error: {_0}")]
    UtxoInitError(String),
    #[display(fmt = "Qtum init error: {_0}")]
    QtumInitError(String),
    #[display(fmt = "ZCoin init error: {_0}")]
    ZcoinInitError(String),
    #[display(fmt = "ETH init error: {_0}")]
    EthInitError(String),
    #[display(fmt = "ERC20 init error: {_0}")]
    Erc20InitError(String),
    #[display(fmt = "Tendermint init error: {_0}")]
    TendermintInitError(String),
}

impl HttpStatusCode for EnableCoinUnifiedError {
    fn status_code(&self) -> StatusCode { match self {
        EnableCoinUnifiedError::UnsupportedProtocol { .. } => StatusCode::BAD_REQUEST,
        EnableCoinUnifiedError::MissingActivationParams(_) => StatusCode::BAD_REQUEST,
        EnableCoinUnifiedError::InvalidActivationRequest(_) => StatusCode::BAD_REQUEST,
        EnableCoinUnifiedError::UtxoInitError(_) => StatusCode::BAD_GATEWAY,
        EnableCoinUnifiedError::QtumInitError(_) => StatusCode::BAD_GATEWAY,
        EnableCoinUnifiedError::ZcoinInitError(_) => StatusCode::BAD_GATEWAY,
        EnableCoinUnifiedError::EthInitError(_) => StatusCode::BAD_GATEWAY,
        EnableCoinUnifiedError::Erc20InitError(_) => StatusCode::BAD_GATEWAY,
        EnableCoinUnifiedError::TendermintInitError(_) => StatusCode::BAD_GATEWAY,
        EnableCoinUnifiedError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }}
}

fn parse_vec<T: for<'a> Deserialize<'a>>(v: &Json) -> Option<Vec<T>> {
    json::from_value::<Vec<T>>(v.clone()).ok()
}

fn electrum_servers_from_conf(conf: &Json) -> Option<Vec<ElectrumConnectionSettings>> {
    // Try common keys
    if let Some(servers) = conf.get("electrum_servers").and_then(parse_vec) {
        return Some(servers);
    }
    if let Some(obj) = conf.get("electrum") {
        if let Some(servers) = obj.get("servers").and_then(parse_vec) {
            return Some(servers);
        }
    }
    // Some configs may use a generic 'servers' key
    if let Some(servers) = conf.get("servers").and_then(parse_vec) {
        return Some(servers);
    }
    None
}

fn eth_nodes_from_conf(conf: &Json) -> Option<Vec<EthNode>> {
    if let Some(nodes) = conf.get("nodes").and_then(parse_vec) { return Some(nodes); }
    // Fallback: array of URLs under 'rpc_urls'
    if let Some(urls) = conf.get("rpc_urls").and_then(|v| json::from_value::<Vec<String>>(v.clone()).ok()) {
        let nodes: Vec<EthNode> = urls.into_iter().map(|url| EthNode { url, komodo_proxy: false }).collect();
        return Some(nodes);
    }
    None
}

fn tendermint_nodes_from_conf(conf: &Json) -> Option<Vec<RpcNode>> {
    if let Some(nodes) = conf.get("nodes").and_then(parse_vec) { return Some(nodes); }
    None
}

fn swap_contract_from_conf(conf: &Json) -> Option<Address> {
    if let Some(addr_str) = conf.get("swap_contract_address").and_then(|v| v.as_str()) {
        return Address::from_str(addr_str).ok();
    }
    None
}

fn default_utxo_params_from_conf(conf: &Json) -> Option<UtxoActivationParams> {
    let servers = electrum_servers_from_conf(conf)?;
    let mode = UtxoRpcMode::Electrum { servers, min_connected: None, max_connected: None };
    let required_confirmations = conf.get("required_confirmations").and_then(|v| v.as_u64());
    let requires_notarization = conf.get("requires_notarization").and_then(|v| v.as_bool());
    Some(UtxoActivationParams {
        mode,
        utxo_merge_params: None,
        tx_history: false,
        required_confirmations,
        requires_notarization,
        address_format: None,
        gap_limit: None,
        enable_params: Default::default(),
        priv_key_policy: coins::lp_coins::PrivKeyActivationPolicy::ContextPrivKey,
        check_utxo_maturity: None,
        path_to_address: Default::default(),
    })
}

fn default_eth_request_from_conf(conf: &Json) -> Option<EthActivationV2Request> {
    let nodes = eth_nodes_from_conf(conf)?;
    let swap_contract_address = swap_contract_from_conf(conf)?;
    Some(EthActivationV2Request {
        nodes,
        rpc_mode: Default::default(),
        swap_contract_address,
        swap_v2_contracts: None,
        fallback_swap_contract: None,
        contract_supports_watchers: false,
        mm2: None,
        required_confirmations: conf.get("required_confirmations").and_then(|v| v.as_u64()),
        priv_key_policy: coins::eth::v2_activation::EthPrivKeyActivationPolicy::ContextPrivKey,
        enable_params: Default::default(),
        path_to_address: Default::default(),
        gap_limit: None,
    })
}

fn default_tendermint_request_from_conf(conf: &Json) -> Option<crate::tendermint_with_assets_activation::TendermintActivationParams> {
    let nodes = tendermint_nodes_from_conf(conf)?;
    Some(crate::tendermint_with_assets_activation::TendermintActivationParams {
        nodes,
        tokens_params: Vec::new(),
        tx_history: false,
        get_balances: true,
        path_to_address: Default::default(),
        activation_params: None,
    })
}

/// Unified coin enable init, selecting underlying activation strategy by `CoinProtocol` and deriving
/// activation parameters from the coin's config.
pub async fn init_enable_coin_unified(
    ctx: MmArc,
    request: RpcInitReq<EnableCoinUnifiedReq>,
) -> MmResult<InitRpcTaskResponse, EnableCoinUnifiedError> {
    let (client_id, inner) = (request.client_id, request.inner);
    let ticker = inner.asset_id;

    // Resolve protocol and coin conf from config
    let (conf, protocol) = coins::lp_coins::coin_conf_with_protocol(&ctx, &ticker, None)
        .map_to_mm(|e| EnableCoinUnifiedError::Internal(e))?;

    match protocol {
        CoinProtocol::UTXO { .. } | CoinProtocol::UTXO(_) => {
            let params = default_utxo_params_from_conf(&conf)
                .ok_or_else(|| MmError::new(EnableCoinUnifiedError::MissingActivationParams(
                    "Missing or empty 'electrum_servers' in coin config".to_string(),
                )))?;
            let init_req = InitStandaloneCoinReq { ticker, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_standalone_coin::<UtxoStandardCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::UtxoInitError(e.to_string())))
        },
        CoinProtocol::QTUM => {
            let params = default_utxo_params_from_conf(&conf)
                .ok_or_else(|| MmError::new(EnableCoinUnifiedError::MissingActivationParams(
                    "Missing or empty 'electrum_servers' in coin config".to_string(),
                )))?;
            let init_req = InitStandaloneCoinReq { ticker, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_standalone_coin::<QtumCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::QtumInitError(e.to_string())))
        },
        CoinProtocol::ZHTLC { .. } => {
            // Build default Zcoin params; attempt to use electrum configuration if present
            let mut params = coins::z_coin::ZcoinActivationParams::default();
            if let Some(servers) = conf.get("electrum_servers").and_then(parse_vec::<ElectrumConnectionSettings>) {
                params.mode = coins::z_coin::ZcoinRpcMode::Light {
                    electrum_servers: servers,
                    min_connected: None,
                    max_connected: None,
                    light_wallet_d_servers: Vec::new(),
                    sync_params: None,
                    skip_sync_params: None,
                };
            } else {
                return MmError::err(EnableCoinUnifiedError::MissingActivationParams(
                    "Missing or empty 'electrum_servers' in coin config".to_string(),
                ));
            }
            let init_req = InitStandaloneCoinReq { ticker, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_standalone_coin::<ZCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::ZcoinInitError(e.to_string())))
        },
        CoinProtocol::ETH { .. } => {
            let params = default_eth_request_from_conf(&conf)
                .ok_or_else(|| MmError::new(EnableCoinUnifiedError::MissingActivationParams(
                    "Missing 'nodes' and/or 'swap_contract_address' in coin config".to_string(),
                )))?;
            let init_req = EnablePlatformCoinWithTokensReq { ticker, request: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_platform_coin_with_tokens::<EthCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::EthInitError(e.to_string())))
        },
        CoinProtocol::ERC20 { .. } => {
            // ERC20 token only, defaults are sufficient for the request
            let params = InitErc20TokenActivationRequest {
                required_confirmations: conf.get("required_confirmations").and_then(|v| v.as_u64()),
                enable_params: Default::default(),
                path_to_address: Default::default(),
            };
            let init_req = crate::init_token::InitTokenReq { ticker, protocol: None, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_token::<EthCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::Erc20InitError(e.to_string())))
        },
        CoinProtocol::TENDERMINT { .. } => {
            let params = default_tendermint_request_from_conf(&conf)
                .ok_or_else(|| MmError::new(EnableCoinUnifiedError::MissingActivationParams(
                    "Missing 'nodes' in coin config".to_string(),
                )))?;
            let init_req = EnablePlatformCoinWithTokensReq { ticker, request: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_platform_coin_with_tokens::<TendermintCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::TendermintInitError(e.to_string())))
        },
        CoinProtocol::TENDERMINTTOKEN { .. } => {
            // Not yet supported as a standalone init; must be activated via platform coin request
            MmError::err(EnableCoinUnifiedError::UnsupportedProtocol { ticker })
        },
        // Other protocols are not supported via unified init for now.
        _ => MmError::err(EnableCoinUnifiedError::UnsupportedProtocol { ticker }),
    }
}