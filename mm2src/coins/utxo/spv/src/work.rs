pub mod btc;

pub use btc::*;

use crate::conf::SPVBlockHeader;
use crate::storage::{BlockHeaderStorageError, BlockHeaderStorageOps};
use crate::work::btc::{btc_mainnet_next_block_bits, btc_testnet_next_block_bits};
use chain::BlockHeaderBits;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::cmp;

/// The maximum value for bits corresponding to lowest difficulty of 1
pub(crate) const LTC_MAX_BITS: u32 = 504365040;

/// The Target number of blocks equals to 2 weeks or 2016 blocks (for btc).
const fn retarget_interval(target_timespan_secs: u32, target_spacing_secs: u32) -> u32 {
    target_timespan_secs / target_spacing_secs
}

const fn is_retarget_height(height: u64, target_timespan_secs: u32, target_spacing_secs: u32) -> bool {
    height % retarget_interval(target_timespan_secs, target_spacing_secs) as u64 == 0
}

#[derive(Clone, Debug, Display, Eq, PartialEq)]
pub enum NextBlockBitsError {
    #[display(fmt = "Block headers storage error: {}", _0)]
    StorageError(BlockHeaderStorageError),
    #[display(fmt = "Can't find Block header for {} with height {}", coin, height)]
    NoSuchBlockHeader { coin: String, height: u64 },
    #[display(fmt = "Can't find a Block header for {} with no max bits", coin)]
    NoBlockHeaderWithNoMaxBits { coin: String },
}

impl From<BlockHeaderStorageError> for NextBlockBitsError {
    fn from(e: BlockHeaderStorageError) -> Self { NextBlockBitsError::StorageError(e) }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum DifficultyAlgorithm {
    #[serde(rename = "Bitcoin Mainnet")]
    BitcoinMainnet,
    #[serde(rename = "Bitcoin Testnet")]
    BitcoinTestnet,
    #[serde(rename = "Litecoin Mainnet")]
    LitecoinMainnet,
}

impl DifficultyAlgorithm {
    pub(crate) fn max_bits(&self) -> u32 {
        match self {
            DifficultyAlgorithm::BitcoinMainnet | DifficultyAlgorithm::BitcoinTestnet => BTC_MAX_BITS,
            DifficultyAlgorithm::LitecoinMainnet => LTC_MAX_BITS,
        }
    }

    pub(crate) fn min_max_timespan(&self) -> (i64, i64) {
        match self {
            DifficultyAlgorithm::BitcoinMainnet | DifficultyAlgorithm::BitcoinTestnet => (MIN_TIMESPAN, MAX_TIMESPAN),
            DifficultyAlgorithm::LitecoinMainnet => (MIN_TIMESPAN / 4, MAX_TIMESPAN / 4),
        }
    }

    pub(crate) fn target_timespan_spacing_secs(&self) -> (u32, u32) {
        match self {
            DifficultyAlgorithm::BitcoinMainnet | DifficultyAlgorithm::BitcoinTestnet => {
                (TARGET_TIMESPAN_SECONDS, TARGET_SPACING_SECONDS)
            },
            DifficultyAlgorithm::LitecoinMainnet => (TARGET_TIMESPAN_SECONDS / 4, TARGET_SPACING_SECONDS / 4),
        }
    }
}

pub async fn next_block_bits(
    coin: &str,
    current_block_timestamp: u32,
    last_block_header: SPVBlockHeader,
    storage: &dyn BlockHeaderStorageOps,
    algorithm: &DifficultyAlgorithm,
) -> Result<BlockHeaderBits, NextBlockBitsError> {
    match algorithm {
        DifficultyAlgorithm::BitcoinMainnet | DifficultyAlgorithm::LitecoinMainnet => {
            btc_mainnet_next_block_bits(coin, last_block_header, storage, algorithm).await
        },
        DifficultyAlgorithm::BitcoinTestnet => {
            btc_testnet_next_block_bits(coin, current_block_timestamp, last_block_header, storage, algorithm).await
        },
    }
}

fn range_constrain(value: i64, min: i64, max: i64) -> i64 { cmp::min(cmp::max(value, min), max) }

/// Returns constrained number of seconds since last retarget
fn retarget_timespan(retarget_timestamp: u32, last_timestamp: u32, algorithm: &DifficultyAlgorithm) -> u32 {
    // subtract unsigned 32 bit numbers in signed 64 bit space in
    // order to prevent underflow before applying the range constraint.
    let (min_timespan, max_timespan) = algorithm.min_max_timespan();
    let timespan = last_timestamp as i64 - retarget_timestamp as i64;
    range_constrain(timespan, min_timespan, max_timespan) as u32
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::storage::{BlockHeaderStorageError, BlockHeaderStorageOps};
    use async_trait::async_trait;
    use chain::BlockHeader;
    use common::block_on;
    use lazy_static::lazy_static;
    use primitives::hash::H256;
    use serde::Deserialize;
    use std::collections::HashMap;

    const BLOCK_HEADERS_STR: &str = include_str!("for_tests/workTestVectors.json");

    #[derive(Deserialize)]
    struct TestRawHeader {
        height: u64,
        hex: String,
    }

    lazy_static! {
        static ref BLOCK_HEADERS_MAP: HashMap<String, Vec<TestRawHeader>> = parse_block_headers();
    }

    fn parse_block_headers() -> HashMap<String, Vec<TestRawHeader>> { serde_json::from_str(BLOCK_HEADERS_STR).unwrap() }

    fn get_block_headers_for_coin(coin: &str) -> HashMap<u64, BlockHeader> {
        BLOCK_HEADERS_MAP
            .get(coin)
            .unwrap()
            .into_iter()
            .map(|h| (h.height, h.hex.as_str().into()))
            .collect()
    }

    pub struct TestBlockHeadersStorage {
        pub(crate) ticker: String,
    }

    #[async_trait]
    impl BlockHeaderStorageOps for TestBlockHeadersStorage {
        async fn init(&self) -> Result<(), BlockHeaderStorageError> { Ok(()) }

        async fn is_initialized_for(&self) -> Result<bool, BlockHeaderStorageError> { Ok(true) }

        async fn add_block_headers_to_storage(
            &self,
            _headers: HashMap<u64, BlockHeader>,
        ) -> Result<(), BlockHeaderStorageError> {
            Ok(())
        }

        async fn get_block_header(&self, height: u64) -> Result<Option<BlockHeader>, BlockHeaderStorageError> {
            Ok(get_block_headers_for_coin(&self.ticker).get(&height).cloned())
        }

        async fn get_block_header_raw(&self, _height: u64) -> Result<Option<String>, BlockHeaderStorageError> {
            Ok(None)
        }

        async fn get_last_block_height(&self) -> Result<Option<u64>, BlockHeaderStorageError> {
            Ok(Some(
                get_block_headers_for_coin(&self.ticker)
                    .into_keys()
                    .max_by(|a, b| a.cmp(b))
                    .unwrap(),
            ))
        }

        async fn get_last_block_header_with_non_max_bits(
            &self,
            max_bits: u32,
        ) -> Result<Option<BlockHeader>, BlockHeaderStorageError> {
            let mut headers = get_block_headers_for_coin(&self.ticker);
            headers.retain(|_, h| h.bits != BlockHeaderBits::Compact(max_bits.into()));
            let header = headers.into_iter().max_by(|a, b| a.0.cmp(&b.0));
            Ok(header.map(|(_, h)| h))
        }

        async fn get_block_height_by_hash(&self, _hash: H256) -> Result<Option<i64>, BlockHeaderStorageError> {
            Ok(None)
        }

        async fn remove_headers_up_to_height(&self, _height: u64) -> Result<(), BlockHeaderStorageError> { Ok(()) }

        async fn is_table_empty(&self) -> Result<(), BlockHeaderStorageError> { Ok(()) }
    }

    #[test]
    fn test_btc_mainnet_next_block_bits() {
        let storage = TestBlockHeadersStorage { ticker: "BTC".into() };

        let last_header: BlockHeader = "000000201d758432ecd495a2177b44d3fe6c22af183461a0b9ea0d0000000000000000008283a1dfa795d9b68bd8c18601e443368265072cbf8c76bfe58de46edd303798035de95d3eb2151756fdb0e8".into();

        let next_block_bits = block_on(btc_mainnet_next_block_bits(
            "BTC",
            SPVBlockHeader::from_block_header_and_height(&last_header, 606815),
            &storage,
            &DifficultyAlgorithm::BitcoinMainnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(387308498.into()));

        // check that bits for very early blocks that didn't change difficulty because of low hashrate is calculated correctly.
        let last_header: BlockHeader = "010000000d9c8c96715756b619116cc2160937fb26c655a2f8e28e3a0aff59c0000000007676252e8434de408ea31920d986aba297bd6f7c6f20756be08748713f7c135962719449ffff001df8c1cb01".into();

        let next_block_bits = block_on(btc_mainnet_next_block_bits(
            "BTC",
            SPVBlockHeader::from_block_header_and_height(&last_header, 4031),
            &storage,
            &DifficultyAlgorithm::BitcoinMainnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(486604799.into()));

        // check that bits stay the same when the next block is not a retarget block
        // https://live.blockcypher.com/btc/block/00000000000000000002622f52b6afe70a5bb139c788e67f221ffc67a762a1e0/
        let last_header: BlockHeader = "00e0ff2f44d953fe12a047129bbc7164668c6d96f3e7a553528b02000000000000000000d0b950384cd23ab0854d1c8f23fa7a97411a6ffd92347c0a3aea4466621e4093ec09c762afa7091705dad220".into();

        let next_block_bits = block_on(btc_mainnet_next_block_bits(
            "BTC",
            SPVBlockHeader::from_block_header_and_height(&last_header, 744014),
            &storage,
            &DifficultyAlgorithm::BitcoinMainnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(386508719.into()));
    }

    #[test]
    fn test_btc_testnet_next_block_bits() {
        let storage = TestBlockHeadersStorage { ticker: "tBTC".into() };

        // https://live.blockcypher.com/btc-testnet/block/000000000057db3806384e2ec1b02b2c86bd928206ff8dff98f54d616b7fa5f2/
        let current_header: BlockHeader = "02000000303505969a1df329e5fccdf69b847a201772e116e557eb7f119d1a9600000000469267f52f43b8799e72f0726ba2e56432059a8ad02b84d4fff84b9476e95f7716e41353ab80011c168cb471".into();
        // https://live.blockcypher.com/btc-testnet/block/00000000961a9d117feb57e516e17217207a849bf6cdfce529f31d9a96053530/
        let last_header: BlockHeader = "02000000ea01a61a2d7420a1b23875e40eb5eb4ca18b378902c8e6384514ad0000000000c0c5a1ae80582b3fe319d8543307fa67befc2a734b8eddb84b1780dfdf11fa2b20e71353ffff001d00805fe0".into();

        let next_block_bits = block_on(btc_testnet_next_block_bits(
            "tBTC",
            current_header.time,
            SPVBlockHeader::from_block_header_and_height(&last_header, 201595),
            &storage,
            &DifficultyAlgorithm::BitcoinTestnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(469860523.into()));

        // https://live.blockcypher.com/btc-testnet/block/00000000961a9d117feb57e516e17217207a849bf6cdfce529f31d9a96053530/
        let current_header: BlockHeader = "02000000ea01a61a2d7420a1b23875e40eb5eb4ca18b378902c8e6384514ad0000000000c0c5a1ae80582b3fe319d8543307fa67befc2a734b8eddb84b1780dfdf11fa2b20e71353ffff001d00805fe0".into();
        // https://live.blockcypher.com/btc-testnet/block/0000000000ad144538e6c80289378ba14cebb50ee47538b2a120742d1aa601ea/
        let last_header: BlockHeader = "02000000cbed7fd98f1f06e85c47e13ff956533642056be45e7e6b532d4d768f00000000f2680982f333fcc9afa7f9a5e2a84dc54b7fe10605cd187362980b3aa882e9683be21353ab80011c813e1fc0".into();

        let next_block_bits = block_on(btc_testnet_next_block_bits(
            "tBTC",
            current_header.time,
            SPVBlockHeader::from_block_header_and_height(&last_header, 201594),
            &storage,
            &DifficultyAlgorithm::BitcoinTestnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(486604799.into()));

        // test testnet retarget bits

        // https://live.blockcypher.com/btc-testnet/block/0000000000376bb71314321c45de3015fe958543afcbada242a3b1b072498e38/
        let current_header: BlockHeader = "02000000ee689e4dcdc3c7dac591b98e1e4dc83aae03ff9fb9d469d704a64c0100000000bfffaded2a67821eb5729b362d613747e898d08d6c83b5704646c26c13146f4c6de91353c02a601b3a817f87".into();
        // https://live.blockcypher.com/btc-testnet/block/00000000014ca604d769d4b99fff03ae3ac84d1e8eb991c5dac7c3cd4d9e68ee/
        let last_header: BlockHeader = "02000000a9dccfcf372d6ce6ae784786ea94d20ce174e093520d779348e5a9000000000077c037863a0134ac05a8c19d258d6c03c225043a08687c90813e8352a144d68035e81353ab80011ca71f3849".into();

        let next_block_bits = block_on(btc_testnet_next_block_bits(
            "tBTC",
            current_header.time,
            SPVBlockHeader::from_block_header_and_height(&last_header, 201599),
            &storage,
            &DifficultyAlgorithm::BitcoinTestnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(459287232.into()));
    }

    ////////////////////////// test litecoin difficulty
    #[test]
    fn test_ltc_mainnet_next_block_bits() {
        let storage = TestBlockHeadersStorage { ticker: "LTC".into() };

        let last_header: BlockHeader = "010000009351a333166426787133ad869d43e43bbe6fba358056d2744c4ab1dc1c81c185c5807523d7c52c3ec52d9c645f0d161bff049a6ae680ca77fc90e433a0c4f8cab598974ec0ff3f1db4040000".into();
        let height = 10001;

        let next_block_bits = block_on(btc_mainnet_next_block_bits(
            "LTC",
            SPVBlockHeader::from_block_header_and_height(&last_header, height),
            &storage,
            &DifficultyAlgorithm::LitecoinMainnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(490733504.into()));

        // check that bits for very early blocks that didn't change difficulty because of low hashrate is calculated correctly.
        let last_header: BlockHeader = "01000000bf49880c4c962a96b8cbac3e1e30fcd4459ad57ab367bbcb98d0fdfa2737b50b71cb69000261d2aa4cb13fdf380011bb34b6c4c110059522e108e045e9b2c479ab7d964effff0f1eb7000000".into();
        let height = 4031;

        let next_block_bits = block_on(btc_mainnet_next_block_bits(
            "BTC",
            SPVBlockHeader::from_block_header_and_height(&last_header, height),
            &storage,
            &DifficultyAlgorithm::LitecoinMainnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(503578623.into()));

        // check that bits stay the same when the next block is not a retarget block
        // https://live.blockcypher.com/btc/block/00000000000000000002622f52b6afe70a5bb139c788e67f221ffc67a762a1e0/
        let last_header: BlockHeader = "0100000065f3882810549f82c399b2eaaea8d8a077504875d77effad271d1525d6057ab6c600f6ddfc42762998e9a50abd0083929499c5b591dc169370e179449ea60a3fa87d964effff0f1ef1070000".into();
        let height = 4030;

        let next_block_bits = block_on(btc_mainnet_next_block_bits(
            "BTC",
            SPVBlockHeader::from_block_header_and_height(&last_header, height),
            &storage,
            &DifficultyAlgorithm::LitecoinMainnet,
        ))
        .unwrap();
        assert_eq!(next_block_bits, BlockHeaderBits::Compact(504365055.into()));
    }
}
