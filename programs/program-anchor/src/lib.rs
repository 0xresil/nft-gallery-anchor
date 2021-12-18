use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Mint, SetAuthority, TokenAccount, Transfer};
use spl_token::{ instruction::AuthorityType};
use solana_program::{
    program::{invoke},
    system_instruction,
    program_error::ProgramError,
    msg
};
use spl_token_metadata::{
    instruction::{ update_metadata_accounts },
    state::{Metadata},
};

use borsh::{BorshDeserialize, BorshSerialize};
declare_id!("Dw96F8NjN84googpni4mtSnCuAud9XkaPUFM1RJX53cK");

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct NFTRecord{
    pub hero_id: u8,
    pub content_uri: String,
    pub key_nft: Pubkey,
    pub last_price: u64,
    pub listed_price: u64
}
pub const NFT_COUNT: usize = 12;
pub const NFT_RECORD_SIZE: usize = 250; // 133
pub const REPO_ACCOUNT_SEED: &str = "hallofheros";

#[program]
pub mod hall_of_hero {
    use super::*;

    pub fn add_record(
        ctx: Context<AddRecord>,
        hero_id: u8,
        content_uri: String,
        price: u64,
    ) -> ProgramResult {
        let new_record = NFTRecord {
            hero_id,
            content_uri,
            key_nft: *ctx.accounts.nft_mint.key,
            last_price: price,
            listed_price: price
        };
        save_nft_data_to_repository(&new_record, ctx.accounts.repository.clone())?;
        Ok(())
    }

    pub fn update_record(
        ctx: Context<UpdateRecord>,
        hero_id: u8,
        content_uri: String,
        new_price: u64,
    ) -> ProgramResult {

        // get nft listed price from repository account
        let mut nft_record = get_nft_data_from_repository(
            hero_id, 
            &ctx.accounts.nft_mint.key, 
            ctx.accounts.repository.clone(), 
            ctx.accounts.nft_mint.clone()
        ).unwrap();

        // update nft last price with listed_price
        nft_record.listed_price = new_price;
        nft_record.content_uri = content_uri.to_string();
        save_nft_data_to_repository(&nft_record, ctx.accounts.repository.clone())?;

        Ok(())
    }

    pub fn buy_record(
        ctx: Context<BuyRecord>,
        hero_id: u8,
        dead_uri: String,
        dead_name: String
    ) -> ProgramResult {

        // verify initializer authority. initializer should be admin
        verify_admin_authority(
            ctx.accounts.initializer.key,
            ctx.accounts.repository.key,
            ctx.program_id
        )?;

        token::transfer(
            ctx.accounts.into_transfer_nft_context(),
            1
        );

        // 4. update metadata of dead nft
        update_metadata_old_nft(
            &ctx,
            dead_uri,
            dead_name
        )?;
        
        // get nft listed price from repository account
        let mut nft_record = get_nft_data_from_repository(
            hero_id, 
            ctx.accounts.dead_nft_mint.key,
            ctx.accounts.repository.clone(),
            ctx.accounts.dead_nft_mint.clone()
        ).unwrap();

        // 5. update nft last price with listed_price
        nft_record.last_price = nft_record.listed_price;
        // update nft key
        nft_record.key_nft = *ctx.accounts.new_nft_mint.key;
        save_nft_data_to_repository(&nft_record, ctx.accounts.repository.clone())?;

        // 6. transfer sol from buyer to prev_owner
        sol_transfer(
            ctx.accounts.buyer.clone(), 
            ctx.accounts.prev_owner.clone(), 
            ctx.accounts.system_program.clone(),
            nft_record.listed_price
        )?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct AddRecord<'info> {
    #[account(signer)]
    pub initializer: AccountInfo<'info>,
    #[account(mut, owner = program_id)]
    pub repository: AccountInfo<'info>,
    pub nft_mint: AccountInfo<'info>
}

#[derive(Accounts)]
pub struct UpdateRecord<'info> {
    #[account(signer)]
    pub updater: AccountInfo<'info>,
    #[account(mut, owner = program_id)]
    pub repository: AccountInfo<'info>,
    pub nft_mint: AccountInfo<'info>,
    #[account(
        constraint = associated_token_account.owner == *updater.key,
        constraint = associated_token_account.mint == *nft_mint.key
    )]
    pub associated_token_account: Account<'info, TokenAccount>,
}

#[derive(Accounts)]
pub struct BuyRecord<'info> {
    #[account(mut, signer)]
    pub initializer: AccountInfo<'info>,
    #[account(mut, signer)]
    pub buyer: AccountInfo<'info>,
    #[account(mut)]
    pub prev_owner: AccountInfo<'info>,
    #[account(mut, owner = program_id)]
    pub repository: AccountInfo<'info>,
    #[account(mut)]
    pub dead_nft_mint: AccountInfo<'info>,
    #[account(
        mut,
        constraint = dead_nft_token_account.owner == *prev_owner.key,
        constraint = dead_nft_token_account.mint == *dead_nft_mint.key
    )]
    pub dead_nft_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        owner = *token_metadata_program.key
    )]
    pub dead_nft_metadata_account: AccountInfo<'info>,
    #[account(mut)]
    pub new_nft_mint: AccountInfo<'info>,
    #[account(mut)]
    pub nft_token_account_to_send: AccountInfo<'info>,
    #[account(mut)]
    pub nft_token_account_to_receive: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
    pub token_metadata_program: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
}

impl<'info> BuyRecord<'info> {
    fn into_transfer_nft_context(
        &self,
    ) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.nft_token_account_to_send.clone(),
            to: self
                .nft_token_account_to_receive
                .clone(),
            authority: self.initializer.clone(),
        };
        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }
}
// fetch nft data from repository account with hero_id
fn get_nft_data_from_repository<'a>(
    hero_id: u8,
    key_nft: &Pubkey,
    repository_account: AccountInfo<'a>,
    nft_account: AccountInfo<'a>,
) -> Result<NFTRecord, ProgramError> {
    let start: usize = hero_id as usize * NFT_RECORD_SIZE;
    let end: usize = start + NFT_RECORD_SIZE;

    let nft_record: NFTRecord = NFTRecord::deserialize(&mut &repository_account.data.borrow()[start..end])?;
    
    if nft_record.key_nft != *key_nft || nft_record.key_nft != *nft_account.key {
        msg!("NFT Key dismatch.");
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(nft_record)
}

// modify nft data to repository
fn save_nft_data_to_repository<'a>(
    nft_record: &NFTRecord,
    repository_account: AccountInfo<'a>,
) -> Result<(), ProgramError> {
    let start: usize = nft_record.hero_id as usize * NFT_RECORD_SIZE;
    let end: usize = start + NFT_RECORD_SIZE;
    nft_record.serialize(&mut &mut repository_account.data.borrow_mut()[start..end])?;
    Ok(())
}

// transfer sol
fn sol_transfer<'a>(
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    system_program: AccountInfo<'a>,
    amount: u64,
) -> Result<(), ProgramError> {
    let ix = solana_program::system_instruction::transfer(source.key, destination.key, amount);
    invoke(&ix, &[source, destination, system_program])
}

// verify repository editable authority
fn verify_admin_authority<'a>(
    admin_account_pk: &Pubkey,
    repository_account_pk: &Pubkey,
    program_id: &Pubkey
) -> Result<(), ProgramError> {

    // verify seed matching - it means verify edit authority of signer
    let expected_repo_account_pubkey = Pubkey::create_with_seed(
        admin_account_pk, REPO_ACCOUNT_SEED, program_id
    )?;

    if expected_repo_account_pubkey != *repository_account_pk {
        msg!("Illegal Admin! Seed dismatch. No authority to modify me.");
        return Err(ProgramError::IncorrectProgramId);
    }

    Ok(())
}

// update metadata account
fn update_metadata_old_nft<'a>(
    ctx: &Context<BuyRecord>,
    dead_uri: String,
    dead_name: String
) -> Result<(), ProgramError> {
    
    let mut old_metadata = Metadata::from_account_info(&ctx.accounts.dead_nft_metadata_account).unwrap();
    // verify validation of metadata account
    if old_metadata.mint != *ctx.accounts.dead_nft_mint.key
    {
        msg!("nft_metadata_account is not valid account");
        return Err(ProgramError::InvalidAccountData);
    }
    old_metadata.data.uri = dead_uri.to_string();
    old_metadata.data.name = dead_name.to_string();
    let update_metadata_instruction = update_metadata_accounts(
        spl_token_metadata::id(),               // program_id
        *ctx.accounts.dead_nft_metadata_account.key,          // metadata_account
        *ctx.accounts.initializer.key,                     // update_authority
        Some(*ctx.accounts.initializer.key),               // new_update_authority
        Some(old_metadata.data),                // data
        Some(true)                              // primary_sale_happened
    );
    invoke(
        &update_metadata_instruction,
        &[
            ctx.accounts.dead_nft_metadata_account.clone(),
            ctx.accounts.initializer.clone(),
            ctx.accounts.dead_nft_metadata_account.clone(),
            ctx.accounts.token_metadata_program.clone()
        ]
    )
}