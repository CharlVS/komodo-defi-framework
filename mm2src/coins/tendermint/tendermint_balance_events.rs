use async_trait::async_trait;
use common::{executor::{AbortSettings, SpawnAbortable},
             http_uri_to_ws_address, log};
use futures_util::{SinkExt, StreamExt};
use mm2_event_stream::{behaviour::EventBehaviour, Event, EventStreamConfiguration};
use mm2_number::BigDecimal;
use std::collections::HashMap;

use super::TendermintCoin;
use crate::{tendermint::TendermintCommons, utxo::utxo_common::big_decimal_from_sat_unsigned, MarketCoinOps, MmCoin};

#[async_trait]
impl EventBehaviour for TendermintCoin {
    const EVENT_NAME: &'static str = "COIN_BALANCE";

    async fn handle(self, _interval: f64) {
        fn generate_subscription_query(query_filter: String) -> String {
            let q = json!({
                "jsonrpc": "2.0",
                "method": "subscribe",
                "id": 0,
                "params": {
                    "query": query_filter
                }
            });

            q.to_string()
        }

        let account_id = self.account_id.to_string();
        let mut current_balances: HashMap<String, BigDecimal> = HashMap::new();

        let receiver_q = generate_subscription_query(format!("coin_received.receiver = '{}'", account_id));
        let receiver_q = tokio_tungstenite_wasm::Message::Text(receiver_q);

        let spender_q = generate_subscription_query(format!("coin_spent.spender = '{}'", account_id));
        let spender_q = tokio_tungstenite_wasm::Message::Text(spender_q);

        loop {
            let node_uri = match self.rpc_client().await {
                Ok(client) => client.uri(),
                Err(e) => {
                    log::error!("{e}");
                    continue;
                },
            };

            let socket_address = format!("{}/{}", http_uri_to_ws_address(node_uri), "websocket");

            let mut wsocket = match tokio_tungstenite_wasm::connect(socket_address).await {
                Ok(ws) => ws,
                Err(e) => {
                    log::error!("{e}");
                    continue;
                },
            };

            // Filter received TX events
            if let Err(e) = wsocket.send(receiver_q.clone()).await {
                log::error!("{e}");
                continue;
            }

            // Filter spent TX events
            if let Err(e) = wsocket.send(spender_q.clone()).await {
                log::error!("{e}");
                continue;
            }

            while let Some(message) = wsocket.next().await {
                let msg = match message {
                    Ok(tokio_tungstenite_wasm::Message::Text(data)) => data.clone(),
                    Ok(tokio_tungstenite_wasm::Message::Close(_)) => break,
                    Err(err) => {
                        log::error!("{err}");
                        break;
                    },
                    _ => continue,
                };

                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&msg) {
                    let transfers: Vec<String> =
                        serde_json::from_value(json_val["result"]["events"]["transfer.amount"].clone())
                            .unwrap_or_default();

                    let mut denoms: Vec<String> = transfers
                        .iter()
                        .map(|t| {
                            let amount: String = t.chars().take_while(|c| c.is_numeric()).collect();
                            let denom = &t[amount.len()..];
                            denom.to_owned()
                        })
                        .collect();

                    denoms.dedup();
                    drop_mutability!(denoms);

                    for denom in denoms {
                        if let Some((ticker, decimals)) = self.active_ticker_and_decimals_from_denom(&denom) {
                            let balance_denom = match self.account_balance_for_denom(&self.account_id, denom).await {
                                Ok(balance_denom) => balance_denom,
                                Err(e) => {
                                    log::error!("{e}");
                                    continue;
                                },
                            };

                            let balance_decimal = big_decimal_from_sat_unsigned(balance_denom, decimals);

                            // Only broadcast when balance is changed
                            let mut broadcast = false;
                            if let Some(balance) = current_balances.get_mut(&ticker) {
                                if *balance != balance_decimal {
                                    *balance = balance_decimal.clone();
                                    broadcast = true;
                                }
                            } else {
                                current_balances.insert(ticker.clone(), balance_decimal.clone());
                                broadcast = true;
                            }

                            if broadcast {
                                let payload = json!({
                                    "ticker": ticker,
                                    "balance": { "spendable": balance_decimal, "unspendable": BigDecimal::default() }
                                });

                                self.ctx
                                    .stream_channel_controller
                                    .broadcast(Event::new(Self::EVENT_NAME.to_string(), payload.to_string()))
                                    .await;
                            }
                        }
                    }
                }
            }
        }
    }

    fn spawn_if_active(self, config: &EventStreamConfiguration) {
        if let Some(event) = config.get_event(Self::EVENT_NAME) {
            log::info!(
                "{} event is activated. `stream_interval_seconds`({}) has no effect for this event.",
                Self::EVENT_NAME,
                event.stream_interval_seconds
            );

            let fut = self.clone().handle(event.stream_interval_seconds);
            let settings =
                AbortSettings::info_on_abort(format!("Balance streaming stopped for {}", self.ticker().to_owned()));
            self.spawner().spawn_with_settings(fut, settings);
        }
    }
}
