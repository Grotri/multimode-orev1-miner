use std::{
    io::{stdout, Write},
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use std::time::Instant;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use ore::{self, state::Bus, state::Hash, BUS_ADDRESSES, BUS_COUNT, EPOCH_DURATION};
use ore::state::Proof;
use rand::Rng;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::instruction::Instruction;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    keccak::{hashv, Hash as KeccakHash},
    signature::Signer,
    pubkey::Pubkey,
    signature::read_keypair_file,
    signature::Keypair
};
use jito_protos::searcher::searcher_service_client::SearcherServiceClient;
use crate::{
    utils,
    Miner,
};

// Odds of being selected to submit a reset tx
const RESET_ODDS: u64 = 20;

impl Miner {
    pub async fn mine(&self, threads: u64) {
        println!("Threads: {threads}");
        // Register, if needed.
        let keys = self.keypairs.clone();
        let mut signers: Vec<Keypair> = Vec::new();
        for pair in 0..keys.len() {
            signers.push(read_keypair_file(keys[pair].clone()).unwrap());
        }
        self.register().await;
        let mut stdout = stdout();
        let mut rng = rand::thread_rng();

        // Start mining loop
        loop {
            // Fetch account state
            // let balance = self.get_ore_display_balance().await;
            let treasury = utils::get_treasury(self.cluster.clone()).await;
            let mut proofs: Vec<Proof> = Vec::new();
            for signer in 0..Miner::calculate_length(&signers) {
                proofs.push(utils::get_proof(self.cluster.clone(), signers[signer].pubkey()).await);
            }

            // Escape sequence that clears the screen and the scrollback buffer
            println!("\nMining for a valid hash...");
            // let mut next_hashes: Vec<Hash> = Vec::new();
            // let mut nounces: Vec<u64> = Vec::new();
            // for proof in 0..proofs.clone().len() {
                
            //     let (next_hash, nonce) =
            //         self.find_next_hash_par(&signers[proof], proofs[proof].hash.into(), treasury.difficulty.into(), threads);
            //     next_hashes.push(next_hash.into());
            //     nounces.push(nonce);
            // }

            let mut hash_and_pubkey: Vec<(KeccakHash, Pubkey)> = Vec::new();
            for i in 0..25 {
                hash_and_pubkey.push((proofs[i].hash.into(), signers[i].pubkey()));
            }
            
            let (mining_duration, mining_results) = self
                .mine_hashes_gpu(&treasury.difficulty.into(), &hash_and_pubkey)
                .await;

            let mut next_hashes: Vec<Hash> = Vec::new();
            let mut nounces: Vec<u64> = Vec::new();
            for (hash, nonce) in mining_results {
                next_hashes.push(hash.into());
                nounces.push(nonce);
            }

            // Submit mine tx.
            // Use busses randomly so on each epoch, transactions don't pile on the same busses
            println!("\n\nSubmitting hash for validation...");
            let mut counter = 3;
            'submit: loop {
                if counter >= 3 {
                    counter = 0;
                    println!("Entered checking function...");
                    let mut replaced_proof = 0;
                    for i in 0..Miner::calculate_length(&signers) {
                        let mut proof_ = utils::get_proof(self.cluster.clone(), signers[i].pubkey()).await;
                        if proof_.hash.ne(&proofs[i].hash) {
                            println!("Hash already validated! An earlier transaction must have landed.");
                            
                            replaced_proof += 1;
                            
                            if replaced_proof != 25 {
                                break 'submit;
                            }
                        }
                    }
                }

                // Submit request.
                let bus = self.find_bus_id(treasury.reward_rate).await;
                let bus_rewards = (bus.rewards as f64) / (10f64.powf(ore::TOKEN_DECIMALS as f64));
                println!("Sending on bus {} ({} ORE)", bus.id, bus_rewards);
                counter += 1;
                
                let mut ixs: Vec<Instruction> = Vec::new();
                for signer in 0..Miner::calculate_length(&signers) {
                    let ix_mine = ore::instruction::mine(
                        signers[signer].pubkey(),
                        BUS_ADDRESSES[bus.id as usize],
                        next_hashes[signer].into(),
                        nounces[signer],
                    );
                    ixs.push(ix_mine);
                }  
                self
                    .send_and_confirm_bigbundle(ixs, &proofs, &signers, false)
                    .await;
            }
        }
    }

    pub fn calculate_length(s: &Vec<Keypair>) -> usize {
        s.len()
    }

    pub fn calculate_hash_len(s: &Vec<Hash>) -> usize {
        s.len()
    }

    async fn find_bus_id(&self, reward_rate: u64) -> Bus {
        let mut rng = rand::thread_rng();
        loop {
            let bus_id = rng.gen_range(0..BUS_COUNT);
            if let Ok(bus) = self.get_bus(bus_id).await {
                if bus.rewards.gt(&reward_rate.saturating_mul(4)) {
                    return bus;
                }
            }
        }
    }

    // fn _find_next_hash(&self, hash: KeccakHash, difficulty: KeccakHash) -> (KeccakHash, u64) {
    //     let signer = self.signer();
    //     let mut next_hash: KeccakHash;
    //     let mut nonce = 0u64;
    //     loop {
    //         next_hash = hashv(&[
    //             hash.to_bytes().as_slice(),
    //             signer.pubkey().to_bytes().as_slice(),
    //             nonce.to_le_bytes().as_slice(),
    //         ]);
    //         if next_hash.le(&difficulty) {
    //             break;
    //         } else {
    //             println!("Invalid hash: {} Nonce: {:?}", next_hash.to_string(), nonce);
    //         }
    //         nonce += 1;
    //     }
    //     (next_hash, nonce)
    // }

    fn find_next_hash_par(
        &self,
        signer: &Keypair,
        hash: KeccakHash,
        difficulty: KeccakHash,
        threads: u64,
    ) -> (KeccakHash, u64) {
        println!("Threads in hash func: {threads}");
        let found_solution = Arc::new(AtomicBool::new(false));
        let solution = Arc::new(Mutex::<(KeccakHash, u64)>::new((
            KeccakHash::new_from_array([0; 32]),
            0,
        )));
        let pubkey = signer.pubkey();
        let thread_handles: Vec<_> = (0..threads)
            .map(|i| {
                std::thread::spawn({
                    let found_solution = found_solution.clone();
                    let solution = solution.clone();
                    let mut stdout = stdout();
                    move || {
                        let n = u64::MAX.saturating_div(threads).saturating_mul(i);
                        let mut next_hash: KeccakHash;
                        let mut nonce: u64 = n;
                        loop {
                            next_hash = hashv(&[
                                hash.to_bytes().as_slice(),
                                pubkey.to_bytes().as_slice(),
                                nonce.to_le_bytes().as_slice(),
                            ]);
                            if nonce % 10_000 == 0 {
                                if found_solution.load(std::sync::atomic::Ordering::Relaxed) {
                                    return;
                                }
                                if n == 0 {
                                    stdout
                                        .write_all(
                                            format!("\r{}", next_hash.to_string()).as_bytes(),
                                        )
                                        .ok();
                                }
                            }
                            if next_hash.le(&difficulty) {
                                stdout
                                    .write_all(format!("\r{}", next_hash.to_string()).as_bytes())
                                    .ok();
                                found_solution.store(true, std::sync::atomic::Ordering::Relaxed);
                                let mut w_solution = solution.lock().expect("failed to lock mutex");
                                *w_solution = (next_hash, nonce);
                                return;
                            }
                            nonce += 1;
                        }
                    }
                })
            })
            .collect();

        for thread_handle in thread_handles {
            thread_handle.join().unwrap();
        }

        let r_solution = solution.lock().expect("Failed to get lock");
        *r_solution
    }

    pub async fn mine_hashes_gpu(
        &self,
        difficulty: &KeccakHash,
        hash_and_pubkey: &[(KeccakHash, Pubkey)],
    ) -> (Duration, Vec<(KeccakHash, u64)>) {
        self.mine_hashes(utils::get_gpu_nonce_worker_path(), 0, difficulty, hash_and_pubkey)
            .await
    }

    pub async fn mine_hashes(
        &self,
        worker: PathBuf,
        threads: usize,
        difficulty: &KeccakHash,
        hash_and_pubkey: &[(KeccakHash, Pubkey)],
    ) -> (Duration, Vec<(KeccakHash, u64)>) {
        let mining_start = Instant::now();

        let mut child = tokio::process::Command::new(worker)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("nonce_worker failed to spawn");

        {
            let stdin = child.stdin.as_mut().unwrap();

            stdin.write_u8(threads as u8).await.unwrap();
            stdin.write_all(difficulty.as_ref()).await.unwrap();

            for (hash, pubkey) in hash_and_pubkey {
                stdin.write_all(hash.as_ref()).await.unwrap();
                println!("{hash}");
                stdin.write_all(pubkey.as_ref()).await.unwrap();
            }
        }

        let output = child.wait_with_output().await.unwrap().stdout;
        let mut results = vec![];

        for item in output.chunks(40) {
            let hash = KeccakHash(item[..32].try_into().unwrap());
            let nonce = u64::from_le_bytes(item[32..40].try_into().unwrap());

            results.push((hash, nonce));
        }

        let mining_duration = mining_start.elapsed();
        (mining_duration, results)
    }
}
