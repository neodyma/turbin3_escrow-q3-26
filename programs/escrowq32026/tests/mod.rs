use {
    anchor_lang::{
        prelude::{Clock, Pubkey},
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
const MINIMUM_RECEIVE_AMOUNT: u64 = 10_000_000;
const UPDATED_RECEIVE_AMOUNT: u64 = 25_000_000;
const NOW: i64 = 1_800_000_000;
const STARTS_AT: i64 = NOW + 100;
const EXCLUSIVE_UNTIL: i64 = STARTS_AT + 100;
const DECAY_ENDS_AT: i64 = EXCLUSIVE_UNTIL + 100;
const EXPIRATION: i64 = DECAY_ENDS_AT + 100;
const UPDATED_STARTS_AT: i64 = STARTS_AT + 50;

type AuctionTerms = escrowq32026::state::AuctionTerms;

trait TestAuctionTerms {
    fn reserved(preferred_taker: Pubkey) -> Self;
    fn public() -> Self;
}

impl TestAuctionTerms for AuctionTerms {
    fn reserved(preferred_taker: Pubkey) -> Self {
        Self {
            receive: RECEIVE_AMOUNT,
            minimum_receive: MINIMUM_RECEIVE_AMOUNT,
            preferred_taker: Some(preferred_taker),
            starts_at: STARTS_AT,
            exclusive_until: EXCLUSIVE_UNTIL,
            decay_ends_at: DECAY_ENDS_AT,
            expiration: EXPIRATION,
        }
    }

    fn public() -> Self {
        Self {
            preferred_taker: None,
            exclusive_until: STARTS_AT,
            ..Self::reserved(Pubkey::default())
        }
    }
}

struct Fixture {
    svm: LiteSVM,
    maker: Keypair,
    taker: Keypair,
    other_taker: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    maker_ata_a: Pubkey,
    maker_ata_b: Pubkey,
    taker_ata_a: Pubkey,
    taker_ata_b: Pubkey,
    other_taker_ata_a: Pubkey,
    other_taker_ata_b: Pubkey,
}

impl Fixture {
    fn new() -> Self {
        let program_id = escrowq32026::id();
        let payer = Keypair::new();
        let maker = Keypair::new();
        let taker = Keypair::new();
        let other_taker = Keypair::new();
        let mut svm = LiteSVM::new();

        let mut clock = svm.get_sysvar::<Clock>();
        clock.unix_timestamp = NOW;
        svm.set_sysvar::<Clock>(&clock);

        let bytes = include_bytes!(concat!(
            env!("CARGO_TARGET_TMPDIR"),
            "/../deploy/escrowq32026.so"
        ));

        svm.add_program(program_id, bytes).unwrap();
        svm.airdrop(&payer.pubkey(), SOL_BALANCE).unwrap();
        svm.airdrop(&maker.pubkey(), SOL_BALANCE).unwrap();
        svm.airdrop(&taker.pubkey(), SOL_BALANCE).unwrap();
        svm.airdrop(&other_taker.pubkey(), SOL_BALANCE).unwrap();

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
        let other_taker_pubkey = other_taker.pubkey();

        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_a)
            .owner(&maker_pubkey)
            .send()
            .unwrap();
        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_b)
            .owner(&taker_pubkey)
            .send()
            .unwrap();
        let other_taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_b)
            .owner(&other_taker_pubkey)
            .send()
            .unwrap();

        MintTo::new(&mut svm, &payer, &mint_a, &maker_ata_a, MAKER_TOKEN_A)
            .send()
            .unwrap();
        MintTo::new(&mut svm, &payer, &mint_b, &taker_ata_b, TAKER_TOKEN_B)
            .send()
            .unwrap();
        MintTo::new(&mut svm, &payer, &mint_b, &other_taker_ata_b, TAKER_TOKEN_B)
            .send()
            .unwrap();

        let maker_ata_b = associated_token::get_associated_token_address(&maker_pubkey, &mint_b);
        let taker_ata_a = associated_token::get_associated_token_address(&taker_pubkey, &mint_a);
        let other_taker_ata_a =
            associated_token::get_associated_token_address(&other_taker_pubkey, &mint_a);

        Self {
            svm,
            maker,
            taker,
            other_taker,
            mint_a,
            mint_b,
            maker_ata_a,
            maker_ata_b,
            taker_ata_a,
            taker_ata_b,
            other_taker_ata_a,
            other_taker_ata_b,
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
        terms: AuctionTerms,
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
                terms,
            }
            .data(),
        }
    }

    fn take_instruction(&self, seed: u64) -> Instruction {
        self.take_instruction_for(
            seed,
            self.taker.pubkey(),
            self.taker_ata_a,
            self.taker_ata_b,
        )
    }

    fn other_taker_instruction(&self, seed: u64) -> Instruction {
        self.take_instruction_for(
            seed,
            self.other_taker.pubkey(),
            self.other_taker_ata_a,
            self.other_taker_ata_b,
        )
    }

    fn take_instruction_for(
        &self,
        seed: u64,
        taker: Pubkey,
        taker_ata_a: Pubkey,
        taker_ata_b: Pubkey,
    ) -> Instruction {
        let (escrow, vault, _) = self.escrow_and_vault(seed);

        Instruction {
            program_id: escrowq32026::id(),
            accounts: escrowq32026::accounts::Take {
                taker,
                maker: self.maker.pubkey(),
                mint_a: self.mint_a,
                mint_b: self.mint_b,
                taker_ata_a,
                taker_ata_b,
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

    fn update_instruction(
        &self,
        seed: u64,
        maker: Pubkey,
        receive: u64,
        starts_at: i64,
    ) -> Instruction {
        let (escrow, _, _) = self.escrow_and_vault(seed);

        Instruction {
            program_id: escrowq32026::id(),
            accounts: escrowq32026::accounts::Update { maker, escrow }.to_account_metas(None),
            data: escrowq32026::instruction::Update { receive, starts_at }.data(),
        }
    }

    fn make(&mut self, seed: u64, deposit: u64, terms: AuctionTerms) {
        let instruction = self.make_instruction(seed, deposit, terms, self.mint_b);
        send_ok(&mut self.svm, &self.maker, instruction);
    }

    fn set_time(&mut self, unix_timestamp: i64) {
        let mut clock = self.svm.get_sysvar::<Clock>();
        clock.unix_timestamp = unix_timestamp;
        self.svm.set_sysvar::<Clock>(&clock);
    }
}

#[allow(clippy::result_large_err)]
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

fn assert_offer_absent(fixture: &Fixture, seed: u64) {
    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());
}

#[test]
fn make_stores_auction_terms_and_deposits_token_a() {
    let mut fixture = Fixture::new();
    let seed = 101;
    let (escrow, vault, bump) = fixture.escrow_and_vault(seed);
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());

    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let state_account = fixture.svm.get_account(&escrow).unwrap();
    assert_eq!(state_account.owner, escrowq32026::id());

    let state = escrow_state(&fixture.svm, &escrow);
    assert_eq!(state.seed, seed);
    assert_eq!(state.maker, fixture.maker.pubkey());
    assert_eq!(state.mint_a, fixture.mint_a);
    assert_eq!(state.mint_b, fixture.mint_b);
    assert_eq!(state.receive, RECEIVE_AMOUNT);
    assert_eq!(state.minimum_receive, MINIMUM_RECEIVE_AMOUNT);
    assert_eq!(state.preferred_taker, Some(fixture.taker.pubkey()));
    assert_eq!(state.starts_at, STARTS_AT);
    assert_eq!(state.exclusive_until, EXCLUSIVE_UNTIL);
    assert_eq!(state.decay_ends_at, DECAY_ENDS_AT);
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
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());

    let zero_deposit_seed = 201;
    let zero_deposit = fixture.make_instruction(zero_deposit_seed, 0, terms, fixture.mint_b);
    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, zero_deposit),
        "InvalidDepositAmount",
    );
    let (escrow, vault, _) = fixture.escrow_and_vault(zero_deposit_seed);
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());

    let zero_receive_seed = 202;
    let zero_receive = fixture.make_instruction(
        zero_receive_seed,
        DEPOSIT_AMOUNT,
        AuctionTerms {
            receive: 0,
            ..terms
        },
        fixture.mint_b,
    );
    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, zero_receive),
        "InvalidReceiveAmount",
    );
    let (escrow, vault, _) = fixture.escrow_and_vault(zero_receive_seed);
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());

    let identical_mints_seed = 203;
    let identical_mints =
        fixture.make_instruction(identical_mints_seed, DEPOSIT_AMOUNT, terms, fixture.mint_a);
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
fn make_rejects_invalid_auction_prices_and_schedule_atomically() {
    let mut fixture = Fixture::new();
    let maker_a_before = token_amount(&fixture.svm, &fixture.maker_ata_a);
    let valid = AuctionTerms::reserved(fixture.taker.pubkey());
    let cases = [
        (
            211,
            AuctionTerms {
                minimum_receive: 0,
                ..valid
            },
            "InvalidMinimumReceiveAmount",
        ),
        (
            212,
            AuctionTerms {
                minimum_receive: RECEIVE_AMOUNT + 1,
                ..valid
            },
            "InvalidPriceRange",
        ),
        (
            213,
            AuctionTerms {
                starts_at: NOW,
                ..valid
            },
            "InvalidAuctionSchedule",
        ),
        (
            214,
            AuctionTerms {
                preferred_taker: None,
                ..valid
            },
            "InvalidAuctionSchedule",
        ),
        (
            215,
            AuctionTerms {
                exclusive_until: STARTS_AT,
                ..valid
            },
            "InvalidAuctionSchedule",
        ),
        (
            216,
            AuctionTerms {
                decay_ends_at: EXCLUSIVE_UNTIL,
                ..valid
            },
            "InvalidAuctionSchedule",
        ),
        (
            217,
            AuctionTerms {
                expiration: DECAY_ENDS_AT,
                ..valid
            },
            "InvalidAuctionSchedule",
        ),
    ];

    for (seed, terms, expected_error) in cases {
        let instruction = fixture.make_instruction(seed, DEPOSIT_AMOUNT, terms, fixture.mint_b);
        assert_anchor_error(
            send_result(&mut fixture.svm, &fixture.maker, instruction),
            expected_error,
        );
        assert_offer_absent(&fixture, seed);
    }

    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_a),
        maker_a_before,
        "invalid auction terms must not move token A"
    );
}

#[test]
fn update_changes_the_opening_price_and_reschedules_the_auction() {
    let mut fixture = Fixture::new();
    let seed = 301;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let before = escrow_state(&fixture.svm, &escrow);
    let vault_before = token_amount(&fixture.svm, &vault);
    let update = fixture.update_instruction(
        seed,
        fixture.maker.pubkey(),
        UPDATED_RECEIVE_AMOUNT,
        UPDATED_STARTS_AT,
    );

    send_ok(&mut fixture.svm, &fixture.maker, update);

    let after = escrow_state(&fixture.svm, &escrow);
    assert_eq!(after.receive, UPDATED_RECEIVE_AMOUNT);
    assert_eq!(after.seed, before.seed);
    assert_eq!(after.maker, before.maker);
    assert_eq!(after.mint_a, before.mint_a);
    assert_eq!(after.mint_b, before.mint_b);
    assert_eq!(after.minimum_receive, before.minimum_receive);
    assert_eq!(after.preferred_taker, before.preferred_taker);
    assert_eq!(after.starts_at, UPDATED_STARTS_AT);
    assert_eq!(after.exclusive_until, EXCLUSIVE_UNTIL + 50);
    assert_eq!(after.decay_ends_at, DECAY_ENDS_AT + 50);
    assert_eq!(after.bump, before.bump);
    assert_eq!(after.expiration, EXPIRATION + 50);
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);

    fixture.set_time(UPDATED_STARTS_AT);
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
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let (escrow, _, _) = fixture.escrow_and_vault(seed);
    let update = fixture.update_instruction(seed, fixture.maker.pubkey(), 0, STARTS_AT);

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, update),
        "InvalidReceiveAmount",
    );
    assert_eq!(escrow_state(&fixture.svm, &escrow).receive, RECEIVE_AMOUNT);
}

#[test]
fn update_rejects_an_opening_price_below_the_floor() {
    let mut fixture = Fixture::new();
    let seed = 304;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let (escrow, _, _) = fixture.escrow_and_vault(seed);
    let update = fixture.update_instruction(
        seed,
        fixture.maker.pubkey(),
        MINIMUM_RECEIVE_AMOUNT - 1,
        STARTS_AT,
    );

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, update),
        "InvalidPriceRange",
    );
    assert_eq!(escrow_state(&fixture.svm, &escrow).receive, RECEIVE_AMOUNT);
}

#[test]
fn update_rejects_a_non_future_start_without_changing_the_offer() {
    let mut fixture = Fixture::new();
    let seed = 306;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let vault_before = token_amount(&fixture.svm, &vault);
    let update =
        fixture.update_instruction(seed, fixture.maker.pubkey(), UPDATED_RECEIVE_AMOUNT, NOW);

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, update),
        "InvalidAuctionSchedule",
    );

    let after = escrow_state(&fixture.svm, &escrow);
    assert_eq!(after.receive, RECEIVE_AMOUNT);
    assert_eq!(after.starts_at, STARTS_AT);
    assert_eq!(after.exclusive_until, EXCLUSIVE_UNTIL);
    assert_eq!(after.decay_ends_at, DECAY_ENDS_AT);
    assert_eq!(after.expiration, EXPIRATION);
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);
}

#[test]
fn update_rejects_timestamp_overflow_without_changing_the_offer() {
    let mut fixture = Fixture::new();
    let seed = 307;
    let terms = AuctionTerms {
        starts_at: i64::MAX - 300,
        exclusive_until: i64::MAX - 200,
        decay_ends_at: i64::MAX - 100,
        expiration: i64::MAX,
        ..AuctionTerms::reserved(fixture.taker.pubkey())
    };
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let vault_before = token_amount(&fixture.svm, &vault);
    let update = fixture.update_instruction(
        seed,
        fixture.maker.pubkey(),
        UPDATED_RECEIVE_AMOUNT,
        i64::MAX - 50,
    );

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, update),
        "InvalidAuctionSchedule",
    );

    let after = escrow_state(&fixture.svm, &escrow);
    assert_eq!(after.receive, terms.receive);
    assert_eq!(after.starts_at, terms.starts_at);
    assert_eq!(after.exclusive_until, terms.exclusive_until);
    assert_eq!(after.decay_ends_at, terms.decay_ends_at);
    assert_eq!(after.expiration, terms.expiration);
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);
}

#[test]
fn unauthorized_update_is_rejected() {
    let mut fixture = Fixture::new();
    let seed = 303;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let (escrow, _, _) = fixture.escrow_and_vault(seed);
    let update = fixture.update_instruction(
        seed,
        fixture.taker.pubkey(),
        UPDATED_RECEIVE_AMOUNT,
        UPDATED_STARTS_AT,
    );

    assert!(send_result(&mut fixture.svm, &fixture.taker, update).is_err());
    assert_eq!(escrow_state(&fixture.svm, &escrow).receive, RECEIVE_AMOUNT);
}

#[test]
fn update_after_the_auction_starts_is_rejected() {
    let mut fixture = Fixture::new();
    let seed = 305;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(STARTS_AT);

    let (escrow, _, _) = fixture.escrow_and_vault(seed);
    let update = fixture.update_instruction(
        seed,
        fixture.maker.pubkey(),
        UPDATED_RECEIVE_AMOUNT,
        UPDATED_STARTS_AT,
    );

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, update),
        "AuctionAlreadyStarted",
    );
    assert_eq!(escrow_state(&fixture.svm, &escrow).receive, RECEIVE_AMOUNT);
}

#[test]
fn take_before_the_auction_starts_is_atomic() {
    let mut fixture = Fixture::new();
    let seed = 403;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let taker_b_before = token_amount(&fixture.svm, &fixture.taker_ata_b);
    let vault_before = token_amount(&fixture.svm, &vault);
    let take = fixture.take_instruction(seed);

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.taker, take),
        "AuctionNotStarted",
    );
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
fn exclusive_window_rejects_a_non_preferred_taker_atomically() {
    let mut fixture = Fixture::new();
    let seed = 404;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(STARTS_AT);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let other_b_before = token_amount(&fixture.svm, &fixture.other_taker_ata_b);
    let vault_before = token_amount(&fixture.svm, &vault);
    let take = fixture.other_taker_instruction(seed);

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.other_taker, take),
        "PreferredTakerOnly",
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.other_taker_ata_b),
        other_b_before
    );
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);
    assert!(fixture.svm.get_account(&escrow).is_some());
    assert!(fixture.svm.get_account(&vault).is_some());
    assert!(fixture.svm.get_account(&fixture.maker_ata_b).is_none());
    assert!(fixture
        .svm
        .get_account(&fixture.other_taker_ata_a)
        .is_none());
}

#[test]
fn take_swaps_exact_amounts_and_closes_the_offer() {
    let mut fixture = Fixture::new();
    let seed = 401;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(STARTS_AT);

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
fn public_auction_without_a_preferred_taker_starts_immediately() {
    let mut fixture = Fixture::new();
    let seed = 405;
    let terms = AuctionTerms::public();
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(STARTS_AT);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let take = fixture.take_instruction(seed);
    send_ok(&mut fixture.svm, &fixture.taker, take);

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
}

#[test]
fn reserved_auction_becomes_public_at_the_exclusive_boundary() {
    let mut fixture = Fixture::new();
    let seed = 409;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(EXCLUSIVE_UNTIL);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let take = fixture.other_taker_instruction(seed);
    send_ok(&mut fixture.svm, &fixture.other_taker, take);

    assert_eq!(
        token_amount(&fixture.svm, &fixture.other_taker_ata_b),
        TAKER_TOKEN_B - RECEIVE_AMOUNT
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_b),
        RECEIVE_AMOUNT,
        "the public phase should begin at the opening price"
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.other_taker_ata_a),
        DEPOSIT_AMOUNT
    );
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());
}

#[test]
fn public_take_pays_the_linearly_decayed_price_rounded_for_the_maker() {
    let mut fixture = Fixture::new();
    let seed = 406;
    let terms = AuctionTerms {
        receive: RECEIVE_AMOUNT + 1,
        ..AuctionTerms::reserved(fixture.taker.pubkey())
    };
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

    let elapsed = 33;
    fixture.set_time(EXCLUSIVE_UNTIL + elapsed);
    let expected_receive = terms.receive
        - ((u128::from(terms.receive - terms.minimum_receive) * elapsed as u128
            / (DECAY_ENDS_AT - EXCLUSIVE_UNTIL) as u128) as u64);
    assert_eq!(expected_receive, 16_700_001);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let take = fixture.other_taker_instruction(seed);
    send_ok(&mut fixture.svm, &fixture.other_taker, take);

    assert_eq!(
        token_amount(&fixture.svm, &fixture.other_taker_ata_b),
        TAKER_TOKEN_B - expected_receive
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_b),
        expected_receive
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.other_taker_ata_a),
        DEPOSIT_AMOUNT
    );
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());
}

#[test]
fn public_take_pays_the_floor_price_after_decay() {
    let mut fixture = Fixture::new();
    let seed = 407;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(DECAY_ENDS_AT);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let take = fixture.other_taker_instruction(seed);
    send_ok(&mut fixture.svm, &fixture.other_taker, take);

    assert_eq!(
        token_amount(&fixture.svm, &fixture.other_taker_ata_b),
        TAKER_TOKEN_B - MINIMUM_RECEIVE_AMOUNT
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_b),
        MINIMUM_RECEIVE_AMOUNT
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.other_taker_ata_a),
        DEPOSIT_AMOUNT
    );
    assert!(fixture.svm.get_account(&escrow).is_none());
    assert!(fixture.svm.get_account(&vault).is_none());
}

#[test]
fn take_at_expiration_is_rejected_atomically() {
    let mut fixture = Fixture::new();
    let seed = 408;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(EXPIRATION);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let taker_b_before = token_amount(&fixture.svm, &fixture.taker_ata_b);
    let vault_before = token_amount(&fixture.svm, &vault);
    let take = fixture.take_instruction(seed);

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.taker, take),
        "EscrowExpired",
    );
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
fn underfunded_take_is_atomic() {
    let mut fixture = Fixture::new();
    let seed = 402;
    let unaffordable_receive = TAKER_TOKEN_B + 1;
    let terms = AuctionTerms {
        receive: unaffordable_receive,
        ..AuctionTerms::reserved(fixture.taker.pubkey())
    };
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(STARTS_AT);

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
fn refund_before_start_returns_all_token_a_and_closes_the_offer() {
    let mut fixture = Fixture::new();
    let seed = 501;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

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
fn refund_while_the_auction_is_live_is_atomic() {
    let mut fixture = Fixture::new();
    let seed = 503;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(STARTS_AT);

    let (escrow, vault, _) = fixture.escrow_and_vault(seed);
    let maker_a_before = token_amount(&fixture.svm, &fixture.maker_ata_a);
    let vault_before = token_amount(&fixture.svm, &vault);
    let refund = fixture.refund_instruction(seed, fixture.maker.pubkey(), fixture.maker_ata_a);

    assert_anchor_error(
        send_result(&mut fixture.svm, &fixture.maker, refund),
        "AuctionLive",
    );
    assert_eq!(
        token_amount(&fixture.svm, &fixture.maker_ata_a),
        maker_a_before
    );
    assert_eq!(token_amount(&fixture.svm, &vault), vault_before);
    assert!(fixture.svm.get_account(&escrow).is_some());
    assert!(fixture.svm.get_account(&vault).is_some());
}

#[test]
fn refund_after_expiration_returns_tokens_and_closes_the_offer() {
    let mut fixture = Fixture::new();
    let seed = 504;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);
    fixture.set_time(EXPIRATION);

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
        "expired refund should return escrow and vault rent, minus the transaction fee"
    );
}

#[test]
fn unauthorized_refund_is_rejected() {
    let mut fixture = Fixture::new();
    let seed = 502;
    let terms = AuctionTerms::reserved(fixture.taker.pubkey());
    fixture.make(seed, DEPOSIT_AMOUNT, terms);

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
