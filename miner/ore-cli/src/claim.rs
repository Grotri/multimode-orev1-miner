use std::str::FromStr;
use std::time::Duration;
use tokio::time::{sleep, timeout};

use ore::{self, state::Proof, utils::AccountDeserialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::Signer,
};
use solana_program::instruction::Instruction;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::keypair::read_keypair_file;



use crate::{utils::proof_pubkey, Miner};

impl Miner {
    pub async fn claim(&self, cluster: String, beneficiary: Option<String>, amount: Option<f64>) {
        let keys = self.keypairs.clone();
        let mut signers: Vec<Keypair> = Vec::new();
        for pair in 0..keys.len() {
            signers.push(read_keypair_file(keys[pair].clone()).unwrap());
        }
        let client = RpcClient::new_with_commitment(cluster, CommitmentConfig::confirmed());
        let mut benefs: Vec<Pubkey> = Vec::new();
        for pk in 0..Miner::calculate_length(&signers) {
            let beneficiary = match beneficiary {
                Some(ref beneficiary) => {
                    Pubkey::from_str(&beneficiary).expect("Failed to parse beneficiary address")
                }
                None => self.initialize_ata(&signers[pk]).await,
            };
            benefs.push(beneficiary);
        }
        let mut amounts: Vec<u64> = Vec::new();
        for pk in 0..Miner::calculate_length(&signers) {
            let amount = if let Some(amount) = amount {
                (amount * 10f64.powf(ore::TOKEN_DECIMALS as f64)) as u64
            } else {
                match client.get_account(&proof_pubkey(signers[pk].pubkey())).await {
                    Ok(proof_account) => {
                        let proof = Proof::try_from_bytes(&proof_account.data).unwrap();
                        proof.claimable_rewards
                    }
                    Err(err) => {
                        println!("Error looking up claimable rewards: {:?}", err);
                        return;
                    }
                }
            };
            amounts.push(amount);
        }

        let mut ixes: Vec<Instruction> = Vec::new();
        for pk in 0..Miner::calculate_length(&signers) {
            let ix = ore::instruction::claim(signers[pk].pubkey(), benefs[pk], amounts[pk]);
            ixes.push(ix);
        }
        println!("Submitting claim transaction...");
        self
            .send_and_confirm_claim1(ixes.clone(), &signers, false)
            .await;
        sleep(Duration::from_millis(10000)).await;
        self
            .send_and_confirm_claim2(ixes.clone(), &signers, false)
            .await;
        sleep(Duration::from_millis(10000)).await;
        self
            .send_and_confirm_transfer(amounts, &signers, benefs, false)
            .await;
    }

    async fn initialize_ata(&self, signer: &Keypair) -> Pubkey {
        let fee_payer = read_keypair_file("fee_payer.json").unwrap();
        let keys_init = self.keypairs.clone();
        let mut signers_init: Vec<Keypair> = Vec::new();
        for pair in 0..keys_init.len() {
            signers_init.push(read_keypair_file(keys_init[pair].clone()).unwrap());
        }
        // Initialize client.
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());

        // Build instructions.
        let token_account_pubkey = spl_associated_token_account::get_associated_token_address(
            &signer.pubkey(),
            &ore::MINT_ADDRESS,
        );

        // Check if ata already exists
        if let Ok(Some(_ata)) = client.get_token_account(&token_account_pubkey).await {
            return token_account_pubkey;
        }

        let mut ixes: Vec<Instruction> = Vec::new();
        for pk in 0..Miner::calculate_length(&signers_init) {
            let ix = spl_associated_token_account::instruction::create_associated_token_account(
                &signers_init[pk].pubkey(),
                &signers_init[pk].pubkey(),
                &ore::MINT_ADDRESS,
                &spl_token::id(),
            );
            ixes.push(ix);
        }
        println!("Creating token account {}...", token_account_pubkey);
        self.send_and_confirm_claim(ixes, &signers_init, false).await;

        // Return token account address
        token_account_pubkey
    }
}
