use crate::utxo::rpc_clients::UtxoRpcClientEnum;
use crate::utxo::utxo_common::payment_script;
use crate::utxo::{utxo_common::UtxoArcBuilder, UtxoArc, UtxoCoinBuilder};
use crate::{BalanceFut, CoinBalance, FoundSwapTxSpend, MarketCoinOps, SwapOps, TransactionEnum, TransactionFut};
use bitcrypto::dhash160;
use chain::constants::SEQUENCE_FINAL;
use chain::Transaction as UtxoTx;
use common::mm_ctx::MmArc;
use common::mm_error::prelude::*;
use common::mm_number::{BigDecimal, MmNumber};
use futures::lock::Mutex as AsyncMutex;
use futures::{FutureExt, TryFutureExt};
use futures01::Future;
use keys::Public;
use rpc::v1::types::Bytes;
use script::{Builder as ScriptBuilder, Opcode};
use serde_json::Value as Json;
use serialization::deserialize;
use std::sync::Arc;
use zcash_client_backend::encoding::encode_payment_address;
use zcash_primitives::{constants::mainnet as z_mainnet_constants, sapling::PaymentAddress, zip32::ExtendedSpendingKey};
use zcash_proofs::prover::LocalTxProver;

mod z_htlc;
use z_htlc::{z_p2sh_spend, z_send_htlc};

mod z_rpc;
use z_rpc::ZRpcOps;

#[cfg(test)] mod z_coin_tests;

pub struct ZCoinFields {
    z_spending_key: ExtendedSpendingKey,
    z_addr: PaymentAddress,
    z_addr_encoded: String,
    z_tx_prover: LocalTxProver,
    /// Mutex preventing concurrent transaction generation/same input usage
    z_tx_mutex: AsyncMutex<()>,
}

impl std::fmt::Debug for ZCoinFields {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "ZCoinFields {{ z_addr: {:?}, z_addr_encoded: {} }}",
            self.z_addr, self.z_addr_encoded
        )
    }
}

#[derive(Clone, Debug)]
pub struct ZCoin {
    utxo_arc: UtxoArc,
    z_fields: Arc<ZCoinFields>,
}

impl ZCoin {
    pub fn z_rpc(&self) -> &(dyn ZRpcOps + Send + Sync) { self.utxo_arc.rpc_client.as_ref() }

    pub fn rpc_client(&self) -> &UtxoRpcClientEnum { &self.utxo_arc.rpc_client }
}

#[derive(Debug)]
pub enum ZCoinBuildError {
    BuilderError(String),
    GetAddressError,
}

pub async fn z_coin_from_conf_and_request(
    ctx: &MmArc,
    ticker: &str,
    conf: &Json,
    req: &Json,
    secp_priv_key: &[u8],
    z_spending_key: ExtendedSpendingKey,
) -> Result<ZCoin, MmError<ZCoinBuildError>> {
    let builder = UtxoArcBuilder::new(ctx, ticker, conf, req, secp_priv_key);
    let utxo_arc = builder
        .build()
        .await
        .map_err(|e| MmError::new(ZCoinBuildError::BuilderError(e)))?;

    let (_, z_addr) = z_spending_key
        .default_address()
        .map_err(|_| MmError::new(ZCoinBuildError::GetAddressError))?;

    let z_tx_prover = LocalTxProver::bundled();
    let z_addr_encoded = encode_payment_address(z_mainnet_constants::HRP_SAPLING_PAYMENT_ADDRESS, &z_addr);
    let z_fields = ZCoinFields {
        z_spending_key,
        z_addr,
        z_addr_encoded,
        z_tx_prover,
        z_tx_mutex: AsyncMutex::new(()),
    };
    Ok(ZCoin {
        utxo_arc,
        z_fields: Arc::new(z_fields),
    })
}

impl MarketCoinOps for ZCoin {
    fn ticker(&self) -> &str { todo!() }

    fn my_address(&self) -> Result<String, String> { todo!() }

    fn my_balance(&self) -> BalanceFut<CoinBalance> {
        let min_conf = 0;
        let fut = self
            .utxo_arc
            .rpc_client
            .as_ref()
            .z_get_balance(&self.z_fields.z_addr_encoded, min_conf)
            // at the moment Z coins do not have an unspendable balance
            .map(|spendable| CoinBalance {
                spendable: spendable.to_decimal(),
                unspendable: BigDecimal::from(0),
            })
            .map_err(|e| e.into());
        Box::new(fut)
    }

    fn base_coin_balance(&self) -> BalanceFut<BigDecimal> { todo!() }

    fn send_raw_tx(&self, _tx: &str) -> Box<dyn Future<Item = String, Error = String> + Send> { todo!() }

    fn wait_for_confirmations(
        &self,
        _tx: &[u8],
        _confirmations: u64,
        _requires_nota: bool,
        _wait_until: u64,
        _check_every: u64,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        todo!()
    }

    fn wait_for_tx_spend(
        &self,
        _transaction: &[u8],
        _wait_until: u64,
        _from_block: u64,
        _swap_contract_address: &Option<Bytes>,
    ) -> TransactionFut {
        todo!()
    }

    fn tx_enum_from_bytes(&self, _bytes: &[u8]) -> Result<TransactionEnum, String> { todo!() }

    fn current_block(&self) -> Box<dyn Future<Item = u64, Error = String> + Send> { todo!() }

    fn address_from_pubkey_str(&self, _pubkey: &str) -> Result<String, String> { todo!() }

    fn display_priv_key(&self) -> String { todo!() }

    fn min_tx_amount(&self) -> BigDecimal { todo!() }

    fn min_trading_vol(&self) -> MmNumber { todo!() }
}

impl SwapOps for ZCoin {
    fn send_taker_fee(&self, _fee_addr: &[u8], _amount: BigDecimal) -> TransactionFut { todo!() }

    fn send_maker_payment(
        &self,
        time_lock: u32,
        taker_pub: &[u8],
        secret_hash: &[u8],
        amount: BigDecimal,
        _swap_contract_address: &Option<Bytes>,
    ) -> TransactionFut {
        let selfi = self.clone();
        let taker_pub = taker_pub.to_vec();
        let secret_hash = secret_hash.to_vec();
        let fut = async move {
            let utxo_tx = try_s!(z_send_htlc(&selfi, time_lock, &taker_pub, &secret_hash, amount).await);
            Ok(utxo_tx.into())
        };
        Box::new(fut.boxed().compat())
    }

    fn send_taker_payment(
        &self,
        time_lock: u32,
        maker_pub: &[u8],
        secret_hash: &[u8],
        amount: BigDecimal,
        _swap_contract_address: &Option<Bytes>,
    ) -> TransactionFut {
        let selfi = self.clone();
        let maker_pub = maker_pub.to_vec();
        let secret_hash = secret_hash.to_vec();
        let fut = async move {
            let utxo_tx = try_s!(z_send_htlc(&selfi, time_lock, &maker_pub, &secret_hash, amount).await);
            Ok(utxo_tx.into())
        };
        Box::new(fut.boxed().compat())
    }

    fn send_maker_spends_taker_payment(
        &self,
        taker_payment_tx: &[u8],
        time_lock: u32,
        taker_pub: &[u8],
        secret: &[u8],
        _swap_contract_address: &Option<Bytes>,
    ) -> TransactionFut {
        let tx: UtxoTx = try_fus!(deserialize(taker_payment_tx).map_err(|e| ERRL!("{:?}", e)));
        let redeem_script = payment_script(
            time_lock,
            &*dhash160(secret),
            &Public::from_slice(taker_pub).unwrap(),
            self.utxo_arc.key_pair.public(),
        );
        let script_data = ScriptBuilder::default()
            .push_data(secret)
            .push_opcode(Opcode::OP_0)
            .into_script();
        let selfi = self.clone();
        let fut = async move {
            let tx_fut = z_p2sh_spend(&selfi, tx, time_lock, SEQUENCE_FINAL, redeem_script, script_data);
            let tx = try_s!(tx_fut.await);
            Ok(tx.into())
        };
        Box::new(fut.boxed().compat())
    }

    fn send_taker_spends_maker_payment(
        &self,
        maker_payment_tx: &[u8],
        time_lock: u32,
        maker_pub: &[u8],
        secret: &[u8],
        _swap_contract_address: &Option<Bytes>,
    ) -> TransactionFut {
        let tx: UtxoTx = try_fus!(deserialize(maker_payment_tx).map_err(|e| ERRL!("{:?}", e)));
        let redeem_script = payment_script(
            time_lock,
            &*dhash160(secret),
            &Public::from_slice(maker_pub).unwrap(),
            self.utxo_arc.key_pair.public(),
        );
        let script_data = ScriptBuilder::default()
            .push_data(secret)
            .push_opcode(Opcode::OP_0)
            .into_script();
        let selfi = self.clone();
        let fut = async move {
            let tx_fut = z_p2sh_spend(&selfi, tx, time_lock, SEQUENCE_FINAL, redeem_script, script_data);
            let tx = try_s!(tx_fut.await);
            Ok(tx.into())
        };
        Box::new(fut.boxed().compat())
    }

    fn send_taker_refunds_payment(
        &self,
        taker_payment_tx: &[u8],
        time_lock: u32,
        maker_pub: &[u8],
        secret_hash: &[u8],
        _swap_contract_address: &Option<Bytes>,
    ) -> TransactionFut {
        let tx: UtxoTx = try_fus!(deserialize(taker_payment_tx).map_err(|e| ERRL!("{:?}", e)));
        let redeem_script = payment_script(
            time_lock,
            secret_hash,
            self.utxo_arc.key_pair.public(),
            &Public::from_slice(maker_pub).unwrap(),
        );
        let script_data = ScriptBuilder::default().push_opcode(Opcode::OP_1).into_script();
        let selfi = self.clone();
        let fut = async move {
            let tx_fut = z_p2sh_spend(&selfi, tx, time_lock, SEQUENCE_FINAL - 1, redeem_script, script_data);
            let tx = try_s!(tx_fut.await);
            Ok(tx.into())
        };
        Box::new(fut.boxed().compat())
    }

    fn send_maker_refunds_payment(
        &self,
        maker_payment_tx: &[u8],
        time_lock: u32,
        taker_pub: &[u8],
        secret_hash: &[u8],
        _swap_contract_address: &Option<Bytes>,
    ) -> TransactionFut {
        let tx: UtxoTx = try_fus!(deserialize(maker_payment_tx).map_err(|e| ERRL!("{:?}", e)));
        let redeem_script = payment_script(
            time_lock,
            secret_hash,
            self.utxo_arc.key_pair.public(),
            &Public::from_slice(taker_pub).unwrap(),
        );
        let script_data = ScriptBuilder::default().push_opcode(Opcode::OP_1).into_script();
        let selfi = self.clone();
        let fut = async move {
            let tx_fut = z_p2sh_spend(&selfi, tx, time_lock, SEQUENCE_FINAL - 1, redeem_script, script_data);
            let tx = try_s!(tx_fut.await);
            Ok(tx.into())
        };
        Box::new(fut.boxed().compat())
    }

    fn validate_fee(
        &self,
        _fee_tx: &TransactionEnum,
        _expected_sender: &[u8],
        _fee_addr: &[u8],
        _amount: &BigDecimal,
        _min_block_number: u64,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        todo!()
    }

    fn validate_maker_payment(
        &self,
        _payment_tx: &[u8],
        _time_lock: u32,
        _maker_pub: &[u8],
        _priv_bn_hash: &[u8],
        _amount: BigDecimal,
        _swap_contract_address: &Option<Bytes>,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        todo!()
    }

    fn validate_taker_payment(
        &self,
        _payment_tx: &[u8],
        _time_lock: u32,
        _taker_pub: &[u8],
        _priv_bn_hash: &[u8],
        _amount: BigDecimal,
        _swap_contract_address: &Option<Bytes>,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        todo!()
    }

    fn check_if_my_payment_sent(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<Bytes>,
    ) -> Box<dyn Future<Item = Option<TransactionEnum>, Error = String> + Send> {
        todo!()
    }

    fn search_for_swap_tx_spend_my(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _tx: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<Bytes>,
    ) -> Result<Option<FoundSwapTxSpend>, String> {
        todo!()
    }

    fn search_for_swap_tx_spend_other(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _tx: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<Bytes>,
    ) -> Result<Option<FoundSwapTxSpend>, String> {
        todo!()
    }

    fn extract_secret(&self, _secret_hash: &[u8], _spend_tx: &[u8]) -> Result<Vec<u8>, String> { todo!() }
}
