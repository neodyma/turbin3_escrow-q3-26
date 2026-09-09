use anchor_lang::prelude::*;

use crate::{error::ErrorCode, state::Escrow, ESCROW_SEED};

#[derive(Accounts)]
pub struct Update<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    #[account(
        mut,
        has_one = maker,
        seeds = [ESCROW_SEED, maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,
}

impl<'info> Update<'info> {
    pub fn update(&mut self, receive: u64, starts_at: i64) -> Result<()> {
        require!(receive > 0, ErrorCode::InvalidReceiveAmount);
        require!(
            receive >= self.escrow.minimum_receive,
            ErrorCode::InvalidPriceRange
        );

        let now = Clock::get()?.unix_timestamp;
        require!(
            now < self.escrow.starts_at,
            ErrorCode::AuctionAlreadyStarted
        );
        require!(starts_at > now, ErrorCode::InvalidAuctionSchedule);

        let delta = i128::from(starts_at) - i128::from(self.escrow.starts_at);
        let exclusive_until = Self::shift_timestamp(self.escrow.exclusive_until, delta)?;
        let decay_ends_at = Self::shift_timestamp(self.escrow.decay_ends_at, delta)?;
        let expiration = Self::shift_timestamp(self.escrow.expiration, delta)?;

        self.escrow.receive = receive;
        self.escrow.starts_at = starts_at;
        self.escrow.exclusive_until = exclusive_until;
        self.escrow.decay_ends_at = decay_ends_at;
        self.escrow.expiration = expiration;

        Ok(())
    }

    fn shift_timestamp(timestamp: i64, delta: i128) -> Result<i64> {
        i64::try_from(i128::from(timestamp) + delta)
            .map_err(|_| error!(ErrorCode::InvalidAuctionSchedule))
    }
}
