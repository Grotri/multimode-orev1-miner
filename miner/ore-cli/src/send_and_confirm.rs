use std::{
    io::{stdout, Write},
    time::Duration,
};
use std::io::BufReader;
use std::io::BufRead;
use ore::state::Proof;
use std::fs::File;
use std::sync::Arc;
use solana_account_decoder::*;
use std::str::FromStr;
use solana_client::{
    client_error::{ClientError, ClientErrorKind, Result as ClientResult},
    nonblocking::rpc_client::RpcClient,
};
use solana_transaction_status::UiTransactionEncoding;
use solana_sdk::commitment_config::CommitmentLevel;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use crate::utils::get_proof;

use solana_program::system_instruction::transfer;
use solana_sdk::{
    self,
    address_lookup_table_account::AddressLookupTableAccount,
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signature},
    signer::Signer,
    transaction::{Transaction, VersionedTransaction},
};
use solana_transaction_status::TransactionConfirmationStatus;
// use solana_sdk::instruction::Instruction;
use jito_searcher_client::{
    send_bundle_no_wait, send_bundle_with_confirmation, get_searcher_client
};
use jito_protos::{
    searcher::{
        mempool_subscription, searcher_service_client::SearcherServiceClient,
        ConnectedLeadersRegionedRequest, GetTipAccountsRequest, MempoolSubscription,
        NextScheduledLeaderRequest, PendingTxNotification, ProgramSubscriptionV0,
        SubscribeBundleResultsRequest, WriteLockedAccountSubscriptionV0,
    },
};
use tokio::time::{sleep, timeout};
use log::info;
use crate::Miner;

const RPC_RETRIES: usize = 0;
const SIMULATION_RETRIES: usize = 1;
const GATEWAY_RETRIES: usize = 0;
const CONFIRM_RETRIES: usize = 1;

impl Miner {
    pub async fn send_and_confirm_bigbundle(
        &self,
        ixs: Vec<Instruction>,
        proofs: &Vec<Proof>,
        signers: &Vec<Keypair>,
        skip_confirm: bool,
    ) {
        let mut stdout = stdout();
        let jito_keypair = Arc::new(self.jito_keypair());
        let tip_account = self.tip_account();
        let mut jito_client_am = self.jito_client_am();
        let mut jito_client_fr = self.jito_client_fr();
        let mut jito_client_ny = self.jito_client_ny();
        let mut jito_client_tk = self.jito_client_tk();
        let mut regions = self.regions();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());
        let fee_payer = read_keypair_file("fee_payer.json").unwrap();

        // Build tx
        let (mut hash, mut slot) = client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await
            .unwrap();

        let mut tx = VersionedTransaction::try_new(
            VersionedMessage::V0(v0::Message::try_compile(
                &fee_payer.pubkey(),
                &[ComputeBudgetInstruction::set_compute_unit_limit(12000), ixs[0].clone(), ixs[1].clone(), ixs[2].clone(), ixs[3].clone(), ixs[4].clone()],
                &[],
                hash
            ).unwrap()),
            &vec![&fee_payer, &signers[0], &signers[1], &signers[2], &signers[3], &signers[4]],
        ).unwrap();
        //     &[
        //         ixs[0].clone(),
        //         ixs[1].clone(),
        //         ixs[2].clone(),
        //         ixs[3].clone(),
        //         ixs[4].clone(),
        //     ],
        //     Some(&fee_payer.pubkey()));
        // // Submit tx
        // tx.partial_sign(&[&fee_payer], hash);
        // tx.partial_sign(&[&signers[0]], hash);
        // tx.partial_sign(&[&signers[1]], hash);
        // tx.partial_sign(&[&signers[2]], hash);
        // tx.partial_sign(&[&signers[3]], hash);
        // tx.partial_sign(&[&signers[4]], hash);

        let mut tx2 = VersionedTransaction::try_new(
            VersionedMessage::V0(v0::Message::try_compile(
                &fee_payer.pubkey(),
                &[ComputeBudgetInstruction::set_compute_unit_limit(12000), ixs[5].clone(), ixs[6].clone(), ixs[7].clone(), ixs[8].clone(), ixs[9].clone()],
                &[],
                hash
            ).unwrap()),
            &vec![&fee_payer, &signers[5], &signers[6], &signers[7], &signers[8], &signers[9]],
        ).unwrap();

        let mut tx3 = VersionedTransaction::try_new(
            VersionedMessage::V0(v0::Message::try_compile(
                &fee_payer.pubkey(),
                &[ComputeBudgetInstruction::set_compute_unit_limit(12000), ixs[10].clone(), ixs[11].clone(), ixs[12].clone(), ixs[13].clone(), ixs[14].clone()],
                &[],
                hash
            ).unwrap()),
            &vec![&fee_payer, &signers[10], &signers[11], &signers[12], &signers[13], &signers[14]],
        ).unwrap();

        let mut tx4 = VersionedTransaction::try_new(
            VersionedMessage::V0(v0::Message::try_compile(
                &fee_payer.pubkey(),
                &[ComputeBudgetInstruction::set_compute_unit_limit(12000), ixs[15].clone(), ixs[16].clone(), ixs[17].clone(), ixs[18].clone(), ixs[19].clone()],
                &[],
                hash
            ).unwrap()),
            &vec![&fee_payer, &signers[15], &signers[16], &signers[17], &signers[18], &signers[19]],
        ).unwrap();

        let mut tip = VersionedTransaction::try_new(
            VersionedMessage::V0(v0::Message::try_compile(
                &fee_payer.pubkey(),
                &[ixs[20].clone(), ixs[21].clone(), ixs[22].clone(), ixs[23].clone(), ixs[24].clone(), transfer(&fee_payer.pubkey(), &tip_account, self.priority_fee)],
                &[],
                hash
            ).unwrap()),
            &vec![&fee_payer, &signers[20], &signers[21], &signers[22], &signers[23], &signers[24]],
        ).unwrap();

        let sig = tx.signatures[0];
        let txs = vec![tx, tx2, tx3, tx4, tip];

        // let mut sigs = vec![];
        let mut attempts = 0;
        loop {
            let mut proof_ = get_proof(self.cluster.clone(), signers[20].pubkey()).await;
            if proof_.hash.ne(&proofs[20].hash) {
                println!("Hash already validated! An earlier transaction must have landed.");            
                break;
            }
            for i in 1..5{
                match timeout(Duration::from_millis(1000), send_bundle_no_wait(
                    &*txs,
                    // &client,
                    &mut jito_client_am,
                    // &mut bundle_results_subscription,
                )).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            let mut proof_ = get_proof(self.cluster.clone(), signers[20].pubkey()).await;
            if proof_.hash.ne(&proofs[20].hash) {
                println!("Hash already validated! An earlier transaction must have landed.");            
                break;
            }
            for i in 1..5{
                match timeout(Duration::from_millis(1000), send_bundle_no_wait(
                    &*txs,
                    // &client,
                    &mut jito_client_fr,
                    // &mut bundle_results_subscription,
                )).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            let mut proof_ = get_proof(self.cluster.clone(), signers[20].pubkey()).await;
            if proof_.hash.ne(&proofs[20].hash) {
                println!("Hash already validated! An earlier transaction must have landed.");            
                break;
            }
            for i in 1..5{
                match timeout(Duration::from_millis(1000), send_bundle_no_wait(
                    &*txs,
                    // &client,
                    &mut jito_client_ny,
                    // &mut bundle_results_subscription,
                )).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            let mut proof_ = get_proof(self.cluster.clone(), signers[20].pubkey()).await;
            if proof_.hash.ne(&proofs[20].hash) {
                println!("Hash already validated! An earlier transaction must have landed.");            
                break;
            }
            for i in 1..5{
                match timeout(Duration::from_millis(1000), send_bundle_no_wait(
                    &*txs,
                    // &client,
                    &mut jito_client_tk,
                    // &mut bundle_results_subscription,
                )).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            stdout.flush().ok();

            attempts += 1;
            if attempts > GATEWAY_RETRIES {
                break;
            }
        }
    }

    pub async fn send_and_confirm_transfer(
        &self,
        amounts: Vec<u64>,
        signers: &Vec<Keypair>,
        token_accs: Vec<Pubkey>,
        skip_confirm: bool,
    ) {
        let mut stdout = stdout();
        let jito_keypair = self.jito_keypair();
        let tip_account = self.tip_account();
        let mut jito_client = self.jito_client_am();
        let mut regions = self.regions();
        let mut ore_prog = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        let mut to_wallet = Pubkey::from_str("4KHZodEajg6T7y18NnRtRGG2qM46WEEqGyCVpaUhS5aX").unwrap();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());
        let fee_payer = read_keypair_file("fee_payer.json").unwrap();

        let mut balances: Vec<u64> = Vec::new();
        for pk in 0..Miner::calculate_length(&signers) {
            let token_acc = (client.get_token_account(&token_accs[pk]).await.unwrap().unwrap().token_amount.ui_amount.unwrap() * 10f64.powf(ore::TOKEN_DECIMALS as f64)) as u64;
            // let bal = client.get_token_account_balance(token_acc).await.unwrap().ui_amount.unwrap() as u64;
            println!("{token_acc}");
            balances.push(token_acc);
        }

        // // Return error if balance is zero
        // let balance = client
        //     .get_balance_with_commitment(&signer.pubkey(), CommitmentConfig::confirmed())
        //     .await
        //     .unwrap();

        // if balance.value <= 0 {
        //     return Err(ClientError {
        //         request: None,
        //         kind: ClientErrorKind::Custom("Insufficient SOL balance".into()),
        //     });
        // }

        let mut bundle_results_subscription = jito_client
                .subscribe_bundle_results(SubscribeBundleResultsRequest {})
                .await
                .expect("subscribe to bundle results")
                .into_inner();

        let mut is_leader_slot = false;
        while !is_leader_slot {
            let next_leader = jito_client
                .get_next_scheduled_leader(NextScheduledLeaderRequest {
                    regions: regions.clone(),
                })
                .await
                .expect("gets next scheduled leader")
                .into_inner();
            let num_slots = next_leader.next_leader_slot - next_leader.current_slot;
            is_leader_slot = num_slots <= 2;
            println!("next jito leader slot in {num_slots} slots in {0}", next_leader.next_leader_region);
            sleep(Duration::from_millis(500)).await;
        }

        // Build tx
        let (mut hash, mut slot) = client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await
            .unwrap();

        let mut tx = Transaction::new_with_payer(
            &[
                spl_token::instruction::transfer(&(ore_prog), &token_accs[0], &(to_wallet), &signers[0].pubkey(), &[&signers[0].pubkey()], balances[0]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[1], &(to_wallet), &signers[1].pubkey(), &[&signers[1].pubkey()], balances[1]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[2], &(to_wallet), &signers[2].pubkey(), &[&signers[2].pubkey()], balances[2]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[3], &(to_wallet), &signers[3].pubkey(), &[&signers[3].pubkey()], balances[3]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[4], &(to_wallet), &signers[4].pubkey(), &[&signers[4].pubkey()], balances[4]).unwrap(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx.partial_sign(&[&fee_payer], hash);
        tx.partial_sign(&[&signers[0]], hash);
        tx.partial_sign(&[&signers[1]], hash);
        tx.partial_sign(&[&signers[2]], hash);
        tx.partial_sign(&[&signers[3]], hash);
        tx.partial_sign(&[&signers[4]], hash);

        let mut tx2 = Transaction::new_with_payer(
            &[
                spl_token::instruction::transfer(&(ore_prog), &token_accs[5], &(to_wallet), &signers[5].pubkey(), &[&signers[5].pubkey()], balances[5]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[6], &(to_wallet), &signers[6].pubkey(), &[&signers[6].pubkey()], balances[6]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[7], &(to_wallet), &signers[7].pubkey(), &[&signers[7].pubkey()], balances[7]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[8], &(to_wallet), &signers[8].pubkey(), &[&signers[8].pubkey()], balances[8]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[9], &(to_wallet), &signers[9].pubkey(), &[&signers[9].pubkey()], balances[9]).unwrap(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx2.partial_sign(&[&fee_payer], hash);
        tx2.partial_sign(&[&signers[5]], hash);
        tx2.partial_sign(&[&signers[6]], hash);
        tx2.partial_sign(&[&signers[7]], hash);
        tx2.partial_sign(&[&signers[8]], hash);
        tx2.partial_sign(&[&signers[9]], hash);

        let mut tx3 = Transaction::new_with_payer(
            &[
                spl_token::instruction::transfer(&(ore_prog), &token_accs[10], &(to_wallet), &signers[10].pubkey(), &[&signers[10].pubkey()], balances[10]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[11], &(to_wallet), &signers[11].pubkey(), &[&signers[11].pubkey()], balances[11]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[12], &(to_wallet), &signers[12].pubkey(), &[&signers[12].pubkey()], balances[12]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[13], &(to_wallet), &signers[13].pubkey(), &[&signers[13].pubkey()], balances[13]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[14], &(to_wallet), &signers[14].pubkey(), &[&signers[14].pubkey()], balances[14]).unwrap(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx3.partial_sign(&[&fee_payer], hash);
        tx3.partial_sign(&[&signers[10]], hash);
        tx3.partial_sign(&[&signers[11]], hash);
        tx3.partial_sign(&[&signers[12]], hash);
        tx3.partial_sign(&[&signers[13]], hash);
        tx3.partial_sign(&[&signers[14]], hash);

        let mut tx4 = Transaction::new_with_payer(
            &[
                spl_token::instruction::transfer(&(ore_prog), &token_accs[15], &(to_wallet), &signers[15].pubkey(), &[&signers[15].pubkey()], balances[15]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[16], &(to_wallet), &signers[16].pubkey(), &[&signers[16].pubkey()], balances[16]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[17], &(to_wallet), &signers[17].pubkey(), &[&signers[17].pubkey()], balances[17]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[18], &(to_wallet), &signers[18].pubkey(), &[&signers[18].pubkey()], balances[18]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[19], &(to_wallet), &signers[19].pubkey(), &[&signers[19].pubkey()], balances[19]).unwrap(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx4.partial_sign(&[&fee_payer], hash);
        tx4.partial_sign(&[&signers[15]], hash);
        tx4.partial_sign(&[&signers[16]], hash);
        tx4.partial_sign(&[&signers[17]], hash);
        tx4.partial_sign(&[&signers[18]], hash);
        tx4.partial_sign(&[&signers[19]], hash);

        let mut tip = Transaction::new_with_payer(
            &[
                spl_token::instruction::transfer(&(ore_prog), &token_accs[20], &(to_wallet), &signers[20].pubkey(), &[&signers[20].pubkey()], balances[20]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[21], &(to_wallet), &signers[21].pubkey(), &[&signers[21].pubkey()], balances[21]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[22], &(to_wallet), &signers[22].pubkey(), &[&signers[22].pubkey()], balances[22]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[23], &(to_wallet), &signers[23].pubkey(), &[&signers[23].pubkey()], balances[23]).unwrap(),
                spl_token::instruction::transfer(&(ore_prog), &token_accs[24], &(to_wallet), &signers[24].pubkey(), &[&signers[24].pubkey()], balances[24]).unwrap(),
                transfer(&fee_payer.pubkey(), &tip_account, self.priority_fee),
            ],
            Some(&fee_payer.pubkey()));

        tip.partial_sign(&[&fee_payer], hash);
        tip.partial_sign(&[&signers[20]], hash);
        tip.partial_sign(&[&signers[21]], hash);
        tip.partial_sign(&[&signers[22]], hash);
        tip.partial_sign(&[&signers[23]], hash);
        tip.partial_sign(&[&signers[24]], hash);

        let sig = tx.signatures[0];
        let txs = vec![VersionedTransaction::from(tx), VersionedTransaction::from(tx2), VersionedTransaction::from(tx3), VersionedTransaction::from(tx4), VersionedTransaction::from(tip)];

        let mut attempts = 0;
        loop {

            match send_bundle_with_confirmation(
                &*txs,
                &client,
                &mut jito_client,
                &mut bundle_results_subscription,
            ).await {
                Ok(sig) => {
                    println!("Transaction sent!");
                }
                Err(err) => {
                    println!("Error {:?}", err);
                }
            };
            sleep(Duration::from_millis(100)).await;
            stdout.flush().ok();

            attempts += 1;
            if attempts > GATEWAY_RETRIES {
                break;
            }
        }
    }

    pub async fn send_and_confirm_claim(
        &self,
        ixs: Vec<Instruction>,
        signers: &Vec<Keypair>,
        skip_confirm: bool,
    ) {
        let mut stdout = stdout();
        let jito_keypair = self.jito_keypair();
        let tip_account = self.tip_account();
        let mut jito_client = self.jito_client_am();
        let mut regions = self.regions();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());
        let fee_payer = read_keypair_file("fee_payer.json").unwrap();

        let mut bundle_results_subscription = jito_client
                .subscribe_bundle_results(SubscribeBundleResultsRequest {})
                .await
                .expect("subscribe to bundle results")
                .into_inner();

        let mut is_leader_slot = false;
        while !is_leader_slot {
            let next_leader = jito_client
                .get_next_scheduled_leader(NextScheduledLeaderRequest {
                    regions: regions.clone(),
                })
                .await
                .expect("gets next scheduled leader")
                .into_inner();
            let num_slots = next_leader.next_leader_slot - next_leader.current_slot;
            is_leader_slot = num_slots <= 2;
            println!("next jito leader slot in {num_slots} slots in {0}", next_leader.next_leader_region);
            sleep(Duration::from_millis(500)).await;
        }

        // Build tx
        let (mut hash, mut slot) = client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await
            .unwrap();

        let mut tx = Transaction::new_with_payer(
            &[
                ixs[0].clone(),
                ixs[1].clone(),
                ixs[2].clone(),
                ixs[3].clone(),
                ixs[4].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx.partial_sign(&[&fee_payer], hash);
        tx.partial_sign(&[&signers[0]], hash);
        tx.partial_sign(&[&signers[1]], hash);
        tx.partial_sign(&[&signers[2]], hash);
        tx.partial_sign(&[&signers[3]], hash);
        tx.partial_sign(&[&signers[4]], hash);

        let mut tx2 = Transaction::new_with_payer(
            &[
                ixs[5].clone(),
                ixs[6].clone(),
                ixs[7].clone(),
                ixs[8].clone(),
                ixs[9].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx2.partial_sign(&[&fee_payer], hash);
        tx2.partial_sign(&[&signers[5]], hash);
        tx2.partial_sign(&[&signers[6]], hash);
        tx2.partial_sign(&[&signers[7]], hash);
        tx2.partial_sign(&[&signers[8]], hash);
        tx2.partial_sign(&[&signers[9]], hash);

        let mut tx3 = Transaction::new_with_payer(
            &[
                ixs[10].clone(),
                ixs[11].clone(),
                ixs[12].clone(),
                ixs[13].clone(),
                ixs[14].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx3.partial_sign(&[&fee_payer], hash);
        tx3.partial_sign(&[&signers[10]], hash);
        tx3.partial_sign(&[&signers[11]], hash);
        tx3.partial_sign(&[&signers[12]], hash);
        tx3.partial_sign(&[&signers[13]], hash);
        tx3.partial_sign(&[&signers[14]], hash);

        let mut tx4 = Transaction::new_with_payer(
            &[
                ixs[15].clone(),
                ixs[16].clone(),
                ixs[17].clone(),
                ixs[18].clone(),
                ixs[19].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx4.partial_sign(&[&fee_payer], hash);
        tx4.partial_sign(&[&signers[15]], hash);
        tx4.partial_sign(&[&signers[16]], hash);
        tx4.partial_sign(&[&signers[17]], hash);
        tx4.partial_sign(&[&signers[18]], hash);
        tx4.partial_sign(&[&signers[19]], hash);

        let mut tip = Transaction::new_with_payer(
            &[
                ixs[20].clone(),
                ixs[21].clone(),
                ixs[22].clone(),
                ixs[23].clone(),
                ixs[24].clone(),
                transfer(&fee_payer.pubkey(), &tip_account, self.priority_fee),
            ],
            Some(&fee_payer.pubkey()));

        tip.partial_sign(&[&fee_payer], hash);
        tip.partial_sign(&[&signers[20]], hash);
        tip.partial_sign(&[&signers[21]], hash);
        tip.partial_sign(&[&signers[22]], hash);
        tip.partial_sign(&[&signers[23]], hash);
        tip.partial_sign(&[&signers[24]], hash);

        let sig = tx.signatures[0];
        let txs = vec![VersionedTransaction::from(tx), VersionedTransaction::from(tx2), VersionedTransaction::from(tx3), VersionedTransaction::from(tx4), VersionedTransaction::from(tip)];

        let mut attempts = 0;
        loop {

            match send_bundle_with_confirmation(
                &*txs,
                &client,
                &mut jito_client,
                &mut bundle_results_subscription,
            ).await {
                Ok(sig) => {
                    println!("Transaction sent!");
                }
                Err(err) => {
                    println!("Error {:?}", err);
                }
            };
            sleep(Duration::from_millis(100)).await;
            stdout.flush().ok();

            attempts += 1;
            if attempts > GATEWAY_RETRIES {
                break;
            }
        }
    }

    pub async fn send_and_confirm_claim1(
        &self,
        ixs: Vec<Instruction>,
        signers: &Vec<Keypair>,
        skip_confirm: bool,
    ) {
        let mut stdout = stdout();
        let jito_keypair = self.jito_keypair();
        let tip_account = self.tip_account();
        let mut jito_client = self.jito_client_am();
        let mut regions = self.regions();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());
        let fee_payer = read_keypair_file("fee_payer.json").unwrap();

        let mut bundle_results_subscription = jito_client
                .subscribe_bundle_results(SubscribeBundleResultsRequest {})
                .await
                .expect("subscribe to bundle results")
                .into_inner();

        let mut is_leader_slot = false;
        while !is_leader_slot {
            let next_leader = jito_client
                .get_next_scheduled_leader(NextScheduledLeaderRequest {
                    regions: regions.clone(),
                })
                .await
                .expect("gets next scheduled leader")
                .into_inner();
            let num_slots = next_leader.next_leader_slot - next_leader.current_slot;
            is_leader_slot = num_slots <= 2;
            println!("next jito leader slot in {num_slots} slots in {0}", next_leader.next_leader_region);
            sleep(Duration::from_millis(500)).await;
        }

        // Build tx
        let (mut hash, mut slot) = client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await
            .unwrap();

        let mut tx = Transaction::new_with_payer(
            &[
                ixs[0].clone(),
                ixs[1].clone(),
                ixs[2].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx.partial_sign(&[&fee_payer], hash);
        tx.partial_sign(&[&signers[0]], hash);
        tx.partial_sign(&[&signers[1]], hash);
        tx.partial_sign(&[&signers[2]], hash);

        let mut tx2 = Transaction::new_with_payer(
            &[
                ixs[3].clone(),
                ixs[4].clone(),
                ixs[5].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx2.partial_sign(&[&fee_payer], hash);
        tx2.partial_sign(&[&signers[3]], hash);
        tx2.partial_sign(&[&signers[4]], hash);
        tx2.partial_sign(&[&signers[5]], hash);

        let mut tx3 = Transaction::new_with_payer(
            &[
                ixs[6].clone(),
                ixs[7].clone(),
                ixs[8].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx3.partial_sign(&[&fee_payer], hash);
        tx3.partial_sign(&[&signers[6]], hash);
        tx3.partial_sign(&[&signers[7]], hash);
        tx3.partial_sign(&[&signers[8]], hash);

        let mut tx4 = Transaction::new_with_payer(
            &[
                ixs[9].clone(),
                ixs[10].clone(),
                ixs[11].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx4.partial_sign(&[&fee_payer], hash);
        tx4.partial_sign(&[&signers[9]], hash);
        tx4.partial_sign(&[&signers[10]], hash);
        tx4.partial_sign(&[&signers[11]], hash);

        let mut tip = Transaction::new_with_payer(
            &[
                ixs[12].clone(),
                ixs[13].clone(),
                ixs[14].clone(),
                transfer(&fee_payer.pubkey(), &tip_account, self.priority_fee),
            ],
            Some(&fee_payer.pubkey()));

        tip.partial_sign(&[&fee_payer], hash);
        tip.partial_sign(&[&signers[12]], hash);
        tip.partial_sign(&[&signers[13]], hash);
        tip.partial_sign(&[&signers[14]], hash);

        let sig = tx.signatures[0];
        let txs = vec![VersionedTransaction::from(tx), VersionedTransaction::from(tx2), VersionedTransaction::from(tx3), VersionedTransaction::from(tx4), VersionedTransaction::from(tip)];

        let mut attempts = 0;
        loop {

            match send_bundle_with_confirmation(
                &*txs,
                &client,
                &mut jito_client,
                &mut bundle_results_subscription,
            ).await {
                Ok(sig) => {
                    println!("Transaction sent!");
                }
                Err(err) => {
                    println!("Error {:?}", err);
                }
            };
            sleep(Duration::from_millis(100)).await;
            stdout.flush().ok();

            attempts += 1;
            if attempts > 0 {
                break;
            }
        }
    }

    pub async fn send_and_confirm_claim2(
        &self,
        ixs: Vec<Instruction>,
        signers: &Vec<Keypair>,
        skip_confirm: bool,
    ) {
        let mut stdout = stdout();
        let jito_keypair = self.jito_keypair();
        let tip_account = self.tip_account();
        let mut jito_client = self.jito_client_am();
        let mut regions = self.regions();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());
        let fee_payer = read_keypair_file("fee_payer.json").unwrap();

        let mut bundle_results_subscription = jito_client
                .subscribe_bundle_results(SubscribeBundleResultsRequest {})
                .await
                .expect("subscribe to bundle results")
                .into_inner();

        let mut is_leader_slot = false;
        while !is_leader_slot {
            let next_leader = jito_client
                .get_next_scheduled_leader(NextScheduledLeaderRequest {
                    regions: regions.clone(),
                })
                .await
                .expect("gets next scheduled leader")
                .into_inner();
            let num_slots = next_leader.next_leader_slot - next_leader.current_slot;
            is_leader_slot = num_slots <= 2;
            println!("next jito leader slot in {num_slots} slots in {0}", next_leader.next_leader_region);
            sleep(Duration::from_millis(500)).await;
        }

        // Build tx
        let (mut hash, mut slot) = client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await
            .unwrap();

        let mut tx = Transaction::new_with_payer(
            &[
                ixs[15].clone(),
                ixs[16].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx.partial_sign(&[&fee_payer], hash);
        tx.partial_sign(&[&signers[15]], hash);
        tx.partial_sign(&[&signers[16]], hash);

        let mut tx2 = Transaction::new_with_payer(
            &[
                ixs[17].clone(),
                ixs[18].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx2.partial_sign(&[&fee_payer], hash);
        tx2.partial_sign(&[&signers[17]], hash);
        tx2.partial_sign(&[&signers[18]], hash);

        let mut tx3 = Transaction::new_with_payer(
            &[
                ixs[19].clone(),
                ixs[20].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx3.partial_sign(&[&fee_payer], hash);
        tx3.partial_sign(&[&signers[19]], hash);
        tx3.partial_sign(&[&signers[20]], hash);

        let mut tx4 = Transaction::new_with_payer(
            &[
                ixs[21].clone(),
                ixs[22].clone(),
            ],
            Some(&fee_payer.pubkey()));
        // Submit tx
        tx4.partial_sign(&[&fee_payer], hash);
        tx4.partial_sign(&[&signers[21]], hash);
        tx4.partial_sign(&[&signers[22]], hash);

        let mut tip = Transaction::new_with_payer(
            &[
                ixs[23].clone(),
                ixs[24].clone(),
                transfer(&fee_payer.pubkey(), &tip_account, self.priority_fee),
            ],
            Some(&fee_payer.pubkey()));

        tip.partial_sign(&[&fee_payer], hash);
        tip.partial_sign(&[&signers[23]], hash);
        tip.partial_sign(&[&signers[24]], hash);

        let sig = tx.signatures[0];
        let txs = vec![VersionedTransaction::from(tx), VersionedTransaction::from(tx2), VersionedTransaction::from(tx3), VersionedTransaction::from(tx4), VersionedTransaction::from(tip)];

        let mut attempts = 0;
        loop {

            match send_bundle_with_confirmation(
                &*txs,
                &client,
                &mut jito_client,
                &mut bundle_results_subscription,
            ).await {
                Ok(sig) => {
                    println!("Transaction sent!");
                }
                Err(err) => {
                    println!("Error {:?}", err);
                }
            };
            sleep(Duration::from_millis(100)).await;
            stdout.flush().ok();

            attempts += 1;
            if attempts > 0 {
                break;
            }
        }
    }

    pub async fn send_and_confirm_legacy(
        &self,
        ixs: Vec<Instruction>,
        proofs: &Vec<Proof>,
        signers: &Vec<Keypair>,
        skip_confirm: bool,
    ) {
        let mut stdout = stdout();
        let jito_keypair = Arc::new(self.jito_keypair());
        let tip_account = self.tip_account();
        let mut jito_client_am = self.jito_client_am();
        let mut jito_client_fr = self.jito_client_fr();
        let mut jito_client_ny = self.jito_client_ny();
        let mut jito_client_tk = self.jito_client_tk();
        let mut regions = self.regions();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());
        let mut client_nodes: Vec<RpcClient> = Vec::new();
        let node_file = File::open("rpcs.txt").expect("open failed");
        let node_reader = BufReader::new(node_file);
        for node_url in node_reader.lines() {
            client_nodes.push(RpcClient::new_with_commitment(node_url.unwrap(), CommitmentConfig::confirmed()));
        }
        let fee_payer = read_keypair_file("fee_payer.json").unwrap();

        let send_cfg = RpcSendTransactionConfig {
            skip_preflight: false,
            preflight_commitment: Some(CommitmentLevel::Finalized),
            encoding: Some(UiTransactionEncoding::Base64),
            max_retries: Some(RPC_RETRIES),
            min_context_slot: None,
        };
        let mut trx_status = vec!(0, 0, 0, 0, 0);
        loop {
            let mut tx = Transaction::new_with_payer(
                &[
                    ComputeBudgetInstruction::set_compute_unit_limit(12000),
                    ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee),
                    ixs[0].clone(),
                    ixs[1].clone(),
                    ixs[2].clone(),
                    ixs[3].clone(),
                    ixs[4].clone(),
                ],
                Some(&fee_payer.pubkey()));
            // Submit tx
            

            let mut tx2 = Transaction::new_with_payer(
                &[
                    ComputeBudgetInstruction::set_compute_unit_limit(12000),
                    ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee),
                    ixs[5].clone(),
                    ixs[6].clone(),
                    ixs[7].clone(),
                    ixs[8].clone(),
                    ixs[9].clone(),
                ],
                Some(&fee_payer.pubkey()));
            // Submit tx
            

            let mut tx3 = Transaction::new_with_payer(
                &[
                    ComputeBudgetInstruction::set_compute_unit_limit(12000),
                    ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee),
                    ixs[10].clone(),
                    ixs[11].clone(),
                    ixs[12].clone(),
                    ixs[13].clone(),
                    ixs[14].clone(),
                ],
                Some(&fee_payer.pubkey()));
            // Submit tx
            

            let mut tx4 = Transaction::new_with_payer(
                &[
                    ComputeBudgetInstruction::set_compute_unit_limit(12000),
                    ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee),
                    ixs[15].clone(),
                    ixs[16].clone(),
                    ixs[17].clone(),
                    ixs[18].clone(),
                    ixs[19].clone(),
                ],
                Some(&fee_payer.pubkey()));
            // Submit tx
            

            let mut tx5 = Transaction::new_with_payer(
                &[
                    ComputeBudgetInstruction::set_compute_unit_limit(12000),
                    ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee),
                    ixs[20].clone(),
                    ixs[21].clone(),
                    ixs[22].clone(),
                    ixs[23].clone(),
                    ixs[24].clone(),
                ],
                Some(&fee_payer.pubkey()));
            

            let (mut hash, mut slot) = client
                .get_latest_blockhash_with_commitment(CommitmentConfig::finalized())
                .await
                .unwrap();

            tx.partial_sign(&[&fee_payer], hash);
            tx.partial_sign(&[&signers[0]], hash);
            tx.partial_sign(&[&signers[1]], hash);
            tx.partial_sign(&[&signers[2]], hash);
            tx.partial_sign(&[&signers[3]], hash);
            tx.partial_sign(&[&signers[4]], hash);
            tx2.partial_sign(&[&fee_payer], hash);
            tx2.partial_sign(&[&signers[5]], hash);
            tx2.partial_sign(&[&signers[6]], hash);
            tx2.partial_sign(&[&signers[7]], hash);
            tx2.partial_sign(&[&signers[8]], hash);
            tx2.partial_sign(&[&signers[9]], hash);
            tx3.partial_sign(&[&fee_payer], hash);
            tx3.partial_sign(&[&signers[10]], hash);
            tx3.partial_sign(&[&signers[11]], hash);
            tx3.partial_sign(&[&signers[12]], hash);
            tx3.partial_sign(&[&signers[13]], hash);
            tx3.partial_sign(&[&signers[14]], hash);
            tx4.partial_sign(&[&fee_payer], hash);
            tx4.partial_sign(&[&signers[15]], hash);
            tx4.partial_sign(&[&signers[16]], hash);
            tx4.partial_sign(&[&signers[17]], hash);
            tx4.partial_sign(&[&signers[18]], hash);
            tx4.partial_sign(&[&signers[19]], hash);
            tx5.partial_sign(&[&fee_payer], hash);
            tx5.partial_sign(&[&signers[20]], hash);
            tx5.partial_sign(&[&signers[21]], hash);
            tx5.partial_sign(&[&signers[22]], hash);
            tx5.partial_sign(&[&signers[23]], hash);
            tx5.partial_sign(&[&signers[24]], hash);

            if trx_status[0] + trx_status[1] + trx_status[2] + trx_status[3] + trx_status[4] >= 3 {
                println!("All transactions landed successfully");
                break;
            }
            // 1
            if trx_status[0] == 1 {
                println!("Hash already validated! An earlier transaction must have landed.");
            }
            if trx_status[0] == 0 {
                let mut proof_ = get_proof(self.cluster.clone(), signers[0].pubkey()).await;
                if proof_.hash.ne(&proofs[0].hash) {
                    println!("Hash already validated! An earlier transaction must have landed.");            
                    trx_status[0] = 1;
                }
            }
            if trx_status[0] == 0 {
                match client_nodes[0].send_transaction_with_config(&tx, send_cfg).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            // 2
            if trx_status[1] == 1 {
                println!("Hash already validated! An earlier transaction must have landed.");
            }
            if trx_status[1] == 0 {
                let mut proof_ = get_proof(self.cluster.clone(), signers[5].pubkey()).await;
                if proof_.hash.ne(&proofs[5].hash) {
                    println!("Hash already validated! An earlier transaction must have landed.");            
                    trx_status[1] = 1;
                }
            }
            if trx_status[1] == 0 {
                match client_nodes[1].send_transaction_with_config(&tx2, send_cfg).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            // 3
            if trx_status[2] == 1 {
                println!("Hash already validated! An earlier transaction must have landed.");
            }
            if trx_status[2] == 0 {
                let mut proof_ = get_proof(self.cluster.clone(), signers[10].pubkey()).await;
                if proof_.hash.ne(&proofs[10].hash) {
                    println!("Hash already validated! An earlier transaction must have landed.");            
                    trx_status[2] = 1;
                }
            }
            if trx_status[2] == 0 {
                match client_nodes[2].send_transaction_with_config(&tx3, send_cfg).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            // 4
            if trx_status[3] == 1 {
                println!("Hash already validated! An earlier transaction must have landed.");
            }
            if trx_status[3] == 0 {
                let mut proof_ = get_proof(self.cluster.clone(), signers[15].pubkey()).await;
                if proof_.hash.ne(&proofs[15].hash) {
                    println!("Hash already validated! An earlier transaction must have landed.");            
                    trx_status[3] = 1;
                }
            }
            if trx_status[3] == 0 {
                match client_nodes[3].send_transaction_with_config(&tx4, send_cfg).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            // 5
            if trx_status[4] == 1 {
                println!("Hash already validated! An earlier transaction must have landed.");
            }
            if trx_status[4] == 0 {
                let mut proof_ = get_proof(self.cluster.clone(), signers[20].pubkey()).await;
                if proof_.hash.ne(&proofs[20].hash) {
                    println!("Hash already validated! An earlier transaction must have landed.");            
                    trx_status[4] = 1;
                }
            }
            if trx_status[4] == 0 {
                match client_nodes[4].send_transaction_with_config(&tx5, send_cfg).await {
                    Ok(sig) => {
                        println!("Transaction sent!");
                    }
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                };
            }
            sleep(Duration::from_millis(1000)).await;
        }
    }
}
