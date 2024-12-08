use anchor_client::{
    anchor_lang::{solana_program::pubkey::Pubkey, system_program},
    solana_sdk::{
        commitment_config::CommitmentConfig,
        signature::{read_keypair_file, Keypair, Signer},
        system_instruction,
        transaction::Transaction,
    },
    Client, Cluster, Program,
};
use solana_program_test::tokio;
use std::fs::File;
use std::io::Write;
use std::str::FromStr;

// Import your program's defined accounts and instructions
use sol_transfer::TreasuryAccount;

const PROGRAM_ID: &str = "AUN1nL8ad53Eqc2ccp1gnK8VBN7SL9TazYLoS6vVsvp9";

// Helper function to setup program test environment
async fn setup_program_test() -> (
    Keypair, // Payer
    Keypair, // Owner
    u8,      // Treasury bump
    Pubkey,  // Treasury PDA
    Pubkey,  // Program ID
    Keypair, // Depositor
    Pubkey,  // Central Wallet PDA
) {
    let program_id: Pubkey = Pubkey::from_str(PROGRAM_ID).unwrap();

    let anchor_wallet =
        std::env::var("ANCHOR_WALLET").expect("ANCHOR_WALLET environment variable is not set");
    let payer =
        read_keypair_file(&anchor_wallet).expect("Failed to read keypair from ANCHOR_WALLET");
    let owner = Keypair::new();

    // Derive treasury PDA
    let (treasury_pda, treasury_bump) =
        Pubkey::find_program_address(&[b"treasury", owner.pubkey().as_ref()], &program_id);

    // Derive central wallet PDA
    let (central_wallet, _) =
        Pubkey::find_program_address(&[b"central_wallet", owner.pubkey().as_ref()], &program_id);

    // Create a depositor
    let depositor = Keypair::new();

    (
        payer,
        owner,
        treasury_bump,
        treasury_pda,
        program_id,
        depositor,
        central_wallet,
    )
}

// Helper function to add SOL to an account
async fn add_funds(
    sender: &Keypair,
    recipient: &Pubkey,
    amount_in_sol: u64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let lamports = amount_in_sol * 1_000_000_000; // Convert SOL to lamports

    let program_id: Pubkey = Pubkey::from_str(PROGRAM_ID).unwrap();

    let transfer_instruction = system_instruction::transfer(&sender.pubkey(), recipient, lamports);

    let client = Client::new_with_options(Cluster::Localnet, sender, CommitmentConfig::confirmed());
    let signature = client
        .program(program_id)
        .unwrap()
        .async_rpc()
        .send_and_confirm_transaction(&Transaction::new_signed_with_payer(
            &[transfer_instruction],
            Some(&sender.pubkey()),
            &[sender],
            client
                .program(program_id)
                .unwrap()
                .async_rpc()
                .get_latest_blockhash()
                .await
                .expect("Failed to get recent blockhash"),
        ))
        .await
        .expect("Failed to send transaction");

    println!("Funds transferred with signature: {}", signature);
    Ok(())
}

// Initialize central wallet
async fn initialize_treasury<'a>(
    program: &Program<&'a Keypair>,
    treasury_bump: u8,
    owner: &Keypair,
    central_wallet: Pubkey,
    treasury_pda: &Pubkey,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Add funds to central wallet to make it rent-exempt
    add_funds(&owner, &central_wallet, 1).await.unwrap();

    let tx = &program
        .request()
        .accounts(sol_transfer::accounts::InitializeCentralWallet {
            owner: owner.pubkey(),
            treasury: *treasury_pda,
            central_wallet,
            system_program: system_program::ID,
        })
        .args(sol_transfer::instruction::InitializeCentralWallet { treasury_bump })
        .signer(&owner)
        .send()
        .await;

    assert!(tx.is_ok(), "Central Wallet initialization failed");

    // Verify treasury account
    let treasury_data: TreasuryAccount = program
        .account(*treasury_pda)
        .await
        .expect("Failed to fetch treasury account");

    assert!(treasury_data.initialized, "Treasury should be initialized");

    Ok(())
}

// Deposit SOL to central wallet
async fn deposit_sol<'a>(
    program: &Program<&'a Keypair>,
    depositor: &Keypair,
    central_wallet: Pubkey,
    deposit_amount: u64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Get initial balance of central wallet
    let deposit_amount_in_sol = deposit_amount * 1_000_000_000;

    //check depositor balance
    let depositor_balance = program
        .async_rpc()
        .get_balance(&depositor.pubkey())
        .await
        .expect("Failed to get initial balance");
    //check it is greater than deposit amount
    //save the balance to a file
    let mut file = File::create("transaction_mxint.txt").expect("Failed to create file");
    file.write_all(format!("Depositor balance: {:?}", depositor_balance).as_bytes())
        .expect("Failed to write to file");

    assert!(
        depositor_balance > deposit_amount_in_sol,
        "Depositor balance should be greater than deposit amount"
    );

    let initial_balance = program
        .async_rpc()
        .get_balance(&central_wallet)
        .await
        .expect("Failed to get initial balance");

    program
        .request()
        .accounts(sol_transfer::accounts::DepositSol {
            depositor: depositor.pubkey(),
            central_wallet,
            system_program: system_program::ID,
        })
        .args(sol_transfer::instruction::Deposit {
            amount: deposit_amount_in_sol,
        })
        .signer(&depositor)
        .send()
        .await
        .expect("Failed to deposit SOL");

    // Get final balance of central wallet
    let final_balance = program
        .async_rpc()
        .get_balance(&central_wallet)
        .await
        .expect("Failed to get final balance");

    assert_eq!(
        final_balance,
        initial_balance + deposit_amount_in_sol,
        "Central wallet balance should increase by deposit amount"
    );

    Ok(())
}

// Withdraw SOL from central wallet
async fn withdraw_sol<'a>(
    program: &Program<&'a Keypair>,
    owner: &Keypair,
    central_wallet: Pubkey,
    treasury_pda: Pubkey,
    recipient: Pubkey,
    withdrawal_amount: u64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Get initial balance of recipient
    let withdrawal_amount = withdrawal_amount * 1_000_000_000;
    let initial_recipient_balance = match program.async_rpc().get_balance(&recipient).await {
        Ok(balance) => balance,
        Err(e) => {
            println!("Error getting recipient balance: {:?}", e);
            return Err(Box::new(e));
        }
    };

    let result = program
        .request()
        .accounts(sol_transfer::accounts::AdminWithdraw {
            treasury: treasury_pda,
            owner: owner.pubkey(),
            central_wallet,
            recipient,
            system_program: system_program::ID,
        })
        .args(sol_transfer::instruction::AdminWithdraw {
            amount: withdrawal_amount,
        })
        .signer(&owner)
        .send()
        .await;

    let mut file = File::create("transaction_wnt.txt").expect("Failed to create file");

    //   // Write the entire result to the file
    writeln!(file, "deopsit  Result: {:?}", result).expect("Failed to write to file");

    assert!(result.is_ok(), "Withdrawal failed");

    // Get final balance of recipient
    let final_recipient_balance = program
        .async_rpc()
        .get_balance(&recipient)
        .await
        .expect("Failed to get final recipient balance");

    assert_eq!(
        final_recipient_balance,
        initial_recipient_balance + withdrawal_amount,
        "Recipient balance should increase by withdrawal amount"
    );

    Ok(())
}

#[tokio::test]
async fn test_central_wallet_initialization() {
    let (payer, owner, treasury_bump, treasury_pda, program_id, _, _) = setup_program_test().await;

    // Add funds to owner
    add_funds(&payer, &owner.pubkey(), 1000).await.unwrap();

    let client = Client::new_with_options(Cluster::Localnet, &owner, CommitmentConfig::confirmed());
    let program = client.program(program_id).unwrap();
    // When setting up the test
    let (central_wallet, _central_wallet_bump) =
        Pubkey::find_program_address(&[b"central_wallet", owner.pubkey().as_ref()], &program_id);
    // Initialize treasury
    initialize_treasury(
        &program,
        treasury_bump,
        &owner,
        central_wallet,
        &treasury_pda,
    )
    .await
    .expect("Failed to initialize treasury");
}

#[tokio::test]
async fn test_valid_sol_deposit() {
    let (payer, owner, treasury_bump, treasury_pda, program_id, depositor, central_wallet) =
        setup_program_test().await;

    // Add funds to owner and depositor
    add_funds(&payer, &owner.pubkey(), 1000).await.unwrap();
    add_funds(&payer, &depositor.pubkey(), 100).await.unwrap();

    let client = Client::new_with_options(Cluster::Localnet, &owner, CommitmentConfig::confirmed());
    let program = client.program(program_id).unwrap();

    // Initialize treasury
    initialize_treasury(
        &program,
        treasury_bump,
        &owner,
        central_wallet,
        &treasury_pda,
    )
    .await
    .expect("Failed to initialize treasury");

    // Deposit SOL
    deposit_sol(&program, &depositor, central_wallet, 50)
        .await
        .expect("Could not deposit SOL");
}

#[tokio::test]
async fn test_valid_admin_withdrawal() {
    let (payer, owner, treasury_bump, treasury_pda, program_id, depositor, central_wallet) =
        setup_program_test().await;

    // Add funds to owner and depositor
    add_funds(&payer, &owner.pubkey(), 2).await.unwrap();
    add_funds(&payer, &depositor.pubkey(), 100).await.unwrap();

    let client = Client::new_with_options(Cluster::Localnet, &owner, CommitmentConfig::confirmed());
    let program = client.program(program_id).unwrap();

    // Initialize treasury
    initialize_treasury(
        &program,
        treasury_bump,
        &owner,
        central_wallet,
        &treasury_pda,
    )
    .await
    .expect("Failed to initialize treasury");

    // Deposit SOL
    deposit_sol(&program, &depositor, central_wallet, 50)
        .await
        .expect("Could not deposit SOL");

    // Create a recipient for withdrawal
    let recipient = Keypair::new();

    // Perform admin withdrawal
    withdraw_sol(
        &program,
        &owner,
        central_wallet,
        treasury_pda,
        recipient.pubkey(),
        25,
    )
    .await
    .expect("Could not perform admin withdrawal");

    //check recipient balance
    let recipient_balance = program
        .async_rpc()
        .get_balance(&recipient.pubkey())
        .await
        .expect("Failed to get recipient balance");

    let mut file = File::create("transaction_w.txt").expect("Failed to create file");

    //   // Write the entire result to the file
    writeln!(file, "balance: {:?}", recipient_balance).expect("Failed to write to file");
}

#[tokio::test]
async fn test_invalid_admin_withdrawal() {
    let (payer, owner, treasury_bump, treasury_pda, program_id, depositor, central_wallet) =
        setup_program_test().await;

    // Add funds to owner and depositor
    add_funds(&payer, &owner.pubkey(), 2).await.unwrap();
    add_funds(&payer, &depositor.pubkey(), 100).await.unwrap();

    let not_owner = Keypair::new();
    add_funds(&payer, &not_owner.pubkey(), 100).await.unwrap();

    let client = Client::new_with_options(Cluster::Localnet, &owner, CommitmentConfig::confirmed());
    let program = client.program(program_id).unwrap();

    // Initialize treasury
    initialize_treasury(
        &program,
        treasury_bump,
        &owner,
        central_wallet,
        &treasury_pda,
    )
    .await
    .expect("Failed to initialize treasury");

    // Deposit SOL
    deposit_sol(&program, &depositor, central_wallet, 50)
        .await
        .expect("Could not deposit SOL");

    //check central

    // Create a recipient for withdrawal
    let recipient = Keypair::new();
    // add_funds(&payer, &recipient.pubkey(), 100).await.unwrap();

    // Perform admin withdrawal
    let invalid_withdraw = program
        .request()
        .accounts(sol_transfer::accounts::AdminWithdraw {
            treasury: treasury_pda,
            owner: not_owner.pubkey(),
            central_wallet,
            recipient: recipient.pubkey(),
            system_program: system_program::ID,
        })
        .args(sol_transfer::instruction::AdminWithdraw { amount: 25 })
        .signer(&not_owner)
        .send()
        .await;

    //should fail
    assert!(invalid_withdraw.is_err(), "Withdrawal should fail");
}
