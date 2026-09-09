use anchor_lang::prelude::*;

use crate::{error::ErrorCode, AuctionTerms, Escrow, ESCROW_SEED};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Make<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    #[account(
        mint::token_program = token_program
    )]
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(
        mint::token_program = token_program
    )]
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init,
        payer = maker,
        seeds = [ESCROW_SEED, maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        space = Escrow::DISCRIMINATOR.len() + Escrow::INIT_SPACE,
        bump
    )]
    pub escrow: Account<'info, Escrow>,
    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}
impl<'info> Make<'info> {
    pub fn validate(&self, deposit: u64, terms: &AuctionTerms) -> Result<()> {
        require!(deposit > 0, ErrorCode::InvalidDepositAmount);
        require!(terms.receive > 0, ErrorCode::InvalidReceiveAmount);
        require!(
            terms.minimum_receive > 0,
            ErrorCode::InvalidMinimumReceiveAmount
        );
        require!(
            terms.receive >= terms.minimum_receive,
            ErrorCode::InvalidPriceRange
        );
        require!(
            self.mint_a.key() != self.mint_b.key(),
            ErrorCode::IdenticalMints
        );

        let now = Clock::get()?.unix_timestamp;
        let valid_timestamps = terms.starts_at > now
            && terms.starts_at <= terms.exclusive_until
            && terms.exclusive_until < terms.decay_ends_at
            && terms.decay_ends_at < terms.expiration;
        let valid_exclusive_window = match terms.preferred_taker {
            Some(_) => terms.starts_at < terms.exclusive_until,
            None => terms.starts_at == terms.exclusive_until,
        };

        require!(
            valid_timestamps && valid_exclusive_window,
            ErrorCode::InvalidAuctionSchedule
        );

        Ok(())
    }

    //Initialize escrow
    pub fn init_escrow(&mut self, seed: u64, terms: AuctionTerms, bumps: &MakeBumps) -> Result<()> {
        self.escrow.set_inner(Escrow {
            seed,
            maker: self.maker.key(),
            mint_a: self.mint_a.key(),
            mint_b: self.mint_b.key(),
            receive: terms.receive,
            minimum_receive: terms.minimum_receive,
            preferred_taker: terms.preferred_taker,
            starts_at: terms.starts_at,
            exclusive_until: terms.exclusive_until,
            decay_ends_at: terms.decay_ends_at,
            bump: bumps.escrow,
            expiration: terms.expiration,
        });
        Ok(())
    }

    //Deposit tokens from maker to vault
    pub fn deposit(&mut self, deposit: u64) -> Result<()> {
        let transfer_accounts = TransferChecked {
            from: self.maker_ata_a.to_account_info(),
            mint: self.mint_a.to_account_info(),
            to: self.vault.to_account_info(),
            authority: self.maker.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(self.token_program.key(), transfer_accounts);

        transfer_checked(cpi_ctx, deposit, self.mint_a.decimals)
    }
}
