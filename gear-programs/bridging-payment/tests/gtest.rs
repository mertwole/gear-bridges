use bridging_payment_client::{
    traits::*, BridgingPayment as BridgingPaymentC,
    BridgingPaymentFactory as BridgingPaymentFactoryC, Config, InitConfig,
};
use extended_vft_client::{traits::*, ExtendedVftFactory as VftFactoryC, Vft as VftC};
use gtest::{Log, Program, System, WasmProgram};
use sails_rs::{calls::*, gtest::calls::*, prelude::*};
use vft_manager_client::{
    traits::*, Config as VftManagerConfig, InitConfig as VftManagerInitConfig, TokenSupply,
    VftManager as VftManagerC, VftManagerFactory as VftManagerFactoryC,
};

const ADMIN_ID: u64 = 1000;
const FEE: u128 = 10_000_000_000_000;
const HISTORICAL_PROXY_ID: u64 = 500;
const BRIDGE_BUILTIN_ID: u64 = 300;

#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
enum Response {
    MessageSent { nonce: U256, hash: H256 },
}

#[derive(Debug)]
struct GearBridgeBuiltinMock;

impl WasmProgram for GearBridgeBuiltinMock {
    fn init(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> {
        Ok(None)
    }

    fn handle(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> {
        Ok(Some(
            Response::MessageSent {
                nonce: U256::from(1),
                hash: [1; 32].into(),
            }
            .encode(),
        ))
    }

    fn handle_reply(&mut self, _payload: Vec<u8>) -> Result<(), &'static str> {
        unimplemented!()
    }

    fn handle_signal(&mut self, _payload: Vec<u8>) -> Result<(), &'static str> {
        unimplemented!()
    }

    fn state(&mut self) -> Result<Vec<u8>, &'static str> {
        unimplemented!()
    }
}

async fn balance_of(
    remoting: &GTestRemoting,
    vft_program_id: ActorId,
    program_id: ActorId,
) -> U256 {
    VftC::new(remoting.clone())
        .balance_of(program_id)
        .recv(vft_program_id)
        .await
        .unwrap()
}

struct Fixture {
    remoting: GTestRemoting,
    bridging_payment_program_id: ActorId,
    vft_manager_program_id: ActorId,
    vft_program_id: ActorId,
}

async fn setup_for_test() -> Fixture {
    let system = System::new();
    system.init_logger();
    system.mint_to(ADMIN_ID, 100_000_000_000_000);

    let remoting = GTestRemoting::new(system, ADMIN_ID.into());

    // Bridge Builtin
    let gear_bridge_builtin =
        Program::mock_with_id(remoting.system(), BRIDGE_BUILTIN_ID, GearBridgeBuiltinMock);
    let _ = gear_bridge_builtin.send_bytes(ADMIN_ID, b"INIT");

    // Vft-manager
    let treasury_code_id = remoting.system().submit_code(vft_manager::WASM_BINARY);
    let init_config = VftManagerInitConfig {
        erc20_manager_address: [1; 20].into(),
        gear_bridge_builtin: BRIDGE_BUILTIN_ID.into(),
        historical_proxy_address: HISTORICAL_PROXY_ID.into(),
        config: VftManagerConfig {
            gas_for_token_ops: 15_000_000_000,
            gas_for_reply_deposit: 15_000_000_000,
            gas_to_send_request_to_builtin: 15_000_000_000,
            reply_timeout: 100,
        },
    };
    let vft_manager_program_id = VftManagerFactoryC::new(remoting.clone())
        .new(init_config)
        .send_recv(treasury_code_id, b"salt")
        .await
        .unwrap();

    // VFT
    let vft_code_id = remoting.system().submit_code(extended_vft::WASM_BINARY);
    let vft_program_id = VftFactoryC::new(remoting.clone())
        .new("Token".into(), "Token".into(), 18)
        .send_recv(vft_code_id, b"salt")
        .await
        .unwrap();

    // Bridging payment with Vara supply
    let bridging_payment_code_id = remoting.system().submit_code(bridging_payment::WASM_BINARY);
    let init_config = InitConfig {
        admin_address: ADMIN_ID.into(),
        vft_manager_address: vft_manager_program_id,
        config: Config {
            fee: FEE,
            gas_for_reply_deposit: 15_000_000_000,
            gas_to_send_request_to_vft_manager: 115_000_000_000,
            reply_timeout: 1000,
            gas_for_request_to_vft_manager_msg: 50_000_000_000,
        },
    };
    let bridging_payment_program_id = BridgingPaymentFactoryC::new(remoting.clone())
        .new(init_config)
        .send_recv(bridging_payment_code_id, b"salt")
        .await
        .unwrap();

    Fixture {
        remoting,
        bridging_payment_program_id,
        vft_manager_program_id,
        vft_program_id,
    }
}

#[tokio::test]
async fn deposit_to_treasury() {
    let Fixture {
        remoting,
        bridging_payment_program_id,
        vft_manager_program_id,
        vft_program_id,
    } = setup_for_test().await;

    let account_id: ActorId = 10000.into();
    let amount = U256::from(10_000_000_000_u64);
    let eth_token_id: H160 = [2; 20].into();

    let mut vft = VftC::new(remoting.clone());

    let ok = vft
        .mint(account_id, amount)
        .send_recv(vft_program_id)
        .await
        .unwrap();
    assert!(ok);

    vft.grant_burner_role(vft_manager_program_id)
        .send_recv(vft_program_id)
        .await
        .unwrap();

    let mut service = VftManagerC::new(remoting.clone());
    service
        .map_vara_to_eth_address(vft_program_id, eth_token_id, TokenSupply::Ethereum)
        .send_recv(vft_manager_program_id)
        .await
        .unwrap();
    service
        .update_fee_charger(Some(bridging_payment_program_id))
        .send_recv(vft_manager_program_id)
        .await
        .unwrap();

    remoting.system().mint_to(account_id, 10 * FEE);
    BridgingPaymentC::new(remoting.clone().with_actor_id(account_id))
        .make_request(amount, [1; 20].into(), vft_program_id)
        .with_value(FEE)
        .send_recv(bridging_payment_program_id)
        .await
        .unwrap();

    assert!(balance_of(&remoting, vft_program_id, account_id)
        .await
        .is_zero(),);
    assert!(
        balance_of(&remoting, vft_program_id, vft_manager_program_id)
            .await
            .is_zero()
    );

    // Claim fee
    BridgingPaymentC::new(remoting.clone())
        .reclaim_fee()
        .send_recv(bridging_payment_program_id)
        .await
        .unwrap();

    let balance_before = remoting.system().balance_of(ADMIN_ID);
    remoting
        .system()
        .get_mailbox(ADMIN_ID)
        .claim_value(Log::builder().dest(ADMIN_ID))
        .unwrap();
    assert!(remoting.system().balance_of(ADMIN_ID) > balance_before + FEE / 4 * 3);
}
