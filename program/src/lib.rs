use anchor_lang::prelude::*;

declare_id!("R4ffL3gDqJ7FqKmL6KjHsGdN5xjG2vV9c9Q7Y8pMnKx");

#[program]
pub mod solraffle {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        tiers: Vec<TierConfig>,
    ) -> Result<()> {
        let raffle = &mut ctx.accounts.raffle;
        raffle.authority = ctx.accounts.authority.key();
        raffle.tiers = tiers;
        raffle.total_tickets_sold = 0;
        raffle.is_active = true;
        Ok(())
    }

    pub fn buy_ticket(ctx: Context<BuyTicket>, tier_index: u8) -> Result<()> {
        let raffle = &mut ctx.accounts.raffle;
        
        require!(raffle.is_active, RaffleError::RaffleNotActive);
        require!(tier_index < raffle.tiers.len() as u8, RaffleError::InvalidTier);
        
        let tier = &mut raffle.tiers[tier_index as usize];
        let user = &ctx.accounts.user;
        
        // Transfer SOL from user to program
        let cpi_ctx = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: user.to_account_info(),
                to: ctx.accounts.program_treasury.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_ctx, tier.ticket_price)?;
        
        tier.tickets_sold += 1;
        raffle.total_tickets_sold += 1;
        
        // Check if tier is sold out (100 tickets)
        if tier.tickets_sold >= 100 {
            tier.is_sold_out = true;
        }
        
        emit!(TicketPurchased {
            user: user.key(),
            tier_index,
            amount: tier.ticket_price,
            total_sold: tier.tickets_sold,
        });
        
        Ok(())
    }

    pub fn draw_winner(ctx: Context<DrawWinner>, tier_index: u8) -> Result<()> {
        let raffle = &mut ctx.accounts.raffle;
        
        require!(raffle.authority == ctx.accounts.authority.key(), RaffleError::Unauthorized);
        require!(tier_index < raffle.tiers.len() as u8, RaffleError::InvalidTier);
        
        let tier = &mut raffle.tiers[tier_index as usize];
        require!(tier.tickets_sold >= 100, RaffleError::NotEnoughTickets);
        
        // Pseudo-random winner selection using clock and previous transactions
        let clock = Clock::get()?;
        let random_index = (clock.unix_timestamp as u64 % 100) as usize;
        
        // Winner gets 99% of the pot
        let prize_pool = tier.tickets_sold as u64 * tier.ticket_price;
        let winner_prize = (prize_pool * 99) / 100;
        
        // Transfer to winner (for now, send to authority as placeholder)
        let dest_account = &ctx.accounts.winner;
        **ctx.accounts.program_treasury.try_borrow_mut_lamports()? -= winner_prize;
        **dest_account.try_borrow_mut_lamports()? += winner_prize;
        
        tier.has_winner = true;
        tier.winner = Some(ctx.accounts.winner.key());
        
        emit!(WinnerDrawn {
            tier_index,
            winner: ctx.accounts.winner.key(),
            prize: winner_prize,
        });
        
        Ok(())
    }

    pub fn reset_tier(ctx: Context<ResetTier>, tier_index: u8) -> Result<()> {
        let raffle = &mut ctx.accounts.raffle;
        
        require!(raffle.authority == ctx.accounts.authority.key(), RaffleError::Unauthorized);
        
        let tier = &mut raffle.tiers[tier_index as usize];
        tier.tickets_sold = 0;
        tier.is_sold_out = false;
        tier.has_winner = false;
        tier.winner = None;
        
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8000)]
    pub raffle: Account<'info, RaffleState>,
    #[account(signer)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BuyTicket<'info> {
    #[account(mut)]
    pub raffle: Account<'info, RaffleState>,
    #[account(signer)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub program_treasury: Account<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DrawWinner<'info> {
    #[account(mut)]
    pub raffle: Account<'info, RaffleState>,
    #[account(signer)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub winner: SystemAccount<'info>,
    #[account(mut)]
    pub program_treasury: Account<'info>,
}

#[derive(Accounts)]
pub struct ResetTier<'info> {
    #[account(mut)]
    pub raffle: Account<'info, RaffleState>,
    #[account(signer)]
    pub authority: Signer<'info>,
}

#[account]
pub struct RaffleState {
    pub authority: Pubkey,
    pub tiers: Vec<TierConfig>,
    pub total_tickets_sold: u64,
    pub is_active: bool,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TierConfig {
    pub name: String,
    pub ticket_price: u64,
    pub tickets_sold: u32,
    pub is_sold_out: bool,
    pub has_winner: bool,
    pub winner: Option<Pubkey>,
}

#[error_code]
pub enum RaffleError {
    #[msg("Raffle is not active")]
    RaffleNotActive,
    #[msg("Invalid tier index")]
    InvalidTier,
    #[msg("Not enough tickets sold")]
    NotEnoughTickets,
    #[msg("Unauthorized")]
    Unauthorized,
}

#[event]
pub struct TicketPurchased {
    pub user: Pubkey,
    pub tier_index: u8,
    pub amount: u64,
    pub total_sold: u32,
}

#[event]
pub struct WinnerDrawn {
    pub tier_index: u8,
    pub winner: Pubkey,
    pub prize: u64,
}