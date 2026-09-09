use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The escrow has expired")]
    EscrowExpired,
    #[msg("The deposit amount must be greater than zero")]
    InvalidDepositAmount,
    #[msg("The requested receive amount must be greater than zero")]
    InvalidReceiveAmount,
    #[msg("The minimum receive amount must be greater than zero")]
    InvalidMinimumReceiveAmount,
    #[msg("The opening receive amount must be at least the minimum receive amount")]
    InvalidPriceRange,
    #[msg("The auction timestamps or preferred-taker window are invalid")]
    InvalidAuctionSchedule,
    #[msg("The deposit and receive mints must be different")]
    IdenticalMints,
    #[msg("The escrow vault is empty")]
    EmptyVault,
    #[msg("The auction has not started")]
    AuctionNotStarted,
    #[msg("Only the preferred taker may accept during the exclusive window")]
    PreferredTakerOnly,
    #[msg("The auction has already started")]
    AuctionAlreadyStarted,
    #[msg("The maker cannot refund while the auction is live")]
    AuctionLive,
}
