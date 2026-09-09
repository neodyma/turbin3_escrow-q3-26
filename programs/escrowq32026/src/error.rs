use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The escrow has expired")]
    EscrowExpired,
    #[msg("The deposit amount must be greater than zero")]
    InvalidDepositAmount,
    #[msg("The requested receive amount must be greater than zero")]
    InvalidReceiveAmount,
    #[msg("The deposit and receive mints must be different")]
    IdenticalMints,
    #[msg("The escrow vault is empty")]
    EmptyVault,
}
