const NFT_LIST_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/0x394d86994f954ed931b86791b62fe64f4c5dac37/nft?chain=POLYGON&format=decimal";
const NFT_HISTORY_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/0x394d86994f954ed931b86791b62fe64f4c5dac37/nft/transfers?chain=POLYGON&format=decimal&direction=both";
const NFT_METADATA_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/nft/0xed55e4477b795eaa9bb4bca24df42214e1a05c18/1111777?chain=POLYGON&format=decimal";
const TEST_WALLET_ADDR_EVM: &str = "0x394d86994f954ed931b86791b62fe64f4c5dac37";

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use crate::nft::nft_structs::{NftTransferHistoryWrapper, NftWrapper};
    use crate::nft::nft_tests::{NFT_HISTORY_URL_TEST, NFT_LIST_URL_TEST, NFT_METADATA_URL_TEST, TEST_WALLET_ADDR_EVM};
    use crate::nft::send_moralis_request;
    use common::block_on;

    #[test]
    fn test_moralis_nft_list() {
        let response = block_on(send_moralis_request(NFT_LIST_URL_TEST)).unwrap();
        let nfts_list = response["result"].as_array().unwrap();
        assert_eq!(2, nfts_list.len());
        for nft_json in nfts_list {
            let nft_wrapper: NftWrapper = serde_json::from_str(&nft_json.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, nft_wrapper.owner_of)
        }
    }

    #[test]
    fn test_moralis_nft_transfer_history() {
        let response = block_on(send_moralis_request(NFT_HISTORY_URL_TEST)).unwrap();
        let transfer_list = response["result"].as_array().unwrap();
        assert_eq!(2, transfer_list.len());
        for transfer in transfer_list {
            let transfer_wrapper: NftTransferHistoryWrapper = serde_json::from_str(&transfer.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, transfer_wrapper.to_address);
        }
    }

    #[test]
    fn test_moralis_nft_metadata() {
        let response = block_on(send_moralis_request(NFT_METADATA_URL_TEST)).unwrap();
        let nft_wrapper: NftWrapper = serde_json::from_str(&response.to_string()).unwrap();
        assert_eq!(41237364, *nft_wrapper.block_number_minted)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use crate::nft::nft_structs::{NftTransferHistoryWrapper, NftWrapper};
    use crate::nft::nft_tests::{NFT_HISTORY_URL_TEST, NFT_LIST_URL_TEST, NFT_METADATA_URL_TEST, TEST_WALLET_ADDR_EVM};
    use crate::nft::send_moralis_request;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_moralis_nft_list() {
        let response = send_moralis_request(NFT_LIST_URL_TEST).await.unwrap();
        let nfts_list = response["result"].as_array().unwrap();
        assert_eq!(2, nfts_list.len());
        for nft_json in nfts_list {
            let nft_wrapper: NftWrapper = serde_json::from_str(&nft_json.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, nft_wrapper.owner_of)
        }
    }

    #[wasm_bindgen_test]
    async fn test_moralis_nft_transfer_history() {
        let response = send_moralis_request(NFT_HISTORY_URL_TEST).await.unwrap();
        let transfer_list = response["result"].as_array().unwrap();
        assert_eq!(2, transfer_list.len());
        for transfer in transfer_list {
            let transfer_wrapper: NftTransferHistoryWrapper = serde_json::from_str(&transfer.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, transfer_wrapper.to_address);
        }
    }

    #[wasm_bindgen_test]
    async fn test_moralis_nft_metadata() {
        let response = send_moralis_request(NFT_METADATA_URL_TEST).await.unwrap();
        let nft_wrapper: NftWrapper = serde_json::from_str(&response.to_string()).unwrap();
        assert_eq!(41237364, *nft_wrapper.block_number_minted)
    }
}