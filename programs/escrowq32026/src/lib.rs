pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("pvFoPx94YaCrYxgaa8FbyCqmNS6UJeqzp7tWWKTYEPK");

// Two parties — a maker and a taker — can swap tokens without trusting each other or a third party.
// The maker deposits token A into a program-controlled vault and specifies how much of token B they want in return.
// A preferred taker gets an exclusive acceptance window. The offer then becomes public and its
// token B price decays to a maker-selected floor. The maker can cancel before the auction starts
// or reclaim the tokens after it expires.

// Maker deposits token A  →  vault (PDA-owned)
//                                       ↓  taker sends token B to maker
//                                       ↓  vault releases token A to taker
//                                       ↓  escrow + vault accounts closed, rent returned

#[program]
pub mod escrowq32026 {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn make(ctx: Context<Make>, seed: u64, deposit: u64, terms: AuctionTerms) -> Result<()> {
        ctx.accounts.validate(deposit, &terms)?;
        ctx.accounts.init_escrow(seed, terms, &ctx.bumps)?;
        ctx.accounts.deposit(deposit)
    }

    #[instruction(discriminator = 3)]
    pub fn take(ctx: Context<Take>) -> Result<()> {
        ctx.accounts.take()
    }

    #[instruction(discriminator = 2)]
    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        ctx.accounts.refund_and_close_vault()
    }

    #[instruction(discriminator = 4)]
    pub fn update(ctx: Context<Update>, receive: u64, starts_at: i64) -> Result<()> {
        ctx.accounts.update(receive, starts_at)
    }
}
