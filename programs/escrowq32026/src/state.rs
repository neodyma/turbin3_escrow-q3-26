use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct AuctionTerms {
    pub receive: u64,
    pub minimum_receive: u64,
    pub preferred_taker: Option<Pubkey>,
    pub starts_at: i64,
    pub exclusive_until: i64,
    pub decay_ends_at: i64,
    pub expiration: i64,
}

#[derive(InitSpace)]
#[account(discriminator = 1)]
pub struct Escrow {
    pub seed: u64,
    pub maker: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    /// Opening token B price.
    pub receive: u64,
    /// Lowest token B price after the Dutch-auction decay.
    pub minimum_receive: u64,
    /// Optional taker with exclusive access at the start of the auction.
    pub preferred_taker: Option<Pubkey>,
    pub starts_at: i64,
    pub exclusive_until: i64,
    pub decay_ends_at: i64,
    pub bump: u8,
    pub expiration: i64,
}
