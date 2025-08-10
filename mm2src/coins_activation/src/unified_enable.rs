use crate::platform_coin_with_tokens::{init_platform_coin_with_tokens, EnablePlatformCoinWithTokensReq};
use crate::standalone_coin::init_standalone_coin::{init_standalone_coin, InitStandaloneCoinReq};
use crate::{init_erc20_token_activation::InitErc20TokenActivationRequest, init_token::init_token};
use async_trait::async_trait;
use coins::eth::EthCoin;
use coins::lp_coins::CoinProtocol;
use coins::tendermint::TendermintCoin;
use coins::utxo::qtum::QtumCoin;
use coins::utxo::utxo_standard::UtxoStandardCoin;
use coins::utxo::UtxoActivationParams;
use coins::z_coin::ZCoin;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use rpc_task::rpc_common::InitRpcTaskResponse;
use rpc_task::RpcInitReq;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as Json;

#[derive(Debug, Clone, Deserialize)]
pub struct EnableCoinUnifiedReq {
    pub ticker: String,
    #[serde(default)]
    pub protocol: Option<CoinProtocol>,
    /// Activation request payload. The expected structure depends on the resolved protocol:
    /// - UTXO/QTUM/ZHTLC: UtxoActivationParams or ZcoinActivationParams
    /// - ETH (platform with tokens): EthWithTokensActivationRequest
    /// - ERC20 (token only): InitErc20TokenActivationRequest
    /// - TENDERMINT (platform with tokens): TendermintActivationParams
    #[serde(default)]
    pub activation_request: Json,
}

#[derive(Clone, Serialize, SerializeErrorType, Display)]
#[serde(tag = "error_type", content = "error_data")]
pub enum EnableCoinUnifiedError {
    #[display(fmt = "Unsupported or missing protocol for ticker '{ticker}'")]
    UnsupportedProtocol { ticker: String },
    #[display(fmt = "Failed to parse activation request: {_0}")]
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

fn parse_or_err<T: for<'a> Deserialize<'a>>(payload: &Json) -> MmResult<T, EnableCoinUnifiedError> {
    serde_json::from_value::<T>(payload.clone())
        .map_err(|e| MmError::new(EnableCoinUnifiedError::InvalidActivationRequest(e.to_string())))
}

/// Unified coin enable init, selecting underlying activation strategy by `CoinProtocol`.
pub async fn init_enable_coin_unified(
    ctx: MmArc,
    request: RpcInitReq<EnableCoinUnifiedReq>,
) -> MmResult<InitRpcTaskResponse, EnableCoinUnifiedError> {
    let (client_id, inner) = (request.client_id, request.inner);

    // Resolve protocol from request or coin config
    let resolved_protocol = if let Some(proto) = inner.protocol.clone() {
        proto
    } else {
        let (_conf, proto) = coins::lp_coins::coin_conf_with_protocol(&ctx, &inner.ticker, None)
            .map_to_mm(|e| EnableCoinUnifiedError::Internal(e))?;
        proto
    };

    match resolved_protocol {
        CoinProtocol::UTXO { .. } | CoinProtocol::UTXO(_) => {
            let params: UtxoActivationParams = parse_or_err(&inner.activation_request)?;
            let init_req = InitStandaloneCoinReq { ticker: inner.ticker, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_standalone_coin::<UtxoStandardCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::UtxoInitError(e.to_string())))
        },
        CoinProtocol::QTUM => {
            let params: UtxoActivationParams = parse_or_err(&inner.activation_request)?;
            let init_req = InitStandaloneCoinReq { ticker: inner.ticker, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_standalone_coin::<QtumCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::QtumInitError(e.to_string())))
        },
        CoinProtocol::ZHTLC { .. } => {
            // ZCoin uses standalone coin init but with its own activation params type; let JSON pass-through to existing endpoint is not exposed here,
            // so delegate using the same UTXO params envelope by expecting caller to send correct ZcoinActivationParams shape.
            let params: coins::z_coin::ZcoinActivationParams = parse_or_err(&inner.activation_request)?;
            let init_req = InitStandaloneCoinReq { ticker: inner.ticker, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_standalone_coin::<ZCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::ZcoinInitError(e.to_string())))
        },
        CoinProtocol::ETH { .. } => {
            // ETH platform with optional tokens
            let params: crate::eth_with_token_activation::EthWithTokensActivationRequest =
                parse_or_err(&inner.activation_request)?;
            let init_req = EnablePlatformCoinWithTokensReq { ticker: inner.ticker, request: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_platform_coin_with_tokens::<EthCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::EthInitError(e.to_string())))
        },
        CoinProtocol::ERC20 { .. } => {
            // ERC20 token only
            let params: InitErc20TokenActivationRequest = parse_or_err(&inner.activation_request)?;
            let init_req = crate::init_token::InitTokenReq { ticker: inner.ticker, protocol: None, activation_params: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_token::<EthCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::Erc20InitError(e.to_string())))
        },
        CoinProtocol::TENDERMINT { .. } => {
            let params: crate::tendermint_with_assets_activation::TendermintActivationParams =
                parse_or_err(&inner.activation_request)?;
            let init_req = EnablePlatformCoinWithTokensReq { ticker: inner.ticker, request: params };
            let rpc_req = RpcInitReq { client_id, inner: init_req };
            init_platform_coin_with_tokens::<TendermintCoin>(ctx, rpc_req)
                .await
                .map_err(|e| MmError::new(EnableCoinUnifiedError::TendermintInitError(e.to_string())))
        },
        CoinProtocol::TENDERMINTTOKEN { .. } => {
            // Not yet supported as a standalone init; must be activated via platform coin request
            MmError::err(EnableCoinUnifiedError::UnsupportedProtocol { ticker: inner.ticker })
        },
        // Other protocols are not supported via unified init for now.
        _ => MmError::err(EnableCoinUnifiedError::UnsupportedProtocol { ticker: inner.ticker }),
    }
}