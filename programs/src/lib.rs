use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hash;
use anchor_lang::solana_program::sysvar::slot_hashes::SolanaSlotHashes;

declare_id!("R4ffL3gDqJ7FqKmL6KjHsGdN5xjG2vV9c9Q7Y8pMnKx");

#[program]
pub mod sol_raffle {
    use super::*;

    pub fn init_raffle(ctx: Context<InitRaffle>, tier: u8, max_tickets: u32) -> Result<()> {
        require!(tier <= 5, RaffleError::InvalidTier);
        
        let raffle = &mut ctx.accounts.raffle;
        raffle.authority = ctx.accounts.authority.key();
        raffle.tier = tier;
        raffle.max_tickets = max_tickets;
        raffle.tickets_sold = 0;
        raffle.collected_amount = 0;
        raffle.house_fee = 100; // 1% in basis points
        raffle.is_drawn = false;
        raffle.winner = None;
        raffle.bump = ctx.bumps.raffle;
        
        Ok(())
    }

    pub fn buy_ticket(ctx: Context<BuyTicket>, _tier: u8) -> Result<()> {
        let raffle = &mut ctx.accounts.raffle;
        
        require!(!raffle.is_drawn, RaffleError::RaffleAlreadyDrawn);
        require!(raffle.tickets_sold < raffle.max_tickets, RaffleError::NoTicketsLeft);
        
        let ticket_price = get_tier_price(raffle.tier)?;
        let buyer = ctx.accounts.buyer.key();
        
        // Check if buyer already has a ticket in this raffle
        for ticket in &ctx.accounts.tickets.to_account_info().lamports().to_le_bytes() {
            require!(buyer != Pubkey::new_from_array(ticket.to_le_bytes()), RaffleError::AlreadyBought);
        }
        
        // Transfer SOL from buyer to raffle vault
        let ix = anchor_lang::solana_program::system_instruction::transfer(
            &buyer,
            &ctx.accounts.vault.key(),
            ticket_price,
        );
        anchor_lang::solana_program::program::invoke(&ix, &[
            ctx.accounts.buyer.to_account_info(),
            ctx.accounts.vault.to_account_info(),
        ])?;
        
        // Record ticket
        let ticket = &mut ctx.accounts.ticket;
        ticket.buyer = buyer;
        ticket.raffle = raffle.key();
        ticket.tier = raffle.tier;
        ticket.timestamp = Clock::get()?.unix_timestamp;
        ticket.bump = ctx.bumps.ticket;
        
        raffle.tickets_sold += 1;
        raffle.collected_amount += ticket_price;
        
        // Auto-draw if max tickets reached
        if raffle.tickets_sold >= raffle.max_tickets {
            draw_winner(ctx, raffle)?;
        }
        
        Ok(())
    }

    pub fn draw(ctx: Context<Draw>) -> Result<()> {
        let raffle = &mut ctx.accounts.raffle;
        
        require!(!raffle.is_drawn, RaffleError::AlreadyDrawn);
        require!(raffle.tickets_sold > 0, RaffleError::NoTicketsSold);
        
        draw_winner(ctx, raffle)?;
        
        Ok(())
    }

    pub fn claimPrize(ctx: Context<ClaimPrize>) -> Result<()> {
        let raffle = &ctx.accounts.raffle;
        
        require!(raffle.is_drawn, RaffleError::NotDrawnYet);
        require!(raffle.winner == Some(ctx.accounts.claimant.key()), RaffleError::NotWinner);
        
        let prize = calculate_prize(raffle.collected_amount, raffle.house_fee);
        
        // Transfer prize to winner
        **ctx.accounts.vault.to_account_info().try_borrow_mut_lamports()? -= prize;
        **ctx.accounts.claimant.to_account_info().try_borrow_mut_lamports()? += prize;
        
        Ok(())
    }

    pub fn claimHouseFee(ctx: Context<ClaimHouseFee>) -> Result<()> {
        let raffle = &ctx.accounts.raffle;
        
        require!(raffle.is_drawn, RaffleError::NotDrawnYet);
        require!(raffle.authority == ctx.accounts.authority.key(), RaffleError::NotAuthority);
        
        let fee = calculate_house_fee(raffle.collected_amount, raffle.house_fee);
        
        **ctx.accounts.vault.to_account_info().try_borrow_mut_lamports()? -= fee;
        **ctx.accounts.authority.to_account_info().try_borrow_mut_lamports()? += fee;
        
        Ok(())
    }
}

fn draw_winner(ctx: Context<Draw>, raffle: &mut Account<Raffle>) -> Result<()> {
    // Get recent blockhash for randomness
    let slot_hashes = SolanaSlotHashes::from_account_info(&ctx.accounts.slot_hashes)?;
    let recent_hash = slot_hashes.slot_hashes[0].1;
    
    // Hash the recent blockhash with the raffle public key for randomness
    let mut hasher = hash::Hash::new(&[]);
    hasher.hash(&recent_hash.to_bytes());
    hasher.hash(&raffle.key().to_bytes());
    
    let random_bytes = hasher.to_bytes();
    let winner_index = (random_bytes[0] as usize + (random_bytes[1] as usize) * 256) % raffle.tickets_sold as usize;
    
    // In production, would look up ticket at winner_index
    // For now, mark as drawn
    raffle.is_drawn = true;
    raffle.winner = Some(ctx.accounts.tickets.to_account_info().key()); // Placeholder
    
    Ok(())
}

fn get_tier_price(tier: u8) -> Result<u64> {
    match tier {
        0 => Ok(10_000_000),      // 0.01 SOL
        1 => Ok(50_000_000),      // 0.05 SOL
        2 => Ok(100_000_000),     // 0.1 SOL
        3 => Ok(200_000_000),     // 0.2 SOL
        4 => Ok(500_000_000),     // 0.5 SOL
        5 => Ok(1_000_000_000),   // 1 SOL
        _ => Err(RaffleError::InvalidTier.into()),
    }
}

fn calculate_prize(collected: u64, house_fee_bps: u32) -> u64 {
    let fee = (collected * house_fee_bps as u64) / 10000;
    collected - fee
}

fn calculate_house_fee(collected: u64, house_fee_bps: u32) -> u64 {
    (collected * house_fee_bps as u64) / 10000
}

#[derive(Accounts)]
pub struct InitRaffle<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = Raffle::SIZE + 8,
        seeds = [b"raffle", authority.key().as_ref(), &[tier]],
        bump
    )]
    pub raffle: Account<'info, Raffle>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BuyTicket<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,
    #[account(mut)]
    pub raffle: Account<'info, Raffle>,
    #[account(mut)]
    pub vault: SystemAccount<'info>,
    #[account(init, payer = buyer, space = Ticket::SIZE + 8)]
    pub ticket: Account<'info, Ticket>,
    pub system_program: Program<'info, System>,
    #[account(address = anchor_lang::solana_program::sysvar::slot_hashes::id())]
    pub slot_hashes: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct Draw<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub raffle: Account<'info, Raffle>,
    pub tickets: AccountInfo<'info>,
    #[account(address = anchor_lang::solana_program::sysvar::slot_hashes::id())]
    pub slot_hashes: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct ClaimPrize<'info> {
    #[account(mut)]
    pub claimant: Signer<'info>,
    #[account(mut)]
    pub raffle: Account<'info, Raffle>,
    #[account(mut)]
    pub vault: SystemAccount<'info>,
}

#[derive(Accounts)]
pub struct ClaimHouseFee<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub raffle: Account<'info, Raffle>,
    #[account(mut)]
    pub vault: SystemAccount<'info>,
}

#[account]
pub struct Raffle {
    pub authority: Pubkey,
    pub tier: u8,
    pub max_tickets: u32,
    pub tickets_sold: u32,
    pub collected_amount: u64,
    pub house_fee: u32,
    pub is_drawn: bool,
    pub winner: Option<Pubkey>,
    pub bump: u8,
}

impl Raffle {
    pub const SIZE: usize = 32 + 1 + 4 + 4 + 8 + 4 + 1 + 33 + 1;
}

#[account]
pub struct Ticket {
    pub buyer: Pubkey,
    pub raffle: Pubkey,
    pub tier: u8,
    pub timestamp: i64,
    pub bump: u8,
}

impl Ticket {
    pub const SIZE: usize = 32 + 32 + 1 + 8 + 1;
}

#[error_code]
pub enum RaffleError {
    #[msg("Invalid tier")]
    InvalidTier,
    #[msg("Raffle already drawn")]
    RaffleAlreadyDrawn,
    #[msg("No tickets left")]
    NoTicketsLeft,
    #[msg("Already bought a ticket")]
    AlreadyBought,
    #[msg("Already drawn")]
    AlreadyDrawn,
    #[msg("No tickets sold")]
    NoTicketsSold,
    #[msg("Not drawn yet")]
    NotDrawnYet,
    #[msg("Not the winner")]
    NotWinner,
    #[msg("Not the authority")]
    NotAuthority,
}