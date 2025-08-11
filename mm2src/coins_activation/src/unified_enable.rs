use crate::platform_coin_with_tokens::{init_platform_coin_with_tokens, EnablePlatformCoinWithTokensReq};
use crate::standalone_coin::init_standalone_coin::{init_standalone_coin, InitStandaloneCoinReq};
use crate::{init_erc20_token_activation::InitErc20TokenActivationRequest, init_token::init_token};
use coins::eth::v2_activation::{Erc20TokenActivationRequest, EthActivationV2Request, EthNode};
use coins::eth::EthCoin;
use coins::lp_coins::CoinProtocol;
use coins::tendermint::{RpcNode, TendermintCoin};
use coins::utxo::qtum::QtumCoin;
use coins::utxo::rpc_clients::electrum_rpc::connection::ElectrumConnectionSettings;
use coins::utxo::utxo_standard::UtxoStandardCoin;
use coins::utxo::{UtxoActivationParams, UtxoRpcMode};
use coins::z_coin::ZCoin;
use coins::{lp_coinfind_any, lp_coinfind_or_err};
use common::SuccessResponse;
use ethereum_types::Address;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use rpc_task::rpc_common::{CancelRpcTaskRequest, RpcTaskStatusRequest};
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

#[derive(Clone, Serialize, SerializeErrorType, Display)]
#[serde(tag = "error_type", content = "error_data")]
pub enum EnableCoinUnifiedStatusError {
    #[display(fmt = "No such task: {_0}")]
    NoSuchTask(u64),
    #[display(fmt = "Internal error: {_0}")]
    Internal(String),
}

#[derive(Clone, Serialize, SerializeErrorType, Display)]
#[serde(tag = "error_type", content = "error_data")]
pub enum CancelEnableCoinUnifiedError {
    #[display(fmt = "No such task: {_0}")]
    NoSuchTask(u64),
    #[display(fmt = "Internal error: {_0}")]
    Internal(String),
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
        CoinProtocol::TRX { .. } => {
            // TRX uses the same activation machinery as ETH (ChainSpec::Tron)
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
        CoinProtocol::ERC20 { ref platform, .. } => {
            // If platform coin is active and available -> enable token only; otherwise enable platform with this token
            let platform_active = match lp_coinfind_any(&ctx, platform).await {
                Ok(Some(coin)) => coin.is_available(),
                _ => false,
            };

            if platform_active {
                // Token-only activation
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
            } else {
                // Platform-with-tokens activation with a single requested token
                let mut platform_req = default_eth_request_from_conf(&conf)
                    .ok_or_else(|| MmError::new(EnableCoinUnifiedError::MissingActivationParams(
                        "Missing 'nodes' and/or 'swap_contract_address' in coin config".to_string(),
                    )))?;
                let token_req = crate::platform_coin_with_tokens::TokenActivationRequest::<Erc20TokenActivationRequest> {
                    ticker: ticker.clone(),
                    protocol: None,
                    request: Erc20TokenActivationRequest { required_confirmations: conf.get("required_confirmations").and_then(|v| v.as_u64()) },
                };
                let eth_with_tokens = crate::eth_with_token_activation::EthWithTokensActivationRequest {
                    platform_request: platform_req,
                    erc20_tokens_requests: vec![token_req],
                    get_balances: true,
                    nft_req: None,
                };
                let init_req = EnablePlatformCoinWithTokensReq { ticker: platform.clone(), request: eth_with_tokens };
                let rpc_req = RpcInitReq { client_id, inner: init_req };
                init_platform_coin_with_tokens::<EthCoin>(ctx, rpc_req)
                    .await
                    .map_err(|e| MmError::new(EnableCoinUnifiedError::EthInitError(e.to_string())))
            }
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
        CoinProtocol::TENDERMINTTOKEN { ref platform, .. } => {
            // If platform is not active, activate platform with this token; if active, currently not supported as token-only.
            let platform_active = match lp_coinfind_any(&ctx, platform).await {
                Ok(Some(coin)) => coin.is_available(),
                _ => false,
            };
            if platform_active {
                return MmError::err(EnableCoinUnifiedError::UnsupportedProtocol { ticker });
            }
            // Build minimal platform-with-tokens request including this token by ticker; protocol info from config
            let params = default_tendermint_request_from_conf(&conf)
                .ok_or_else(|| MmError::new(EnableCoinUnifiedError::MissingActivationParams(
                    "Missing 'nodes' in coin config".to_string(),
                )))?;
            // Append the token by ticker
            let mut params = params;
            params.tokens_params.push(crate::platform_coin_with_tokens::TokenActivationRequest::<crate::tendermint_with_assets_activation::TendermintTokenActivationParams> {
                ticker: ticker.clone(),
                protocol: None,
                request: crate::tendermint_with_assets_activation::TendermintTokenActivationParams {},
            });
            // The platform ticker is provided by `platform` field of protocol
            let init_req = EnablePlatformCoinWithTokensReq { ticker: platform.clone(), request: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_platform_coin_with_tokens::<TendermintCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::TendermintInitError(e.to_string())))
        },
        _ => MmError::err(EnableCoinUnifiedError::UnsupportedProtocol { ticker }),
    }
}

pub async fn enable_coin_unified_status(
    ctx: MmArc,
    req: RpcTaskStatusRequest,
) -> MmResult<Json, EnableCoinUnifiedStatusError> {
    use rpc_task::RpcTaskStatus;
    let coins_act_ctx = crate::context::CoinsActivationContext::from_ctx(&ctx)
        .map_to_mm(|e| EnableCoinUnifiedStatusError::Internal(e))?;

    // Helper: try status on a manager and return Option<Json>
    async fn try_manager_status<Task, E>(
        manager: &rpc_task::RpcTaskManagerShared<Task>,
        req: &RpcTaskStatusRequest,
    ) -> Option<Result<Json, String>>
    where
        Task: rpc_task::RpcTask + Send + Sync + 'static,
        <Task as rpc_task::RpcTaskTypes>::Error: mm2_err_handle::SerMmErrorType + Clone,
        <Task as rpc_task::RpcTaskTypes>::Item: serde::Serialize + Clone,
        <Task as rpc_task::RpcTaskTypes>::InProgressStatus: serde::Serialize + Clone,
        <Task as rpc_task::RpcTaskTypes>::AwaitingStatus: serde::Serialize + Clone,
    {
        let guard = manager.lock().ok()?;
        match guard.task_status(req.task_id, req.forget_if_finished) {
            Ok(status) => Some(serde_json::to_value(status.map_err(|e| e)).map_err(|e| e.to_string())),
            Err(rpc_task::RpcTaskError::NoSuchTask(_)) => None,
            Err(other) => Some(Err(other.to_string())),
        }
    }

    macro_rules! try_status {
        ($mgr:expr) => {{
            if let Some(res) = try_manager_status(&$mgr, &req).await { return res.map_err(|e| MmError::new(EnableCoinUnifiedStatusError::Internal(e))); }
        }};
    }

    try_status!(coins_act_ctx.init_utxo_standard_task_manager);
    try_status!(coins_act_ctx.init_bch_task_manager);
    try_status!(coins_act_ctx.init_qtum_task_manager);
    try_status!(coins_act_ctx.init_z_coin_task_manager);
    try_status!(coins_act_ctx.init_eth_task_manager);
    try_status!(coins_act_ctx.init_erc20_token_task_manager);
    try_status!(coins_act_ctx.init_tendermint_coin_task_manager);
    #[cfg(not(target_arch = "wasm32"))]
    try_status!(coins_act_ctx.init_lightning_task_manager);

    MmError::err(EnableCoinUnifiedStatusError::NoSuchTask(req.task_id))
}

pub async fn cancel_enable_coin_unified(
    ctx: MmArc,
    req: CancelRpcTaskRequest,
) -> MmResult<SuccessResponse, CancelEnableCoinUnifiedError> {
    let coins_act_ctx = crate::context::CoinsActivationContext::from_ctx(&ctx)
        .map_to_mm(|e| CancelEnableCoinUnifiedError::Internal(e))?;

    // Helper: try cancel on a manager
    fn try_manager_cancel<Task>(manager: &rpc_task::RpcTaskManagerShared<Task>, task_id: u64) -> Option<Result<(), String>>
    where
        Task: rpc_task::RpcTask + Send + Sync + 'static,
    {
        let guard = manager.lock().ok()?;
        match guard.cancel_task(task_id) {
            Ok(_) => Some(Ok(())),
            Err(rpc_task::RpcTaskError::NoSuchTask(_)) => None,
            Err(other) => Some(Err(other.to_string())),
        }
    }

    macro_rules! try_cancel {
        ($mgr:expr) => {{
            if let Some(res) = try_manager_cancel(&$mgr, req.task_id) { return res.map(|_| SuccessResponse::new()).map_err(|e| MmError::new(CancelEnableCoinUnifiedError::Internal(e))); }
        }};
    }

    try_cancel!(coins_act_ctx.init_utxo_standard_task_manager);
    try_cancel!(coins_act_ctx.init_bch_task_manager);
    try_cancel!(coins_act_ctx.init_qtum_task_manager);
    try_cancel!(coins_act_ctx.init_z_coin_task_manager);
    try_cancel!(coins_act_ctx.init_eth_task_manager);
    try_cancel!(coins_act_ctx.init_erc20_token_task_manager);
    try_cancel!(coins_act_ctx.init_tendermint_coin_task_manager);
    #[cfg(not(target_arch = "wasm32"))]
    try_cancel!(coins_act_ctx.init_lightning_task_manager);

    MmError::err(CancelEnableCoinUnifiedError::NoSuchTask(req.task_id))
}