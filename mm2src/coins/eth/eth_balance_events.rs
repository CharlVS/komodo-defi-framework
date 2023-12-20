use async_trait::async_trait;
use common::{executor::{AbortSettings, SpawnAbortable, Timer},
             log, Future01CompatExt};
use futures::{channel::oneshot::{self, Receiver, Sender},
              stream::FuturesUnordered,
              StreamExt};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::MmError;
use mm2_event_stream::{behaviour::{EventBehaviour, EventInitStatus},
                       Event, EventStreamConfiguration};
use mm2_number::BigDecimal;
use std::collections::BTreeMap;

use super::EthCoin;
use crate::{eth::{u256_to_big_decimal, Erc20TokenInfo},
            BalanceError, MmCoin};

/// This implementation differs from others, as they immediately return
/// an error if any of the requests fails. This one completes all futures
/// and returns their results individually.
async fn get_individual_balance_results_concurrently(
    coin: &EthCoin,
) -> Vec<Result<(String, BigDecimal), MmError<BalanceError>>> {
    let mut tokens = coin.get_erc_tokens_infos();

    // Workaround for performance purposes.
    //
    // Unlike tokens, the platform coin length is constant (=1). Instead of creating a generic
    // type and mapping the platform coin and the entire token list (which can grow at any time), we map
    // the platform coin to Erc20TokenInfo so that we can use the token list right away without
    // additional mapping.
    tokens.insert(coin.ticker.clone(), Erc20TokenInfo {
        token_address: coin.my_address,
        decimals: coin.decimals,
    });

    let jobs = tokens
        .into_iter()
        .map(|(token_ticker, info)| async move {
            if token_ticker == coin.ticker {
                let balance_as_u256 = coin.address_balance(coin.my_address).compat().await?;
                let balance_as_big_decimal = u256_to_big_decimal(balance_as_u256, coin.decimals)?;
                Ok((coin.ticker.clone(), balance_as_big_decimal))
            } else {
                let balance_as_u256 = coin.get_token_balance_by_address(info.token_address).await?;
                let balance_as_big_decimal = u256_to_big_decimal(balance_as_u256, info.decimals)?;
                Ok((token_ticker, balance_as_big_decimal))
            }
        })
        .collect::<FuturesUnordered<_>>();

    jobs.collect().await
}

#[async_trait]
impl EventBehaviour for EthCoin {
    const EVENT_NAME: &'static str = "COIN_BALANCE";

    async fn handle(self, interval: f64, tx: oneshot::Sender<EventInitStatus>) {
        const RECEIVER_DROPPED_MSG: &str = "Receiver is dropped, which should never happen.";

        async fn with_socket(_coin: EthCoin, _ctx: MmArc) { todo!() }

        async fn with_polling(coin: EthCoin, ctx: MmArc, interval: f64) {
            let mut cache: BTreeMap<String, BigDecimal> = BTreeMap::new();

            loop {

                let mut balance_updates = vec![];
                for result in get_individual_balance_results_concurrently(&coin).await {
                    match result {
                        Ok((ticker, balance)) => {
                            if Some(&balance) == cache.get(&ticker) {
                                continue;
                            }

                            balance_updates.push(json!({
                                "ticker": ticker,
                                "balance": { "spendable": balance, "unspendable": BigDecimal::default() }
                            }));
                            cache.insert(ticker.to_owned(), balance);
                        },
                        Err(_e) => {
                            // TODO:
                            // broadcast an error event here
                        },
                    };
                }

                if !balance_updates.is_empty() {
                    ctx.stream_channel_controller
                        .broadcast(Event::new(
                            EthCoin::EVENT_NAME.to_string(),
                            json!(balance_updates).to_string(),
                        ))
                        .await;
                }

                // TODO: subtract the time complexity
                Timer::sleep(interval).await;
            }
        }

        let ctx = match MmArc::from_weak(&self.ctx) {
            Some(ctx) => ctx,
            None => {
                let msg = "MM context must have been initialized already.";
                tx.send(EventInitStatus::Failed(msg.to_owned()))
                    .expect(RECEIVER_DROPPED_MSG);
                panic!("{}", msg);
            },
        };

        tx.send(EventInitStatus::Success).expect(RECEIVER_DROPPED_MSG);

        with_polling(self, ctx, interval).await
    }

    async fn spawn_if_active(self, config: &EventStreamConfiguration) -> EventInitStatus {
        if let Some(event) = config.get_event(Self::EVENT_NAME) {
            log::info!("{} event is activated for {}", Self::EVENT_NAME, self.ticker,);

            let (tx, rx): (Sender<EventInitStatus>, Receiver<EventInitStatus>) = oneshot::channel();
            let fut = self.clone().handle(event.stream_interval_seconds, tx);
            let settings =
                AbortSettings::info_on_abort(format!("{} event is stopped for {}.", Self::EVENT_NAME, self.ticker));
            self.spawner().spawn_with_settings(fut, settings);

            rx.await.unwrap_or_else(|e| {
                EventInitStatus::Failed(format!("Event initialization status must be received: {}", e))
            })
        } else {
            EventInitStatus::Inactive
        }
    }
}
