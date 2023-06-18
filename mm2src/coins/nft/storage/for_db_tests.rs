use crate::nft::nft_structs::{Chain, ContractType, Nft, NftTransferHistory, NftTxHistoryFilters, TransferStatus,
                              UriMeta};
use crate::nft::storage::{NftListStorageOps, NftStorageBuilder, NftTxHistoryStorageOps, RemoveNftResult};
use mm2_number::BigDecimal;
use mm2_test_helpers::for_tests::mm_ctx_with_custom_db;
use std::num::NonZeroUsize;
use std::str::FromStr;

cfg_wasm32! {
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);
}

const TOKEN_ADD: &str = "0xfd913a305d70a60aac4faac70c739563738e1f81";
const TOKEN_ID: &str = "214300044414";
const TX_HASH: &str = "0x1e9f04e9b571b283bde02c98c2a97da39b2bb665b57c1f2b0b733f9b681debbe";

fn nft() -> Nft {
    Nft {
        chain: Chain::Bsc,
        token_address: "0x5c7d6712dfaf0cb079d48981781c8705e8417ca0".to_string(),
        token_id: Default::default(),
        amount: BigDecimal::from_str("2").unwrap(),
        owner_of: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
        token_hash: "b34ddf294013d20a6d70691027625839".to_string(),
        block_number_minted: 25465916,
        block_number: 25919780,
        contract_type: ContractType::Erc1155,
        collection_name: None,
        symbol: None,
        token_uri: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.json".to_string()),
        metadata: Some("{\"name\":\"Tiki box\"}".to_string()),
        last_token_uri_sync: Some("2023-02-07T17:10:08.402Z".to_string()),
        last_metadata_sync: Some("2023-02-07T17:10:16.858Z".to_string()),
        minter_address: Some("ERC1155 tokens don't have a single minter".to_string()),
        possible_spam: Some(false),
        uri_meta: UriMeta {
            image: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.png".to_string()),
            token_name: None,
            description: Some("Born to usher in Bull markets.".to_string()),
            attributes: None,
            animation_url: None,
        },
    }
}

fn nft_list() -> Vec<Nft> {
    let nft = Nft {
        chain: Chain::Bsc,
        token_address: "0x5c7d6712dfaf0cb079d48981781c8705e8417ca0".to_string(),
        token_id: Default::default(),
        amount: BigDecimal::from_str("2").unwrap(),
        owner_of: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
        token_hash: "b34ddf294013d20a6d70691027625839".to_string(),
        block_number_minted: 25465916,
        block_number: 25919780,
        contract_type: ContractType::Erc1155,
        collection_name: None,
        symbol: None,
        token_uri: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.json".to_string()),
        metadata: Some("{\"name\":\"Tiki box\"}".to_string()),
        last_token_uri_sync: Some("2023-02-07T17:10:08.402Z".to_string()),
        last_metadata_sync: Some("2023-02-07T17:10:16.858Z".to_string()),
        minter_address: Some("ERC1155 tokens don't have a single minter".to_string()),
        possible_spam: Some(false),
        uri_meta: UriMeta {
            image: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.png".to_string()),
            token_name: None,
            description: Some("Born to usher in Bull markets.".to_string()),
            attributes: None,
            animation_url: None,
        },
    };

    let nft1 = Nft {
        chain: Chain::Bsc,
        token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
        token_id: BigDecimal::from_str("214300047252").unwrap(),
        amount: BigDecimal::from_str("1").unwrap(),
        owner_of: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
        token_hash: "c5d1cfd75a0535b0ec750c0156e6ddfe".to_string(),
        block_number_minted: 25721963,
        block_number: 28056726,
        contract_type: ContractType::Erc721,
        collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
        symbol: Some("BMBBBF".to_string()),
        token_uri: Some("https://public.nftstatic.com/static/nft/BSC/BMBBBF/214300047252".to_string()),
        metadata: Some(
            "{\"image\":\"https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png\"}"
                .to_string(),
        ),
        last_token_uri_sync: Some("2023-02-16T16:35:52.392Z".to_string()),
        last_metadata_sync: Some("2023-02-16T16:36:04.283Z".to_string()),
        minter_address: Some("0xdbdeb0895f3681b87fb3654b5cf3e05546ba24a9".to_string()),
        possible_spam: Some(false),
        uri_meta: UriMeta {
            image: Some("https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string()),
            token_name: Some("Nebula Nodes".to_string()),
            description: Some("Interchain nodes".to_string()),
            attributes: None,
            animation_url: None,
        },
    };

    let nft2 = Nft {
        chain: Chain::Bsc,
        token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
        token_id: BigDecimal::from_str("214300044414").unwrap(),
        amount: BigDecimal::from_str("1").unwrap(),
        owner_of: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
        token_hash: "125f8f4e952e107c257960000b4b250e".to_string(),
        block_number_minted: 25810308,
        block_number: 28056721,
        contract_type: ContractType::Erc721,
        collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
        symbol: Some("BMBBBF".to_string()),
        token_uri: Some("https://public.nftstatic.com/static/nft/BSC/BMBBBF/214300044414".to_string()),
        metadata: Some(
            "{\"image\":\"https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png\"}"
                .to_string(),
        ),
        last_token_uri_sync: Some("2023-02-19T19:12:09.471Z".to_string()),
        last_metadata_sync: Some("2023-02-19T19:12:18.080Z".to_string()),
        minter_address: Some("0xdbdeb0895f3681b87fb3654b5cf3e05546ba24a9".to_string()),
        possible_spam: Some(false),
        uri_meta: UriMeta {
            image: Some("https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string()),
            token_name: Some("Nebula Nodes".to_string()),
            description: Some("Interchain nodes".to_string()),
            attributes: None,
            animation_url: None,
        },
    };
    vec![nft, nft1, nft2]
}

fn nft_tx_historty() -> Vec<NftTransferHistory> {
    let tx = NftTransferHistory {
        chain: Chain::Bsc,
        block_number: 25919780,
        block_timestamp: 1677166110,
        block_hash: "0xcb41654fc5cf2bf5d7fd3f061693405c74d419def80993caded0551ecfaeaae5".to_string(),
        transaction_hash: "0x9c16b962f63eead1c5d2355cc9037dde178b14b53043c57eb40c27964d22ae6a".to_string(),
        transaction_index: 57,
        log_index: 139,
        value: Default::default(),
        contract_type: ContractType::Erc1155,
        transaction_type: "Single".to_string(),
        token_address: "0x5c7d6712dfaf0cb079d48981781c8705e8417ca0".to_string(),
        token_id: Default::default(),
        collection_name: None,
        image: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.png".to_string()),
        token_name: None,
        from_address: "0x4ff0bbc9b64d635a4696d1a38554fb2529c103ff".to_string(),
        to_address: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
        status: TransferStatus::Receive,
        amount: BigDecimal::from_str("1").unwrap(),
        verified: 1,
        operator: Some("0x4ff0bbc9b64d635a4696d1a38554fb2529c103ff".to_string()),
        possible_spam: Some(false),
    };

    let tx1 = NftTransferHistory {
        chain: Chain::Bsc,
        block_number: 28056726,
        block_timestamp: 1683627432,
        block_hash: "0x3d68b78391fb3cf8570df27036214f7e9a5a6a45d309197936f51d826041bfe7".to_string(),
        transaction_hash: "0x1e9f04e9b571b283bde02c98c2a97da39b2bb665b57c1f2b0b733f9b681debbe".to_string(),
        transaction_index: 198,
        log_index: 495,
        value: Default::default(),
        contract_type: ContractType::Erc721,
        transaction_type: "Single".to_string(),
        token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
        token_id: BigDecimal::from_str("214300047252").unwrap(),
        collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
        image: Some("https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string()),
        token_name: Some("Nebula Nodes".to_string()),
        from_address: "0x6fad0ec6bb76914b2a2a800686acc22970645820".to_string(),
        to_address: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
        status: TransferStatus::Receive,
        amount: BigDecimal::from_str("1").unwrap(),
        verified: 1,
        operator: None,
        possible_spam: Some(false),
    };

    let tx2 = NftTransferHistory {
        chain: Chain::Bsc,
        block_number: 28056721,
        block_timestamp: 1683627417,
        block_hash: "0x326db41c5a4fd5f033676d95c590ced18936ef2ef6079e873b23af087fd966c6".to_string(),
        transaction_hash: "0x981bad702cc6e088f0e9b5e7287ff7a3487b8d269103cee3b9e5803141f63f91".to_string(),
        transaction_index: 83,
        log_index: 201,
        value: Default::default(),
        contract_type: ContractType::Erc721,
        transaction_type: "Single".to_string(),
        token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
        token_id: BigDecimal::from_str("214300044414").unwrap(),
        collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
        image: Some("https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string()),
        token_name: Some("Nebula Nodes".to_string()),
        from_address: "0x6fad0ec6bb76914b2a2a800686acc22970645820".to_string(),
        to_address: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
        status: TransferStatus::Receive,
        amount: BigDecimal::from_str("1").unwrap(),
        verified: 1,
        operator: None,
        possible_spam: Some(false),
    };
    vec![tx, tx1, tx2]
}

pub(crate) async fn test_add_get_nfts_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftListStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftListStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let nft_list = nft_list();
    storage.add_nfts_to_list(&chain, nft_list, 28056726).await.unwrap();

    let token_id = BigDecimal::from_str(TOKEN_ID).unwrap();
    let nft = storage
        .get_nft(&chain, TOKEN_ADD.to_string(), token_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(nft.block_number, 28056721);
}

pub(crate) async fn test_last_nft_blocks_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftListStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftListStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let nft_list = nft_list();
    storage.add_nfts_to_list(&chain, nft_list, 28056726).await.unwrap();

    let last_scanned_block = storage.get_last_scanned_block(&chain).await.unwrap().unwrap();
    let last_block = NftListStorageOps::get_last_block_number(&storage, &chain)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_scanned_block, last_block);
}

pub(crate) async fn test_nft_list_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftListStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftListStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let nft_list = nft_list();
    storage.add_nfts_to_list(&chain, nft_list, 28056726).await.unwrap();

    let nft_list = storage
        .get_nft_list(vec![chain], false, 1, Some(NonZeroUsize::new(2).unwrap()))
        .await
        .unwrap();
    assert_eq!(nft_list.nfts.len(), 1);
    let nft = nft_list.nfts.get(0).unwrap();
    assert_eq!(nft.block_number, 28056721);
    assert_eq!(nft_list.skipped, 1);
    assert_eq!(nft_list.total, 3);
}

pub(crate) async fn test_remove_nft_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftListStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftListStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let nft_list = nft_list();
    storage.add_nfts_to_list(&chain, nft_list, 28056726).await.unwrap();

    let token_id = BigDecimal::from_str(TOKEN_ID).unwrap();
    let remove_rslt = storage
        .remove_nft_from_list(&chain, TOKEN_ADD.to_string(), token_id, 28056800)
        .await
        .unwrap();
    assert_eq!(remove_rslt, RemoveNftResult::NftRemoved);
    let list_len = storage
        .get_nft_list(vec![chain], true, 1, None)
        .await
        .unwrap()
        .nfts
        .len();
    assert_eq!(list_len, 2);
    let last_scanned_block = storage.get_last_scanned_block(&chain).await.unwrap().unwrap();
    assert_eq!(last_scanned_block, 28056800);
}

pub(crate) async fn test_nft_amount_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftListStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftListStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let mut nft = nft();
    storage
        .add_nfts_to_list(&chain, vec![nft.clone()], 25919780)
        .await
        .unwrap();

    nft.amount -= BigDecimal::from(1);
    storage.update_nft_amount(&chain, nft.clone(), 25919800).await.unwrap();
    let amount = storage
        .get_nft_amount(&chain, nft.token_address.clone(), nft.token_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(amount, "1");
    let last_scanned_block = storage.get_last_scanned_block(&chain).await.unwrap().unwrap();
    assert_eq!(last_scanned_block, 25919800);

    nft.amount += BigDecimal::from(1);
    nft.block_number = 25919900;
    storage
        .update_nft_amount_and_block_number(&chain, nft.clone())
        .await
        .unwrap();
    let amount = storage
        .get_nft_amount(&chain, nft.token_address, nft.token_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(amount, "2");
    let last_scanned_block = storage.get_last_scanned_block(&chain).await.unwrap().unwrap();
    assert_eq!(last_scanned_block, 25919900);
}

pub(crate) async fn test_refresh_metadata_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftListStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftListStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);

    let new_symbol = "NEW_SYMBOL";
    let mut nft = nft();
    storage
        .add_nfts_to_list(&chain, vec![nft.clone()], 25919780)
        .await
        .unwrap();
    nft.symbol = Some(new_symbol.to_string());
    drop_mutability!(nft);
    let token_add = nft.token_address.clone();
    let token_id = nft.token_id.clone();
    storage.refresh_nft_metadata(&chain, nft).await.unwrap();
    let nft_upd = storage.get_nft(&chain, token_add, token_id).await.unwrap().unwrap();
    assert_eq!(new_symbol.to_string(), nft_upd.symbol.unwrap());
}

pub(crate) async fn test_add_get_txs_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftTxHistoryStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftTxHistoryStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let txs = nft_tx_historty();
    storage.add_txs_to_history(&chain, txs).await.unwrap();
    let token_id = BigDecimal::from_str(TOKEN_ID).unwrap();
    let tx1 = storage
        .get_txs_by_token_addr_id(&chain, TOKEN_ADD.to_string(), token_id)
        .await
        .unwrap()
        .get(0)
        .unwrap()
        .clone();
    assert_eq!(tx1.block_number, 28056721);
    let tx2 = storage
        .get_tx_by_tx_hash(&chain, TX_HASH.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tx2.block_number, 28056726);
    let tx_from = storage.get_txs_from_block(&chain, 28056721).await.unwrap();
    assert_eq!(tx_from.len(), 2);
}

pub(crate) async fn test_last_tx_block_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftTxHistoryStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftTxHistoryStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let txs = nft_tx_historty();
    storage.add_txs_to_history(&chain, txs).await.unwrap();

    let last_block = NftTxHistoryStorageOps::get_last_block_number(&storage, &chain)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_block, 28056726);
}

pub(crate) async fn test_tx_history_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftTxHistoryStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftTxHistoryStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let txs = nft_tx_historty();
    storage.add_txs_to_history(&chain, txs).await.unwrap();

    let tx_history = storage
        .get_tx_history(vec![chain], false, 1, Some(NonZeroUsize::new(2).unwrap()), None)
        .await
        .unwrap();
    assert_eq!(tx_history.transfer_history.len(), 1);
    let tx = tx_history.transfer_history.get(0).unwrap();
    assert_eq!(tx.block_number, 28056721);
    assert_eq!(tx_history.skipped, 1);
    assert_eq!(tx_history.total, 3);
}

pub(crate) async fn test_tx_history_filters_impl() {
    let ctx = mm_ctx_with_custom_db();
    let storage = NftStorageBuilder::new(&ctx).build().unwrap();
    let chain = Chain::Bsc;
    NftTxHistoryStorageOps::init(&storage, &chain).await.unwrap();
    let is_initialized = NftTxHistoryStorageOps::is_initialized(&storage, &chain).await.unwrap();
    assert!(is_initialized);
    let txs = nft_tx_historty();
    storage.add_txs_to_history(&chain, txs).await.unwrap();

    let filters = NftTxHistoryFilters {
        receive: true,
        send: false,
        from_date: None,
        to_date: None,
    };

    let filters1 = NftTxHistoryFilters {
        receive: false,
        send: false,
        from_date: None,
        to_date: Some(1677166110),
    };

    let filters2 = NftTxHistoryFilters {
        receive: false,
        send: false,
        from_date: Some(1677166110),
        to_date: Some(1683627417),
    };

    let tx_history = storage
        .get_tx_history(vec![chain], true, 1, None, Some(filters))
        .await
        .unwrap();
    assert_eq!(tx_history.transfer_history.len(), 3);
    let tx = tx_history.transfer_history.get(0).unwrap();
    assert_eq!(tx.block_number, 28056726);

    let tx_history1 = storage
        .get_tx_history(vec![chain], true, 1, None, Some(filters1))
        .await
        .unwrap();
    assert_eq!(tx_history1.transfer_history.len(), 1);
    let tx1 = tx_history1.transfer_history.get(0).unwrap();
    assert_eq!(tx1.block_number, 25919780);

    let tx_history2 = storage
        .get_tx_history(vec![chain], true, 1, None, Some(filters2))
        .await
        .unwrap();
    assert_eq!(tx_history2.transfer_history.len(), 2);
    let tx_0 = tx_history2.transfer_history.get(0).unwrap();
    assert_eq!(tx_0.block_number, 28056721);
    let tx_1 = tx_history2.transfer_history.get(1).unwrap();
    assert_eq!(tx_1.block_number, 25919780);
}