use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

use common::log::{debug, error};
use mm2_net::native_http::slurp_post_json;

use crate::{error_anyhow, error_bail};

#[async_trait]
pub(super) trait Transport {
    async fn send<ReqT, OkT, ErrT>(&self, req: ReqT) -> Result<Result<OkT, ErrT>>
    where
        ReqT: Serialize + Send + Sync,
        OkT: for<'a> Deserialize<'a>,
        ErrT: for<'a> Deserialize<'a>;
}

pub(super) struct SlurpTransport {
    rpc_uri: String,
}

impl SlurpTransport {
    pub(super) fn new(rpc_uri: String) -> SlurpTransport { SlurpTransport { rpc_uri } }
}

#[async_trait]
impl Transport for SlurpTransport {
    async fn send<ReqT, OkT, ErrT>(&self, req: ReqT) -> Result<Result<OkT, ErrT>>
    where
        ReqT: Serialize + Send + Sync,
        OkT: for<'a> Deserialize<'a>,
        ErrT: for<'a> Deserialize<'a>,
    {
        let data = serde_json::to_string(&req)
            .map_err(|error| error_anyhow!("Failed to serialize data being sent: {error}"))?;
        match slurp_post_json(&self.rpc_uri, data).await {
            Err(error) => error_bail!("Failed to send json: {error}"),
            Ok(resp) => resp.process::<OkT, ErrT>(),
        }
    }
}

trait Response {
    fn process<OkT, ErrT>(self) -> Result<Result<OkT, ErrT>>
    where
        OkT: for<'a> Deserialize<'a>,
        ErrT: for<'a> Deserialize<'a>;
}

impl Response for (StatusCode, HeaderMap, Vec<u8>) {
    fn process<OkT, ErrT>(self) -> Result<Result<OkT, ErrT>>
    where
        OkT: for<'a> Deserialize<'a>,
        ErrT: for<'a> Deserialize<'a>,
    {
        let (status, _headers, data) = self;
        match status {
            StatusCode::OK => match serde_json::from_slice::<OkT>(&data) {
                Ok(resp_data) => Ok(Ok(resp_data)),
                Err(error) => {
                    let data = String::from_utf8(data)
                        .map_err(|error| error_anyhow!("Failed to get string from resp data: {error}"))?;
                    error_bail!("Failed to deserialize response from data: {data:?}, error: {error}")
                },
            },
            code => {
                debug!("Remote service answered with the code: {}", code);
                match serde_json::from_slice::<ErrT>(&data) {
                    Ok(resp_data) => Ok(Err(resp_data)),
                    Err(error) => {
                        let data = String::from_utf8(data)
                            .map_err(|error| error_anyhow!("Failed to get string from resp data: {error}"))?;
                        error_bail!("Failed to deserialize response from data: {data:?}, error: {error}")
                    },
                }
            },
        }
    }
}