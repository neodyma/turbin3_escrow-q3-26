use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, program_pack::Pack},
        system_program::ID as SYSTEM_PROGRAM_ID,
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::associated_token::{self, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
    litesvm::{
        types::{TransactionMetadata, TransactionResult},
        LiteSVM,
    },
    litesvm_token::{
        spl_token::{self, ID as TOKEN_PROGRAM_ID},
        CreateAssociatedTokenAccount, CreateMint, MintTo,
    },
    solana_keypair::Keypair,
    solana_message::Message,
    solana_signer::Signer,
    solana_transaction::Transaction,
};

const SOL_BALANCE: u64 = 2_000_000_000;
const MAKER_TOKEN_A: u64 = 1_000_000_000;
const TAKER_TOKEN_B: u64 = 1_000_000_000;
const DEPOSIT_AMOUNT: u64 = 10_000_000;
const RECEIVE_AMOUNT: u64 = 20_000_000;
const UPDATED_RECEIVE_AMOUNT: u64 = 25_000_000;
const EXPIRATION: i64 = 17_780_206_209;

struct Fixture {
    svm: LiteSVM,
    maker: Keypair,
    taker: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    maker_ata_a: Pubkey,
    maker_ata_b: Pubkey,
    taker_ata_a: Pubkey,
    taker_ata_b: Pubkey,
}

impl Fixture {
    fn new() -> Self {
        let program_id = escrowq32026::id();
        let payer = Keypair::new();
        let maker = Keypair::new();
        let taker = Keypair::new();
        let mut svm = LiteSVM::new();

        let bytes = include_bytes!(concat!(
            env!("CARGO_TARGET_TMPDIR"),
            "/../deploy/escrowq32026.so"
        ));

        svm.add_program(program_id, bytes).unwrap();
        svm.airdrop(&payer.pubkey(), SOL_BALANCE).unwrap();
        svm.airdrop(&maker.pubkey(), SOL_BALANCE).unwrap();
        svm.airdrop(&taker.pubkey(), SOL_BALANCE).unwrap();

        let mint_authority = payer.pubkey();
        let mint_a = CreateMint::new(&mut svm, &payer)
            .decimals(6)
            .authority(&mint_authority)
            .send()
            .unwrap();
        let mint_b = CreateMint::new(&mut svm, &payer)
            .decimals(6)
            .authority(&mint_authority)
            .send()
            .unwrap();

        let maker_pubkey = maker.pubkey();
        let taker_pubkey = taker.pubkey();

        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_a)
            .owner(&maker_pubkey)
            .send()
            .unwrap();
        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_b)
            .owner(&taker_pubkey)
            .send()
            .unwrap();

        MintTo::new(&mut svm, &payer, &mint_a, &maker_ata_a, MAKER_TOKEN_A)
            .send()
            .unwrap();
        MintTo::new(&mut svm, &payer, &mint_b, &taker_ata_b, TAKER_TOKEN_B)
            .send()
            .unwrap();

        let maker_ata_b = associated_token::get_associated_token_address(&maker_pubkey, &mint_b);
        let taker_ata_a = associated_token::get_associated_token_address(&taker_pubkey, &mint_a);

        Self {
            svm,
            maker,
            taker,
            mint_a,
            mint_b,
            maker_ata_a,
            maker_ata_b,
            taker_ata_a,
            taker_ata_b,
        }
    }

    fn escrow_and_vault(&self, seed: u64) -> (Pubkey, Pubkey, u8) {
        let (escrow, bump) = Pubkey::find_program_address(
            &[b"escrow", self.maker.pubkey().as_ref(), &seed.to_le_bytes()],
            &escrowq32026::id(),
        );
        let vault = associated_token::get_associated_token_address(&escrow, &self.mint_a);

        (escrow, vault, bump)
    }

    fn make_instruction(
        &self,
        seed: u64,
        deposit: u64,
        receive: u64,
        mint_b: Pubkey,
    ) -> Instruction {
        let (escrow, vault, _) = self.escrow_and_vault(seed);

        Instruction {
            program_id: escrowq32026::id(),
            accounts: escrowq32026::accounts::Make {
                maker: self.maker.pubkey(),
                mint_a: self.mint_a,
                mint_b,
                maker_ata_a: self.maker_ata_a,
                escrow,
                vault,
                associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
                token_program: TOKEN_PROGRAM_ID,
                system_program: SYSTEM_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: escrowq32026::instruction::Make {
                seed,
                deposit,
                receive,
                expiration: EXPIRATION,
            }
            .data(),
        }
    }

    fn take_instruction(&self, seed: u64) -> Instruction {
        let (escrow, vault, _) = self.escrow_and_vault(seed);

        Instruction {
            program_id: escrowq32026::id(),
            accounts: escrowq32026::accounts::Take {
                taker: self.taker.pubkey(),
                maker: self.maker.pubkey(),
                mint_a: self.mint_a,
                mint_b: self.mint_b,
                taker_ata_a: self.taker_ata_a,
                taker_ata_b: self.taker_ata_b,
                maker_ata_b: self.maker_ata_b,
                escrow,
                vault,
                associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
                token_program: TOKEN_PROGRAM_ID,
                system_program: SYSTEM_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: escrowq32026::instruction::Take {}.data(),
        }
    }

    fn refund_instruction(&self, seed: u64, maker: Pubkey, maker_ata_a: Pubkey) -> Instruction {
        let (escrow, vault, _) = self.escrow_and_vault(seed);

        Instruction {
            program_id: escrowq32026::id(),
            accounts: escrowq32026::accounts::Refund {
                maker,
                mint_a: self.mint_a,
                maker_ata_a,
                escrow,
                vault,
                token_program: TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: escrowq32026::instruction::Refund {}.data(),
        }
    }

    fn update_instruction(&self, seed: u64, maker: Pubkey, receive: u64) -> Instruction {
        let (escrow, _, _) = self.escrow_and_vault(seed);

        Instruction {
            program_id: escrowq32026::id(),
            accounts: escrowq32026::accounts::Update { maker, escrow }.to_account_metas(None),
            data: escrowq32026::instruction::Update { receive }.data(),
        }
    }

    fn make(&mut self, seed: u64, deposit: u64, receive: u64) {
        let instruction = self.make_instruction(seed, deposit, receive, self.mint_b);
        send_ok(&mut self.svm, &self.maker, instruction);
    }
}

fn send_result(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> TransactionResult {
    let message = Message::new(&[instruction], Some(&signer.pubkey()));
    let transaction = Transaction::new(&[signer], message, svm.latest_blockhash());

    svm.send_transaction(transaction)
}

fn send_ok(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> TransactionMetadata {
    match send_result(svm, signer, instruction) {
        Ok(metadata) => metadata,
        Err(error) => panic!(
            "transaction failed: {:?}\nlogs: {:#?}",
            error.err, error.meta.logs
        ),
    }
}

fn assert_anchor_error(result: TransactionResult, expected_code: &str) {
    let error = result.expect_err("transaction should fail");
    let expected_log = format!("Error Code: {expected_code}");

    assert!(
        error
            .meta
            .logs
            .iter()
            .any(|log| log.contains(&expected_log)),
        "expected Anchor error {expected_code}, got {:?}\nlogs: {:#?}",
        error.err,
        error.meta.logs
    );
}

fn escrow_state(svm: &LiteSVM, escrow: &Pubkey) -> escrowq32026::state::Escrow {
    let account = svm.get_account(escrow).expect("escrow should exist");
    let mut data = account.data.as_slice();

    escrowq32026::state::Escrow::try_deserialize(&mut data).expect("escrow should deserialize")
}

fn token_account(svm: &LiteSVM, address: &Pubkey) -> spl_token::state::Account {
    let account = svm
        .get_account(address)
        .expect("token account should exist");

    spl_token::state::Account::unpack(&account.data).expect("token account should deserialize")
}

fn token_amount(svm: &LiteSVM, address: &Pubkey) -> u64 {
    token_account(svm, address).amount
}

#[test]
fn make_stores_terms_and_deposits_token_a() {
    let mut fixture = Fixture::new();
    let seed = 101;
    let (escrow, vault, bump) = fixture.escrow_and_vault(seed);

    fixture.make(seed, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let state_account = fixture.svm.get_account(&escrow).unwrap();
    assert_eq!(state_account.owner, escrowq32026::id());

    let state = escrow_state(&fixture.svm, &escrow);
    assert_eq!(state.seed, seed);
    assert_eq!(state.maker, fixture.maker.pubkey());
    assert_eq!(state.mint_a, fixture.mint_a);
    assert_eq!(state.mint_b, fixture.mint_b);
    assert_eq!(state.receive, RECEIVE_AMOUNT);
    assert_eq!(state.bump, bump);
    assert_eq!(state.expiration, EXPIRATION);

    let vault_state = token_account(&fixture.svm, &vault);
    assert_eq!(vault_state.amount, DEPOSIT_AMOUNT);
    assert_eq!(vault_state.owner, escrow);
    assert_eq!(vault_state.mint, fixture.mint_a);
    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_a),
        MAKER_TOKEN_A - DEPOSIT_AMOUNT
    );
}

#[test]
fn make_rejects_invalid_terms_without_creating_accounts() {
    let mut fixture = Fixture::new();
    let maker_a_before = token_amount(&fixture.svm, &fixture.maker_ata_a);

    let zero_deposit_seed = 201;
    let zero_deposit =
        fixture.make_instruction(zero_deposit_seed, 0, RECEIVE_AMOUNT, fixture.mint_b);
    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, zero_deposit),
        "InvalidDepositAmount",
    );
    let (escrow, vault, _) = fixture.escrow_and_vault(zero_deposit_seed);
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());

    let zero_receive_seed = 202;
    let zero_receive =
        fixture.make_instruction(zero_receive_seed, DEPOSIT_AMOUNT, 0, fixture.mint_b);
    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, zero_receive),
        "InvalidReceiveAmount",
    );
    let (escrow, vault, _) = fixture.escrow_and_vault(zero_receive_seed);
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());

    let identical_mints_seed = 203;
    let identical_mints = fixture.make_instruction(
        identical_mints_seed,
        DEPOSIT_AMOUNT,
        RECEIVE_AMOUNT,
        fixture.mint_a,
    );
    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, identical_mints),
        "IdenticalMints",
    );
    let (escrow, vault, _) = fixture.escrow_and_vault(identical_mints_seed);
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());

    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_a),
        maker_a_before,
        "rejected offers must not move token A"
    );
}

#[test]
fn update_changes_the_receive_amount_used_by_take() {
    let mut fixture = Fixture::new();
    let seed = 301;
    fixture.make(seed, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let before = escrow_state(&fixture.svm, &escrow);
    let vault_before = token_amount(&fixture.svm, &vault);
    let update = fixture.update_instruction(seed, fixture.maker.pubkey(), UPDATED_RECEIVE_AMOUNT);

    send_ok(&mut fixture.svm, &fixture.maker, update);

    let after = escrow_state(&fixture.svm, &escrow);
    assert_eq!(after.receive, UPDATED_RECEIVE_AMOUNT);
    assert_eq!(after.seed, before.seed);
    assert_eq!(after.maker, before.maker);
    assert_eq!(after.mint_a, before.mint_a);
    assert_eq!(after.mint_b, before.mint_b);
    assert_eq!(after.bump, before.bump);
    assert_eq!(after.expiration, before.expiration);
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);

    let take = fixture.take_instruction(seed);
    send_ok(&mut fixture.svm, &fixture.taker, take);

    assert_eq!(
        token_amount(&fixture.svm, &fixture.taker_ata_b),
        TAKER_TOKEN_B - UPDATED_RECEIVE_AMOUNT,
        "take should charge the updated receive amount"
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_b),
        UPDATED_RECEIVE_AMOUNT,
        "the maker should receive the updated amount"
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.taker_ata_a),
        DEPOSIT_AMOUNT,
        "the taker should receive the deposited token A amount"
    );
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());
}

#[test]
fn update_rejects_zero_receive_amount() {
    let mut fixture = Fixture::new();
    let seed = 302;
    fixture.make(seed, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let (escrow, _, _) = fixture.escrow_and_vault(seed);
    let update = fixture.update_instruction(seed, fixture.maker.pubkey(), 0);

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, update),
        "InvalidReceiveAmount",
    );
    assert_eq!(escrow_state(&fixture.svm, &escrow).receive, RECEIVE_AMOUNT);
}

#[test]
fn unauthorized_update_is_rejected() {
    let mut fixture = Fixture::new();
    let seed = 303;
    fixture.make(seed, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let (escrow, _, _) = fixture.escrow_and_vault(seed);
    let update = fixture.update_instruction(seed, fixture.taker.pubkey(), UPDATED_RECEIVE_AMOUNT);

    assert!(send_result(&mut fixture.svm, &fixture.taker, update).is_err());
    assert_eq!(escrow_state(&fixture.svm, &escrow).receive, RECEIVE_AMOUNT);
}

#[test]
fn take_swaps_exact_amounts_and_closes_the_offer() {
    let mut fixture = Fixture::new();
    let seed = 401;
    fixture.make(seed, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    assert!(fixture.svm.get_account(&fixture.maker_ata_b).is_none());
    assert!(fixture.svm.get_account(&fixture.taker_ata_a).is_none());

    let maker_lamports_before = fixture.svm.get_balance(&fixture.maker.pubkey()).unwrap();
    let taker_lamports_before = fixture.svm.get_balance(&fixture.taker.pubkey()).unwrap();
    let escrow_rent = fixture.svm.get_balance(&escrow).unwrap();
    let vault_rent = fixture.svm.get_balance(&vault).unwrap();
    let token_account_rent = fixture
        .svm
        .minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
    let take = fixture.take_instruction(seed);

    let metadata = send_ok(&mut fixture.svm, &fixture.taker, take);

    assert_eq!(
        token_amount(&fixture.svm, &fixture.taker_ata_b),
        TAKER_TOKEN_B - RECEIVE_AMOUNT
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_b),
        RECEIVE_AMOUNT
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.taker_ata_a),
        DEPOSIT_AMOUNT
    );
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());
    assert_eq!(
        fixture.svm.get_balance(&fixture.maker.pubkey()).unwrap(),
        maker_lamports_before + escrow_rent,
        "take should return escrow state rent to the maker"
    );
    assert_eq!(
        fixture.svm.get_balance(&fixture.taker.pubkey()).unwrap(),
        taker_lamports_before + vault_rent - metadata.fee - 2 * token_account_rent,
        "take should return vault rent to the taker after ATA creation costs"
    );
}

#[test]
fn underfunded_take_is_atomic() {
    let mut fixture = Fixture::new();
    let seed = 402;
    let unaffordable_receive = TAKER_TOKEN_B + 1;
    fixture.make(seed, DEPOSIT_AMOUNT, unaffordable_receive);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let taker_b_before = token_amount(&fixture.svm, &fixture.taker_ata_b);
    let vault_before = token_amount(&fixture.svm, &vault);
    let take = fixture.take_instruction(seed);

    assert!(send_result(&mut fixture.svm, &fixture.taker, take).is_err());

    assert_eq!(
        token_amount(&fixture.svm, &fixture.taker_ata_b),
        taker_b_before
    );
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);
    assert!(fixture.svm.get_account(&escrow).is_some());
    assert!(fixture.svm.get_account(&vault).is_some());
    assert!(fixture.svm.get_account(&fixture.maker_ata_b).is_none());
    assert!(fixture.svm.get_account(&fixture.taker_ata_a).is_none());
}

#[test]
fn refund_returns_all_token_a_and_closes_the_offer() {
    let mut fixture = Fixture::new();
    let seed = 501;
    fixture.make(seed, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let maker_lamports_before = fixture.svm.get_balance(&fixture.maker.pubkey()).unwrap();
    let escrow_rent = fixture.svm.get_balance(&escrow).unwrap();
    let vault_rent = fixture.svm.get_balance(&vault).unwrap();
    let refund = fixture.refund_instruction(seed, fixture.maker.pubkey(), fixture.maker_ata_a);

    let metadata = send_ok(&mut fixture.svm, &fixture.maker, refund);

    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_a),
        MAKER_TOKEN_A
    );
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());
    assert_eq!(
        fixture.svm.get_balance(&fixture.maker.pubkey()).unwrap(),
        maker_lamports_before + escrow_rent + vault_rent - metadata.fee,
        "refund should return escrow and vault rent, minus the transaction fee"
    );
}

#[test]
fn unauthorized_refund_is_rejected() {
    let mut fixture = Fixture::new();
    let seed = 502;
    fixture.make(seed, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let taker_pubkey = fixture.taker.pubkey();
    let taker_ata_a =
        CreateAssociatedTokenAccount::new(&mut fixture.svm, &fixture.taker, &fixture.mint_a)
            .owner(&taker_pubkey)
            .send()
            .unwrap();

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let maker_a_before = token_amount(&fixture.svm, &fixture.maker_ata_a);
    let vault_before = token_amount(&fixture.svm, &vault);
    let refund = fixture.refund_instruction(seed, taker_pubkey, taker_ata_a);

    assert!(send_result(&mut fixture.svm, &fixture.taker, refund).is_err());

    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_a),
        maker_a_before
    );
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);
    assert_eq!(token_amount(&fixture.svm, &taker_ata_a), 0);
    assert!(fixture.svm.get_account(&escrow).is_some());
    assert!(fixture.svm.get_account(&vault).is_some());
}
