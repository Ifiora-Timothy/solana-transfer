use anchor_lang::{prelude::*,Accounts};

declare_id!("AUN1nL8ad53Eqc2ccp1gnK8VBN7SL9TazYLoS6vVsvp9");

#[program]
pub mod sol_central_treasury {
    use anchor_lang::{solana_program, system_program};

    use super::*;

    pub fn initialize_central_wallet(
        ctx: Context<InitializeCentralWallet>,
        treasury_bump: u8,
    ) -> Result<()> {
        let treasury = &mut ctx.accounts.treasury;
        treasury.owner = ctx.accounts.owner.key();
        treasury.central_wallet = ctx.accounts.central_wallet.key();
        treasury.bump = treasury_bump;
        treasury.initialized = true;

        // Ensure central wallet is rent-exempt
        let rent = Rent::get()?;
        let central_wallet_info = ctx.accounts.central_wallet.to_account_info();
        if !rent.is_exempt(central_wallet_info.lamports(), central_wallet_info.data_len()) {
            return Err(ProgramError::AccountNotRentExempt.into());
        }

        Ok(())
    }

    // Deposit SOL function
    pub fn deposit(ctx: Context<DepositSol>, amount: u64) -> Result<()> {
        // Transfer SOL from depositor to central wallet
        let ix = solana_program::system_instruction::transfer(
            &ctx.accounts.depositor.key(), 
            &ctx.accounts.central_wallet.key(), 
            amount
        );
        // Invoke the transfer
        solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.depositor.to_account_info(),
                ctx.accounts.central_wallet.to_account_info(),
                ctx.accounts.system_program.to_account_info()
            ]
        )?;

        Ok(())
    }

    // Admin function to withdraw SOL from central wallet
    pub fn admin_withdraw(ctx: Context<AdminWithdraw>, amount: u64) -> Result<()> {
// Get the central wallet PDA signer seeds


          // Transfer SOL from central wallet to recipient
    let cpi_accounts = system_program::Transfer {
        from: ctx.accounts.central_wallet.to_account_info(),
        to: ctx.accounts.recipient.to_account_info(),
    };
   let central_wallet_bump = ctx.bumps.central_wallet;
    
      // Create CPI context with seeds for PDA signing
      let owner_key = ctx.accounts.owner.key();
      let authority_seeds = &[
          b"central_wallet",
          owner_key.as_ref(),
          &[central_wallet_bump]
      ];
      let signer_seeds = &[&authority_seeds[..]];
      let cpi_context = CpiContext::new_with_signer(
        ctx.accounts.system_program.to_account_info(),
        cpi_accounts,
        signer_seeds
    );
    

       // Perform the transfer
       system_program::transfer(cpi_context, amount)?;

 



        Ok(())
    }
}

// Accounts for depositing SOL
#[derive(Accounts)]
pub struct DepositSol<'info> {
    /// The user making the deposit
    #[account(mut)]
    pub depositor: Signer<'info>,

    /// Central wallet that receives deposits
    #[account(mut)]
    pub central_wallet: SystemAccount<'info>,

    /// System program
    pub system_program: Program<'info, System>,
}

// Accounts for admin withdrawal
#[derive(Accounts)]
pub struct AdminWithdraw<'info> {
    #[account(
        mut,
        seeds = [b"treasury", owner.key().as_ref()],
        bump,
        has_one = owner,
        constraint = treasury.initialized
    )]
    pub treasury: Account<'info, TreasuryAccount>,
    
    #[account(mut)]
    pub owner: Signer<'info>,
    
     /// CHECK: This is a PDA that will be used as a wallet
     #[account(
        mut,
        seeds = [b"central_wallet", owner.key().as_ref()],
        bump
    )]
    pub central_wallet: SystemAccount<'info>,
    
    #[account(mut)]
    pub recipient: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

// Accounts for initializing central wallet
#[derive(Accounts)]
pub struct InitializeCentralWallet<'info> {
    #[account(
        init,
        seeds = [b"treasury", owner.key().as_ref()],
        bump,
        payer = owner,
        space = 8 + 32 + 32 + 1 + 1, // Discriminator + owner + central_wallet + bump + initialized
        constraint = !treasury.initialized
    )]
    pub treasury: Account<'info, TreasuryAccount>,

    #[account(mut)]
    pub owner: Signer<'info>,

      /// CHECK: This is a PDA that will be used as a wallet, we just need to derive it
      #[account(
        seeds = [b"central_wallet", owner.key().as_ref()],
        bump
    )]
    pub central_wallet: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

// Treasury account structure
#[account]
pub struct TreasuryAccount {
    pub owner: Pubkey,          // Owner of the treasury
    pub central_wallet: Pubkey, // Central collection wallet
    pub bump: u8,               // PDA bump seed
    pub initialized: bool,      // Initialization flag
}

// Custom error types
#[error_code]
pub enum TreasuryError {
    #[msg("Unauthorized withdrawal attempt")]
    UnauthorizedWithdrawal,
    #[msg("Treasury has already been initialized")]
    TreasuryAlreadyInitialized,
}