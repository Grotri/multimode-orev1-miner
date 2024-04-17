mod balance;
mod busses;
mod claim;
mod cu_limits;
#[cfg(feature = "admin")]
mod initialize;
mod mine;
mod register;
mod rewards;
mod send_and_confirm;
mod treasury;
#[cfg(feature = "admin")]
mod update_admin;
#[cfg(feature = "admin")]
mod update_difficulty;
mod utils;

use std::str::FromStr;
use std::sync::Arc;
use clap::{command, Parser, Subcommand};
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Keypair};
use tonic::codegen::InterceptedService;
use tonic::transport::Channel;
use jito_protos::searcher::searcher_service_client::SearcherServiceClient;
use jito_searcher_client::{
    get_searcher_client, token_authenticator::ClientInterceptor,
};
use std::io::{self, prelude::*, BufReader};
use std::num::ParseIntError;
use std::{
    io::{stdout, Write},
    fs::File,
};

struct Miner {
    pub keypairs: Vec<String>,
    pub jito_keypair: Keypair,
    pub priority_fee: u64,
    pub cluster: String,
    pub regions: Vec<String>,
    pub tip_account: Pubkey,
    pub jito_client_am: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>,
    pub jito_client_fr: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>,
    pub jito_client_ny: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>,
    pub jito_client_tk: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>,
}

#[derive(Parser, Debug)]
#[command(about, version)]
struct Args {
    #[arg(
        long,
        value_name = "NETWORK_URL",
        help = "Network address of your RPC provider",
    )]
    rpc: String,

    /// URL of the block engine.
    /// See: https://jito-labs.gitbook.io/mev/searcher-resources/block-engine#connection-details
    #[arg(long)]
    block_engine_url: String,

    /// Comma-separated list of regions to request cross-region data from.
    /// If no region specified, then default to the currently connected block engine's region.
    /// Details: https://jito-labs.gitbook.io/mev/searcher-services/recommendations#cross-region
    /// Available regions: https://jito-labs.gitbook.io/mev/searcher-resources/block-engine#connection-details
    #[arg(long, value_delimiter = ',')]
    regions: Vec<String>,

    #[arg(
        long,
        value_name = "PRIVATE_KEY",
        help = "Private key to use",
    )]
    private_key: Option<String>,

    #[arg(
        long,
        value_name = "JITO_PRIVATE_KEY",
        help = "Jito private key to use",
    )]
    jito_private_key: String,

    #[arg(
        long,
        value_name = "MICROLAMPORTS",
        help = "Number of microlamports to pay as priority fee per transaction",
        default_value = "0",
    )]
    priority_fee: u64,

    #[arg(
        long,
        value_name = "TIP_ACCOUNT",
        help = "Jito tip account public key",
    )]
    tip_account: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Fetch the Ore balance of an account")]
    Balance(BalanceArgs),

    #[command(about = "Fetch the distributable rewards of the busses")]
    Busses(BussesArgs),

    #[command(about = "Mine Ore using local compute")]
    Mine(MineArgs),

    #[command(about = "Claim available mining rewards")]
    Claim(ClaimArgs),

    #[command(about = "Fetch your balance of unclaimed mining rewards")]
    Rewards(RewardsArgs),

    #[command(about = "Fetch the treasury account and balance")]
    Treasury(TreasuryArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Initialize the program")]
    Initialize(InitializeArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Update the program admin authority")]
    UpdateAdmin(UpdateAdminArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Update the mining difficulty")]
    UpdateDifficulty(UpdateDifficultyArgs),
}

#[derive(Parser, Debug)]
struct BalanceArgs {
    #[arg(
        // long,
        value_name = "ADDRESS",
        help = "The address of the account to fetch the balance of"
    )]
    pub address: Option<String>,
}

#[derive(Parser, Debug)]
struct BussesArgs {}

#[derive(Parser, Debug)]
struct RewardsArgs {
    #[arg(
        // long,
        value_name = "ADDRESS",
        help = "The address of the account to fetch the rewards balance of"
    )]
    pub address: Option<String>,
}

#[derive(Parser, Debug)]
struct MineArgs {
    #[arg(
        long,
        short,
        value_name = "THREAD_COUNT",
        help = "The number of threads to dedicate to mining",
        default_value = "1"
    )]
    threads: u64,
}

#[derive(Parser, Debug)]
struct TreasuryArgs {}

#[derive(Parser, Debug)]
struct ClaimArgs {
    #[arg(
        // long,
        value_name = "AMOUNT",
        help = "The amount of rewards to claim. Defaults to max."
    )]
    amount: Option<f64>,

    #[arg(
        // long,
        value_name = "TOKEN_ACCOUNT_ADDRESS",
        help = "Token account to receive mining rewards."
    )]
    beneficiary: Option<String>,
}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct InitializeArgs {}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct UpdateAdminArgs {
    new_admin: String,
}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct UpdateDifficultyArgs {}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let jito_key_pair = Arc::new(read_keypair_file(&*args.jito_private_key).unwrap());
    let jito_key_pair_1 = read_keypair_file(&*args.jito_private_key).unwrap();
    let mut jito_client_am = get_searcher_client(&"https://amsterdam.mainnet.block-engine.jito.wtf", &jito_key_pair).await.expect("jito err");
    let mut jito_client_fr = get_searcher_client(&"https://frankfurt.mainnet.block-engine.jito.wtf", &jito_key_pair).await.expect("jito err");
    let mut jito_client_ny = get_searcher_client(&"https://ny.mainnet.block-engine.jito.wtf", &jito_key_pair).await.expect("jito err");
    let mut jito_client_tk = get_searcher_client(&"https://tokyo.mainnet.block-engine.jito.wtf", &jito_key_pair).await.expect("jito err");
    // let ore_key_pair = args.private_key;
    let tip_account = Pubkey::from_str(&*args.tip_account).unwrap();
    let mut keypairs: Vec<String> = Vec::new();

    let keypair_file = File::open("keys.txt").expect("open failed");
    let keypair_reader = BufReader::new(keypair_file);
    for keypair_path in keypair_reader.lines() {
        keypairs.push(keypair_path.unwrap());
    }

    // Initialize miner.
    let cluster = args.rpc;

    let miner = Arc::new(Miner::new(
        cluster.clone(),
        jito_key_pair_1,
        args.priority_fee,
        args.regions,
        keypairs,
        jito_client_am,
        jito_client_fr,
        jito_client_ny,
        jito_client_tk,
        tip_account,
    ));

    // Execute user command.
    match args.command {
        Commands::Balance(args) => {
            miner.balance(args.address).await;
        }
        Commands::Busses(_) => {
            miner.busses().await;
        }
        Commands::Rewards(args) => {
            miner.rewards(args.address).await;
        }
        Commands::Treasury(_) => {
            miner.treasury().await;
        }
        Commands::Mine(args) => {
            miner.mine(args.threads).await;
        }
        Commands::Claim(args) => {
            miner.claim(cluster, args.beneficiary, args.amount).await;
        }
        #[cfg(feature = "admin")]
        Commands::Initialize(_) => {
            miner.initialize().await;
        }
        #[cfg(feature = "admin")]
        Commands::UpdateAdmin(args) => {
            miner.update_admin(args.new_admin).await;
        }
        #[cfg(feature = "admin")]
        Commands::UpdateDifficulty(_) => {
            miner.update_difficulty().await;
        }
    }
}

impl Miner {
    pub fn new(cluster: String, jito_keypair: Keypair, priority_fee: u64, regions: Vec<String>, keypairs: Vec<String>, jito_client_am: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>, jito_client_fr: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>, jito_client_ny: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>, jito_client_tk: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>, tip_account: Pubkey) -> Self {
        Self {
            keypairs,
            jito_keypair,
            priority_fee,
            cluster,
            regions,
            jito_client_am,
            jito_client_fr,
            jito_client_ny,
            jito_client_tk,
            tip_account
        }
    }

    pub fn jito_keypair(&self) -> Keypair {
        return self.jito_keypair.insecure_clone();
    }

    // pub fn signer(&self) -> Keypair {
    //     match self.keypair.clone() {
    //         Some(filepath) => read_keypair_file(filepath).unwrap(),
    //         None => panic!("No keypair provided"),
    //     }
    // }

    pub fn regions(&self) -> Vec<String> {
        return self.regions.clone();
    }

    pub fn jito_client_am(&self) -> SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>> {
        return self.jito_client_am.clone();
    }

    pub fn jito_client_fr(&self) -> SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>> {
        return self.jito_client_fr.clone();
    }

    pub fn jito_client_ny(&self) -> SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>> {
        return self.jito_client_ny.clone();
    }

    pub fn jito_client_tk(&self) -> SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>> {
        return self.jito_client_tk.clone();
    }

    pub fn tip_account(&self) -> Pubkey {
        return self.tip_account.clone();
    }
}
