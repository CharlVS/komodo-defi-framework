use common::{
    for_tests::MarketMakerIt,
    block_on,
    mm_ctx::MmCtxBuilder,
    privkey::key_pair_from_seed,
};
use crate::{
    mm2::lp_bot::simple_market_maker_bot::process_price_request,
    mm2::lp_bot::start_simple_market_maker_bot,
    mm2::lp_bot::stop_simple_market_maker_bot,
    mm2::lp_bot::simple_market_maker_bot::StartSimpleMakerBotRequest,
};
use http::StatusCode;
use serde_json::{Value as Json};

mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_process_price_request() {
        let resp = block_on(process_price_request()).unwrap();
        assert_eq!(resp.0, StatusCode::OK);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_start_and_stop_simple_market_maker_bot_from_ctx() {
        let ctx = MmCtxBuilder::default()
            .with_secp256k1_key_pair(
                key_pair_from_seed("also shoot benefit prefer juice shell elder veteran woman mimic image kidney").unwrap(),
            )
            .into_mm_arc();

        let mut req = StartSimpleMakerBotRequest::new();
        let cloned_ctx = ctx.clone();
        let another_cloned_ctx = ctx.clone();
        let answer = block_on(start_simple_market_maker_bot(ctx, req)).unwrap();
        assert_eq!(answer.get_result(), "Success");
        req = StartSimpleMakerBotRequest::new();
        let answer = block_on(start_simple_market_maker_bot(cloned_ctx, req));
        assert!(answer.is_err());
        let answer = block_on(stop_simple_market_maker_bot(another_cloned_ctx, Json::default())).unwrap();
        assert_eq!(answer.get_result(), "Success");
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_start_and_stop_simple_market_maker_bot() {
        let coins = json!([
        {"coin":"RICK","asset":"RICK","rpcport":8923,"txversion":4,"protocol":{"type":"UTXO"}},
    ]);

        let mm = MarketMakerIt::start(json!({
            "gui": "nogui",
            "netid": 9998,
            "passphrase": "bob passphrase",
            "rpc_password": "password",
            "coins": coins,
            "i_am_seed": true,
        }), "password".into(), None)
            .unwrap();
        let (_dump_log, _dump_dashboard) = mm.mm_dump();
        log!({"Log path: {}", mm.log_path.display()});
        let functor = || block_on(mm.rpc(json!({
         "userpass": "password",
         "mmrpc": "2.0",
         "method": "start_simple_market_maker_bot",
         "params": {
         "cfg": {
            "KMD-BEP20/BUSD-BEP20": {
                "base": "KMD-BEP20",
                "rel": "BUSD-BEP20",
                "max": true,
                "min_volume": "0.25",
                "spread": "1.025",
                "base_confs": 3,
                "base_nota": false,
                "rel_confs": 1,
                "rel_nota": false,
                "enable": true
            }
        }
    },
    "id": 0
    }))).unwrap();
        let mut start_simple_market_maker_bot = functor();

        // Must be 200
        assert_eq!(start_simple_market_maker_bot.0, 200);

        // Let's repeat - should get an already started
        start_simple_market_maker_bot = functor();

        // Must be 400
        assert_eq!(start_simple_market_maker_bot.0, 400);

        let functor = || block_on(mm.rpc(json!({
    "userpass": "password",
    "mmrpc": "2.0",
    "method": "stop_simple_market_maker_bot",
    "params": null,
    "id": 0
}))).unwrap();

        let mut stop_simple_market_maker_bot = functor();
        // Must be 200
        assert_eq!(stop_simple_market_maker_bot.0, 200);

        stop_simple_market_maker_bot = functor();

        assert_eq!(stop_simple_market_maker_bot.0, 400);
    }
}